#![deny(warnings)]

use std::collections::HashMap;
use std::path::Path;

use rython_core::{EngineError, SchedulerHandle, Vec3};
use rython_modules::Module;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use kira::{
    manager::{backend::cpal::CpalBackend, AudioManager as KiraManager, AudioManagerSettings},
    sound::static_sound::{StaticSoundData, StaticSoundHandle, StaticSoundSettings},
    track::{TrackBuilder, TrackHandle},
    tween::Tween,
    Volume,
};

// ─── Configuration ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputMode {
    #[default]
    Stereo,
    #[serde(rename = "5.1")]
    Surround51,
    #[serde(rename = "5.1.2")]
    Surround512,
}

fn default_volume() -> f32 {
    1.0
}
fn default_max_sources() -> usize {
    32
}
fn default_max_radius() -> f32 {
    100.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    #[serde(default)]
    pub output_mode: OutputMode,
    #[serde(default = "default_volume")]
    pub master_volume: f32,
    #[serde(default = "default_volume")]
    pub sfx_volume: f32,
    #[serde(default = "default_volume")]
    pub dialogue_volume: f32,
    #[serde(default = "default_volume")]
    pub music_volume: f32,
    #[serde(default = "default_volume")]
    pub ambient_volume: f32,
    #[serde(default = "default_max_sources")]
    pub max_sources: usize,
    #[serde(default = "default_max_radius")]
    pub max_audible_radius: f32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            output_mode: OutputMode::Stereo,
            master_volume: 1.0,
            sfx_volume: 1.0,
            dialogue_volume: 1.0,
            music_volume: 1.0,
            ambient_volume: 1.0,
            max_sources: 32,
            max_audible_radius: 100.0,
        }
    }
}

// ─── Audio Category ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioCategory {
    Sfx,
    Dialogue,
    Music,
    Ambient,
}

impl AudioCategory {
    // Intentional method name — matches Python's lowercase category tags and
    // is used by the scripting bridge. We don't implement `std::str::FromStr`
    // because the method returns `Option`, not `Result`, to match how callers
    // treat unknown strings as a simple "not recognised" case.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "sfx" => Some(Self::Sfx),
            "dialogue" => Some(Self::Dialogue),
            "music" => Some(Self::Music),
            "ambient" => Some(Self::Ambient),
            _ => None,
        }
    }
}

// ─── Playback Handle ──────────────────────────────────────────────────────────

/// Opaque handle identifying a specific sound playback instance.
/// Handles for culled or rejected plays are non-zero but untracked —
/// calling stop() on them is a no-op (idempotent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlaybackHandle(u64);

impl PlaybackHandle {
    pub fn id(self) -> u64 {
        self.0
    }

    pub fn from_raw(id: u64) -> Self {
        Self(id)
    }
}

// ─── Listener State ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ListenerState {
    pub position: Vec3,
    pub forward: Vec3,
    pub up: Vec3,
}

impl Default for ListenerState {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            forward: Vec3::Z,
            up: Vec3::Y,
        }
    }
}

// ─── Ambient Group ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AmbientGroupDef {
    pub sound: String,
    pub positions: Vec<Vec3>,
    pub max_audible: usize,
}

// ─── Play Request ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PlayRequest {
    pub path: String,
    pub category: AudioCategory,
    pub position: Option<Vec3>,
    pub looping: bool,
}

// ─── Audio Error ──────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("Unsupported audio format: {extension}")]
    UnsupportedFormat { extension: String },

    #[error("Audio backend error: {0}")]
    Backend(String),

    #[error("Unknown category: {0}")]
    UnknownCategory(String),

    #[error("Play failed: {0}")]
    PlayFailed(String),
}

impl From<AudioError> for EngineError {
    fn from(e: AudioError) -> Self {
        EngineError::Audio(e.to_string())
    }
}

// ─── Kira inner state ─────────────────────────────────────────────────────────

struct KiraInner {
    manager: KiraManager<CpalBackend>,
    sfx_track: TrackHandle,
    dialogue_track: TrackHandle,
    music_track: TrackHandle,
    ambient_track: TrackHandle,
    sound_handles: HashMap<u64, StaticSoundHandle>,
}

impl KiraInner {
    fn new(config: &AudioConfig) -> Result<Self, EngineError> {
        let mut manager = KiraManager::<CpalBackend>::new(AudioManagerSettings::default())
            .map_err(|e| EngineError::Audio(format!("kira init: {e}")))?;

        let sfx_track = manager
            .add_sub_track(TrackBuilder::new())
            .map_err(|e| EngineError::Audio(e.to_string()))?;
        let dialogue_track = manager
            .add_sub_track(TrackBuilder::new())
            .map_err(|e| EngineError::Audio(e.to_string()))?;
        let music_track = manager
            .add_sub_track(TrackBuilder::new())
            .map_err(|e| EngineError::Audio(e.to_string()))?;
        let ambient_track = manager
            .add_sub_track(TrackBuilder::new())
            .map_err(|e| EngineError::Audio(e.to_string()))?;

        let inner = Self {
            manager,
            sfx_track,
            dialogue_track,
            music_track,
            ambient_track,
            sound_handles: HashMap::new(),
        };
        inner.apply_volumes(config);
        Ok(inner)
    }

    /// Applies master*category volume to each track. Takes &self because
    /// TrackHandle::set_volume only needs &self.
    fn apply_volumes(&self, config: &AudioConfig) {
        let tween = Tween::default();
        let _ = self.sfx_track.set_volume(
            Volume::Amplitude((config.master_volume * config.sfx_volume) as f64),
            tween,
        );
        let _ = self.dialogue_track.set_volume(
            Volume::Amplitude((config.master_volume * config.dialogue_volume) as f64),
            tween,
        );
        let _ = self.music_track.set_volume(
            Volume::Amplitude((config.master_volume * config.music_volume) as f64),
            tween,
        );
        let _ = self.ambient_track.set_volume(
            Volume::Amplitude((config.master_volume * config.ambient_volume) as f64),
            tween,
        );
    }

    fn play_sound(
        &mut self,
        id: u64,
        request: &PlayRequest,
        effective_vol: f32,
    ) -> Result<(), String> {
        // Build settings in a block so the track borrow ends before manager.play().
        let settings = {
            let track = match request.category {
                AudioCategory::Sfx => &self.sfx_track,
                AudioCategory::Dialogue => &self.dialogue_track,
                AudioCategory::Music => &self.music_track,
                AudioCategory::Ambient => &self.ambient_track,
            };
            let base = StaticSoundSettings::new()
                .volume(Volume::Amplitude(effective_vol as f64))
                .output_destination(track);
            // `..` (RangeFull) → loop entire sound
            if request.looping {
                base.loop_region(..)
            } else {
                base
            }
        };

        let data = StaticSoundData::from_file(&request.path, settings)
            .map_err(|e| format!("audio load '{}' failed: {e}", request.path))?;
        let handle = self
            .manager
            .play(data)
            .map_err(|e| format!("audio play '{}' failed: {e}", request.path))?;
        self.sound_handles.insert(id, handle);
        Ok(())
    }

    fn stop_sound(&mut self, id: u64) {
        if let Some(mut handle) = self.sound_handles.remove(&id) {
            let _ = handle.stop(Tween::default());
        }
    }

    fn stop_all(&mut self) {
        let ids: Vec<u64> = self.sound_handles.keys().copied().collect();
        for id in ids {
            if let Some(mut handle) = self.sound_handles.remove(&id) {
                let _ = handle.stop(Tween::default());
            }
        }
    }
}

// ─── Audio Manager ────────────────────────────────────────────────────────────

pub struct AudioManager {
    config: AudioConfig,
    kira: Option<KiraInner>,
    next_id: u64,
    listener: ListenerState,
    handle_categories: HashMap<u64, AudioCategory>,
    ambient_groups: HashMap<String, AmbientGroupDef>,
    active_count: usize,
    /// Set when all sounds stop; causes kira to reinitialize on the next play()
    /// call to avoid CPAL backend state corruption from stop+restart cycles.
    needs_reinit: bool,
}

impl AudioManager {
    pub fn new(config: AudioConfig) -> Self {
        Self {
            config,
            kira: None,
            next_id: 1,
            listener: ListenerState::default(),
            handle_categories: HashMap::new(),
            ambient_groups: HashMap::new(),
            active_count: 0,
            needs_reinit: false,
        }
    }

    pub fn with_default_config() -> Self {
        Self::new(AudioConfig::default())
    }

    pub fn ensure_initialized(&mut self) -> Result<(), EngineError> {
        if self.kira.is_none() {
            self.kira = Some(KiraInner::new(&self.config)?);
        }
        Ok(())
    }

    // ─── Pure logic (always runs, no audio hardware needed) ──────────────────

    /// Effective volume for a category after master scaling.
    pub fn effective_volume(&self, category: AudioCategory) -> f32 {
        let cat_vol = match category {
            AudioCategory::Sfx => self.config.sfx_volume,
            AudioCategory::Dialogue => self.config.dialogue_volume,
            AudioCategory::Music => self.config.music_volume,
            AudioCategory::Ambient => self.config.ambient_volume,
        };
        self.config.master_volume * cat_vol
    }

    /// True if the given world position is within the max audible radius.
    pub fn is_within_range(&self, pos: Vec3) -> bool {
        (pos - self.listener.position).length() <= self.config.max_audible_radius
    }

    /// Validates that a file extension is a supported audio format.
    pub fn check_format(path: &str) -> Result<(), AudioError> {
        let ext = Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        match ext.as_str() {
            "wav" | "ogg" | "flac" | "mp3" => Ok(()),
            other => Err(AudioError::UnsupportedFormat {
                extension: other.to_string(),
            }),
        }
    }

    /// True if another sound can be started (below max_sources limit).
    pub fn can_play_more(&self) -> bool {
        self.active_count < self.config.max_sources
    }

    /// Returns the indices of the `max_audible` emitters closest to the listener.
    pub fn cull_ambient_group(&self, group: &AmbientGroupDef) -> Vec<usize> {
        let mut indexed: Vec<(usize, f32)> = group
            .positions
            .iter()
            .enumerate()
            .map(|(i, &p)| (i, (p - self.listener.position).length()))
            .collect();
        indexed.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        indexed.truncate(group.max_audible);
        indexed.into_iter().map(|(i, _)| i).collect()
    }

    // ─── Public API ──────────────────────────────────────────────────────────

    pub fn set_master_volume(&mut self, volume: f32) {
        // f32::clamp is unspecified for NaN inputs, so we normalize explicitly
        // and reject NaN/non-finite values rather than forwarding them to kira.
        let clean = if volume.is_finite() {
            volume.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.config.master_volume = clean;
        if let Some(ref kira) = self.kira {
            kira.apply_volumes(&self.config);
        }
    }

    pub fn set_volume(&mut self, category_str: &str, volume: f32) -> Result<(), AudioError> {
        let v = if volume.is_finite() {
            volume.clamp(0.0, 1.0)
        } else {
            0.0
        };
        match category_str {
            "sfx" => self.config.sfx_volume = v,
            "dialogue" => self.config.dialogue_volume = v,
            "music" => self.config.music_volume = v,
            "ambient" => self.config.ambient_volume = v,
            other => {
                return Err(AudioError::UnknownCategory(other.to_string()));
            }
        }
        if let Some(ref kira) = self.kira {
            kira.apply_volumes(&self.config);
        }
        Ok(())
    }

    pub fn set_listener(&mut self, position: Vec3, forward: Vec3, up: Vec3) {
        self.listener = ListenerState {
            position,
            forward,
            up,
        };
    }

    /// Play a sound. Returns a handle that uniquely identifies this playback.
    /// Sounds culled by distance or rejected by max_sources return a valid
    /// non-zero handle but are silently not played.
    pub fn play(&mut self, request: PlayRequest) -> Result<PlaybackHandle, AudioError> {
        Self::check_format(&request.path)?;

        // Distance culling for spatial sounds — silent rejection, no error.
        if let Some(pos) = request.position {
            if !self.is_within_range(pos) {
                let id = self.next_id;
                self.next_id += 1;
                return Ok(PlaybackHandle(id));
            }
        }

        // Source-limit enforcement — reject beyond max_sources.
        if !self.can_play_more() {
            let id = self.next_id;
            self.next_id += 1;
            return Ok(PlaybackHandle(id));
        }

        // Reinitialize the kira backend if all sounds were previously stopped —
        // CPAL can get into a corrupted state after a stop+restart cycle on Linux.
        if self.needs_reinit {
            self.needs_reinit = false;
            if self.kira.is_some() {
                self.kira = None;
                // Err case ignored: audio stays silent rather than crashing.
                if let Ok(inner) = KiraInner::new(&self.config) {
                    inner.apply_volumes(&self.config);
                    self.kira = Some(inner);
                }
            }
        }

        let id = self.next_id;
        self.next_id += 1;

        let vol = self.effective_volume(request.category);
        if let Some(ref mut kira) = self.kira {
            // If kira fails to load or play the sound, do NOT increment
            // active_count or insert into handle_categories — otherwise the
            // counter drifts above the real number of playing sounds and
            // can_play_more() would lie forever.
            if let Err(e) = kira.play_sound(id, &request, vol) {
                return Err(AudioError::PlayFailed(e));
            }
        }
        self.handle_categories.insert(id, request.category);
        self.active_count += 1;

        Ok(PlaybackHandle(id))
    }

    /// Stop a sound by handle. Idempotent — calling on an already-stopped or
    /// untracked handle returns Ok(()) without error.
    pub fn stop(&mut self, handle: PlaybackHandle) -> Result<(), AudioError> {
        let id = handle.0;
        if self.handle_categories.remove(&id).is_some() {
            self.active_count = self.active_count.saturating_sub(1);
            if self.active_count == 0 {
                self.needs_reinit = true;
            }
            if let Some(ref mut kira) = self.kira {
                kira.stop_sound(id);
            }
        }
        Ok(())
    }

    /// Stop all sounds in a category.
    pub fn stop_category(&mut self, category_str: &str) -> Result<(), AudioError> {
        let category = AudioCategory::from_str(category_str)
            .ok_or_else(|| AudioError::UnknownCategory(category_str.to_string()))?;

        let ids: Vec<u64> = self
            .handle_categories
            .iter()
            .filter(|(_, &c)| c == category)
            .map(|(&id, _)| id)
            .collect();

        for id in ids {
            self.handle_categories.remove(&id);
            self.active_count = self.active_count.saturating_sub(1);
            if let Some(ref mut kira) = self.kira {
                kira.stop_sound(id);
            }
        }
        if self.active_count == 0 {
            self.needs_reinit = true;
        }
        Ok(())
    }

    /// Register an ambient sound group with multiple emitter positions.
    pub fn register_ambient_group(
        &mut self,
        name: String,
        sound: String,
        positions: Vec<Vec3>,
        max_audible: usize,
    ) {
        self.ambient_groups.insert(
            name,
            AmbientGroupDef {
                sound,
                positions,
                max_audible,
            },
        );
    }
}

// ─── Module trait ─────────────────────────────────────────────────────────────

impl Module for AudioManager {
    fn name(&self) -> &str {
        "audio"
    }

    fn on_load(&mut self, _scheduler: &dyn SchedulerHandle) -> Result<(), EngineError> {
        let inner = KiraInner::new(&self.config)?;
        self.kira = Some(inner);
        Ok(())
    }

    fn on_unload(&mut self, _scheduler: &dyn SchedulerHandle) -> Result<(), EngineError> {
        if let Some(ref mut kira) = self.kira {
            kira.stop_all();
        }
        self.kira = None;
        self.handle_categories.clear();
        self.active_count = 0;
        Ok(())
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn manager() -> AudioManager {
        AudioManager::with_default_config()
    }

    fn manager_with(config: AudioConfig) -> AudioManager {
        AudioManager::new(config)
    }

    struct NoopScheduler;
    impl SchedulerHandle for NoopScheduler {
        fn submit_sequential(
            &self,
            _f: Box<dyn FnOnce() -> Result<(), EngineError> + Send + 'static>,
            _priority: rython_core::Priority,
            _owner: rython_core::OwnerId,
        ) {
        }
        fn cancel_owned(&self, _owner: rython_core::OwnerId) {}
    }

    // ── T-AUD-05: Master volume scaling ──────────────────────────────────────

    #[test]
    fn t_aud_05_master_volume_scaling() {
        let mut m = manager();
        m.config.master_volume = 0.5;
        m.config.sfx_volume = 1.0;
        let vol = m.effective_volume(AudioCategory::Sfx);
        assert!((vol - 0.5).abs() < f32::EPSILON, "expected 0.5, got {vol}");
    }

    // ── T-AUD-06: Category volume independence ────────────────────────────────

    #[test]
    fn t_aud_06_category_volume_independence() {
        let m = manager_with(AudioConfig {
            sfx_volume: 0.8,
            music_volume: 0.3,
            master_volume: 1.0,
            ..Default::default()
        });
        let sfx_vol = m.effective_volume(AudioCategory::Sfx);
        let music_vol = m.effective_volume(AudioCategory::Music);
        assert!((sfx_vol - 0.8).abs() < f32::EPSILON, "sfx={sfx_vol}");
        assert!((music_vol - 0.3).abs() < f32::EPSILON, "music={music_vol}");
    }

    // ── T-AUD-07: Master volume zero mutes all ────────────────────────────────

    #[test]
    fn t_aud_07_master_volume_zero_mutes_all() {
        let m = manager_with(AudioConfig {
            master_volume: 0.0,
            ..Default::default()
        });
        for cat in [
            AudioCategory::Sfx,
            AudioCategory::Dialogue,
            AudioCategory::Music,
            AudioCategory::Ambient,
        ] {
            assert_eq!(m.effective_volume(cat), 0.0, "category {cat:?} not muted");
        }
    }

    // ── T-AUD-09: Distance culling beyond radius ──────────────────────────────

    #[test]
    fn t_aud_09_distance_culling_beyond_radius() {
        let m = manager_with(AudioConfig {
            max_audible_radius: 50.0,
            ..Default::default()
        });
        assert!(!m.is_within_range(Vec3::new(100.0, 0.0, 0.0)));
    }

    // ── T-AUD-10: Distance culling within radius ──────────────────────────────

    #[test]
    fn t_aud_10_distance_culling_within_radius() {
        let m = manager_with(AudioConfig {
            max_audible_radius: 50.0,
            ..Default::default()
        });
        assert!(m.is_within_range(Vec3::new(30.0, 0.0, 0.0)));
    }

    // ── T-AUD-08: Max sources enforcement ────────────────────────────────────

    #[test]
    fn t_aud_08_max_sources_enforcement() {
        let mut m = manager_with(AudioConfig {
            max_sources: 4,
            ..Default::default()
        });
        // Manually fill the source slots
        for i in 1u64..=4 {
            m.handle_categories.insert(i, AudioCategory::Sfx);
            m.active_count += 1;
            m.next_id = i + 1;
        }
        assert!(!m.can_play_more());

        // 5th play is rejected silently (valid non-zero handle returned)
        let h5 = m
            .play(PlayRequest {
                path: "test.wav".into(),
                category: AudioCategory::Sfx,
                position: None,
                looping: false,
            })
            .unwrap();
        assert_ne!(h5.id(), 0);
        assert_eq!(
            m.active_count, 4,
            "active count must not exceed max_sources"
        );
    }

    // ── T-AUD-03: Idempotent stop ─────────────────────────────────────────────

    #[test]
    fn t_aud_03_stop_handle_idempotent() {
        let mut m = manager();
        let dead = PlaybackHandle(999);
        assert!(m.stop(dead).is_ok());
        assert!(m.stop(dead).is_ok());
    }

    // ── T-AUD-04: Stop category ────────────────────────────────────────────────

    #[test]
    fn t_aud_04_stop_category() {
        let mut m = manager();

        // Inject tracked handles without audio hardware
        for id in [1u64, 2, 3] {
            m.handle_categories.insert(id, AudioCategory::Sfx);
        }
        for id in [4u64, 5] {
            m.handle_categories.insert(id, AudioCategory::Music);
        }
        m.active_count = 5;

        m.stop_category("sfx").unwrap();

        assert_eq!(m.active_count, 2, "2 music sounds should remain");
        for id in [1u64, 2, 3] {
            assert!(
                !m.handle_categories.contains_key(&id),
                "sfx {id} should be gone"
            );
        }
        assert!(m.handle_categories.contains_key(&4));
        assert!(m.handle_categories.contains_key(&5));
    }

    // ── T-AUD-02: Play returns non-zero handle ────────────────────────────────

    #[test]
    fn t_aud_02_play_returns_nonzero_handle() {
        let mut m = manager();
        // kira is None (no hardware) — sound doesn't play but handle is allocated
        let h = m
            .play(PlayRequest {
                path: "sound.wav".into(),
                category: AudioCategory::Sfx,
                position: None,
                looping: false,
            })
            .unwrap();
        assert_ne!(h.id(), 0);
    }

    // ── T-AUD-14: Ambient group culling ───────────────────────────────────────

    #[test]
    fn t_aud_14_ambient_group_culling() {
        let m = manager();
        let group = AmbientGroupDef {
            sound: "birds.ogg".into(),
            positions: vec![
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(5.0, 0.0, 0.0),
                Vec3::new(10.0, 0.0, 0.0),
                Vec3::new(20.0, 0.0, 0.0),
                Vec3::new(50.0, 0.0, 0.0),
            ],
            max_audible: 2,
        };
        // Listener at origin — 2 closest should be indices 0 and 1
        let active = m.cull_ambient_group(&group);
        assert_eq!(active.len(), 2);
        assert!(active.contains(&0));
        assert!(active.contains(&1));
    }

    #[test]
    fn t_aud_14_ambient_culling_moves_with_listener() {
        let mut m = manager();
        let group = AmbientGroupDef {
            sound: "birds.ogg".into(),
            positions: vec![
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(100.0, 0.0, 0.0),
                Vec3::new(102.0, 0.0, 0.0),
            ],
            max_audible: 2,
        };
        // Listener near origin — index 0 closest
        let active = m.cull_ambient_group(&group);
        assert!(active.contains(&0));

        // Move listener to far cluster
        m.set_listener(Vec3::new(101.0, 0.0, 0.0), Vec3::Z, Vec3::Y);
        let active2 = m.cull_ambient_group(&group);
        assert!(
            !active2.contains(&0),
            "near emitter should be culled after move"
        );
        assert!(active2.contains(&1) || active2.contains(&2));
    }

    // ── T-AUD-15: Unsupported format ──────────────────────────────────────────

    #[test]
    fn t_aud_15_unsupported_format_returns_error() {
        assert!(AudioManager::check_format("sound.aac").is_err());
    }

    #[test]
    fn t_aud_15_supported_formats_accepted() {
        for ext in ["wav", "ogg", "flac", "mp3"] {
            assert!(
                AudioManager::check_format(&format!("sound.{ext}")).is_ok(),
                "{ext} should be accepted"
            );
        }
    }

    // ── T-AUD-11: Listener position update ───────────────────────────────────

    #[test]
    fn t_aud_11_listener_position_update() {
        let mut m = manager();
        m.set_listener(Vec3::new(10.0, 0.0, 0.0), Vec3::Z, Vec3::Y);
        assert_eq!(m.listener.position, Vec3::new(10.0, 0.0, 0.0));
        // Sound at same position as listener — distance = 0, within any radius
        assert!(m.is_within_range(Vec3::new(10.0, 0.0, 0.0)));
    }

    // ── T-AUD-01: Module name ─────────────────────────────────────────────────

    #[test]
    fn t_aud_01_module_name() {
        assert_eq!(manager().name(), "audio");
    }

    // ── T-AUD-09 boundary: exactly at radius ─────────────────────────────────

    #[test]
    fn t_aud_09_distance_exactly_at_radius() {
        let m = manager_with(AudioConfig {
            max_audible_radius: 50.0,
            ..Default::default()
        });
        // Exactly at radius — should be audible (<=)
        assert!(m.is_within_range(Vec3::new(50.0, 0.0, 0.0)));
    }

    // ── T-AUD-16: set_master_volume clamping ─────────────────────────────────

    #[test]
    fn t_aud_16_set_master_volume_clamps_above_one() {
        let mut m = manager();
        m.set_master_volume(2.0);
        assert_eq!(m.config.master_volume, 1.0);
    }

    #[test]
    fn t_aud_16_set_master_volume_clamps_below_zero() {
        let mut m = manager();
        m.set_master_volume(-0.5);
        assert_eq!(m.config.master_volume, 0.0);
    }

    // ── T-AUD-17: set_volume category clamping ────────────────────────────────

    #[test]
    fn t_aud_17_set_volume_clamps_above_one() {
        let mut m = manager();
        m.set_volume("sfx", 5.0).unwrap();
        assert_eq!(m.config.sfx_volume, 1.0);
    }

    #[test]
    fn t_aud_17_set_volume_clamps_below_zero() {
        let mut m = manager();
        m.set_volume("music", -1.0).unwrap();
        assert_eq!(m.config.music_volume, 0.0);
    }

    // ── T-AUD-18: set_volume unknown category returns error ───────────────────

    #[test]
    fn t_aud_18_set_volume_unknown_category_returns_error() {
        let mut m = manager();
        let err = m.set_volume("boom", 1.0).unwrap_err();
        assert!(matches!(err, AudioError::UnknownCategory(_)));
    }

    // ── T-AUD-19: stop_category unknown category returns error ────────────────

    #[test]
    fn t_aud_19_stop_category_unknown_returns_error() {
        let mut m = manager();
        let err = m.stop_category("boom").unwrap_err();
        assert!(matches!(err, AudioError::UnknownCategory(_)));
    }

    // ── T-AUD-20: play increments active_count ────────────────────────────────

    #[test]
    fn t_aud_20_play_increments_active_count() {
        let mut m = manager();
        assert_eq!(m.active_count, 0);
        m.play(PlayRequest {
            path: "sound.wav".into(),
            category: AudioCategory::Sfx,
            position: None,
            looping: false,
        })
        .unwrap();
        assert_eq!(m.active_count, 1);
    }

    // ── T-AUD-21: stop decrements active_count ───────────────────────────────

    #[test]
    fn t_aud_21_stop_decrements_active_count() {
        let mut m = manager();
        let h = m
            .play(PlayRequest {
                path: "sound.wav".into(),
                category: AudioCategory::Sfx,
                position: None,
                looping: false,
            })
            .unwrap();
        assert_eq!(m.active_count, 1);
        m.stop(h).unwrap();
        assert_eq!(m.active_count, 0);
    }

    // ── T-AUD-22: stop on untracked handle does not change active_count ───────

    #[test]
    fn t_aud_22_stop_untracked_handle_is_noop() {
        let mut m = manager();
        m.active_count = 3;
        m.stop(PlaybackHandle::from_raw(9999)).unwrap();
        assert_eq!(
            m.active_count, 3,
            "untracked stop must not change active_count"
        );
    }

    // ── T-AUD-23: needs_reinit set when last sound stopped ────────────────────

    #[test]
    fn t_aud_23_needs_reinit_set_when_last_sound_stopped() {
        let mut m = manager();
        let h = m
            .play(PlayRequest {
                path: "sound.wav".into(),
                category: AudioCategory::Sfx,
                position: None,
                looping: false,
            })
            .unwrap();
        assert!(!m.needs_reinit);
        m.stop(h).unwrap();
        assert!(
            m.needs_reinit,
            "needs_reinit should be set after last sound stops"
        );
    }

    // ── T-AUD-24: needs_reinit not set while other sounds remain ─────────────

    #[test]
    fn t_aud_24_needs_reinit_not_set_while_sounds_remain() {
        let mut m = manager();
        let h1 = m
            .play(PlayRequest {
                path: "a.wav".into(),
                category: AudioCategory::Sfx,
                position: None,
                looping: false,
            })
            .unwrap();
        m.play(PlayRequest {
            path: "b.wav".into(),
            category: AudioCategory::Sfx,
            position: None,
            looping: false,
        })
        .unwrap();
        m.stop(h1).unwrap();
        assert!(
            !m.needs_reinit,
            "needs_reinit should be clear while sounds remain"
        );
    }

    // ── T-AUD-25: spatial play beyond radius skips active_count ──────────────

    #[test]
    fn t_aud_25_spatial_play_beyond_radius_skips_active_count() {
        let mut m = manager_with(AudioConfig {
            max_audible_radius: 10.0,
            ..Default::default()
        });
        let h = m
            .play(PlayRequest {
                path: "sound.wav".into(),
                category: AudioCategory::Sfx,
                position: Some(Vec3::new(50.0, 0.0, 0.0)),
                looping: false,
            })
            .unwrap();
        assert_ne!(h.id(), 0, "culled play still returns non-zero handle");
        assert_eq!(
            m.active_count, 0,
            "culled play must not increment active_count"
        );
    }

    // ── T-AUD-26: AudioCategory::from_str all valid values ───────────────────

    #[test]
    fn t_aud_26_category_from_str_valid() {
        assert_eq!(AudioCategory::from_str("sfx"), Some(AudioCategory::Sfx));
        assert_eq!(
            AudioCategory::from_str("dialogue"),
            Some(AudioCategory::Dialogue)
        );
        assert_eq!(AudioCategory::from_str("music"), Some(AudioCategory::Music));
        assert_eq!(
            AudioCategory::from_str("ambient"),
            Some(AudioCategory::Ambient)
        );
    }

    // ── T-AUD-27: AudioCategory::from_str unknown returns None ───────────────

    #[test]
    fn t_aud_27_category_from_str_unknown_returns_none() {
        assert_eq!(AudioCategory::from_str("boom"), None);
        assert_eq!(AudioCategory::from_str(""), None);
        assert_eq!(AudioCategory::from_str("SFX"), None, "case sensitive");
    }

    // ── T-AUD-28: PlaybackHandle from_raw / id() roundtrip ───────────────────

    #[test]
    fn t_aud_28_playback_handle_roundtrip() {
        let h = PlaybackHandle::from_raw(42);
        assert_eq!(h.id(), 42);
        let h0 = PlaybackHandle::from_raw(0);
        assert_eq!(h0.id(), 0);
    }

    // ── T-AUD-29: AudioConfig default values ─────────────────────────────────

    #[test]
    fn t_aud_29_audio_config_defaults() {
        let c = AudioConfig::default();
        assert_eq!(c.output_mode, OutputMode::Stereo);
        assert!((c.master_volume - 1.0).abs() < f32::EPSILON);
        assert!((c.sfx_volume - 1.0).abs() < f32::EPSILON);
        assert!((c.dialogue_volume - 1.0).abs() < f32::EPSILON);
        assert!((c.music_volume - 1.0).abs() < f32::EPSILON);
        assert!((c.ambient_volume - 1.0).abs() < f32::EPSILON);
        assert_eq!(c.max_sources, 32);
        assert!((c.max_audible_radius - 100.0).abs() < f32::EPSILON);
    }

    // ── T-AUD-30: OutputMode default is Stereo ───────────────────────────────

    #[test]
    fn t_aud_30_output_mode_default_is_stereo() {
        assert_eq!(OutputMode::default(), OutputMode::Stereo);
    }

    // ── T-AUD-31: ambient cull when max_audible exceeds positions count ───────

    #[test]
    fn t_aud_31_ambient_cull_max_audible_exceeds_positions() {
        let m = manager();
        let group = AmbientGroupDef {
            sound: "wind.ogg".into(),
            positions: vec![Vec3::new(1.0, 0.0, 0.0), Vec3::new(2.0, 0.0, 0.0)],
            max_audible: 10,
        };
        let active = m.cull_ambient_group(&group);
        assert_eq!(
            active.len(),
            2,
            "should return all positions when max_audible > len"
        );
        assert!(active.contains(&0));
        assert!(active.contains(&1));
    }

    // ── T-AUD-32: ambient cull with zero positions ────────────────────────────

    #[test]
    fn t_aud_32_ambient_cull_zero_positions() {
        let m = manager();
        let group = AmbientGroupDef {
            sound: "wind.ogg".into(),
            positions: vec![],
            max_audible: 3,
        };
        assert!(m.cull_ambient_group(&group).is_empty());
    }

    // ── T-AUD-33: register_ambient_group stores the group ────────────────────

    #[test]
    fn t_aud_33_register_ambient_group() {
        let mut m = manager();
        m.register_ambient_group(
            "forest".to_string(),
            "birds.ogg".to_string(),
            vec![Vec3::new(10.0, 0.0, 0.0)],
            2,
        );
        let g = m
            .ambient_groups
            .get("forest")
            .expect("group should be stored");
        assert_eq!(g.sound, "birds.ogg");
        assert_eq!(g.positions.len(), 1);
        assert_eq!(g.max_audible, 2);
    }

    // ── T-AUD-34: on_unload clears handle state without audio hardware ────────

    #[test]
    fn t_aud_34_on_unload_clears_state_no_hardware() {
        let mut m = manager();
        m.handle_categories.insert(1, AudioCategory::Sfx);
        m.handle_categories.insert(2, AudioCategory::Music);
        m.active_count = 2;

        m.on_unload(&NoopScheduler).unwrap();

        assert_eq!(m.active_count, 0);
        assert!(m.handle_categories.is_empty());
        assert!(m.kira.is_none());
    }

    // ── T-AUD-35: check_format accepts uppercase extensions ──────────────────

    #[test]
    fn t_aud_35_check_format_uppercase_accepted() {
        assert!(AudioManager::check_format("SOUND.WAV").is_ok());
        assert!(AudioManager::check_format("music.OGG").is_ok());
        assert!(AudioManager::check_format("track.MP3").is_ok());
        assert!(AudioManager::check_format("effect.FLAC").is_ok());
    }

    // ── T-AUD-36: check_format path without extension returns error ───────────

    #[test]
    fn t_aud_36_check_format_no_extension_returns_error() {
        let err = AudioManager::check_format("soundfile").unwrap_err();
        assert!(
            matches!(err, AudioError::UnsupportedFormat { ref extension } if extension.is_empty()),
            "expected empty extension in error, got: {err}"
        );
    }

    // ── T-AUD-37: play with unsupported format returns error ──────────────────

    #[test]
    fn t_aud_37_play_unsupported_format_returns_error() {
        let mut m = manager();
        let err = m
            .play(PlayRequest {
                path: "sound.aac".into(),
                category: AudioCategory::Sfx,
                position: None,
                looping: false,
            })
            .unwrap_err();
        assert!(matches!(err, AudioError::UnsupportedFormat { .. }));
        assert_eq!(
            m.active_count, 0,
            "active_count must not change on format error"
        );
    }

    // ── T-AUD-38: stop_category sets needs_reinit when all sounds cleared ─────

    #[test]
    fn t_aud_38_stop_category_sets_needs_reinit_when_all_cleared() {
        let mut m = manager();
        m.handle_categories.insert(1, AudioCategory::Sfx);
        m.handle_categories.insert(2, AudioCategory::Sfx);
        m.active_count = 2;

        m.stop_category("sfx").unwrap();

        assert_eq!(m.active_count, 0);
        assert!(
            m.needs_reinit,
            "needs_reinit should be set after stop_category empties all sounds"
        );
    }

    // ── T-AUD-39: handle IDs are sequential and unique ────────────────────────

    #[test]
    fn t_aud_39_handle_ids_sequential() {
        let mut m = manager();
        let h1 = m
            .play(PlayRequest {
                path: "a.wav".into(),
                category: AudioCategory::Sfx,
                position: None,
                looping: false,
            })
            .unwrap();
        let h2 = m
            .play(PlayRequest {
                path: "b.wav".into(),
                category: AudioCategory::Sfx,
                position: None,
                looping: false,
            })
            .unwrap();
        assert_ne!(h1.id(), 0);
        assert_ne!(h2.id(), 0);
        assert_ne!(h1.id(), h2.id(), "IDs must be unique");
        assert_eq!(h2.id(), h1.id() + 1, "IDs should increment by 1");
    }

    // ── T-AUD-40: effective_volume for dialogue and ambient ───────────────────

    #[test]
    fn t_aud_40_effective_volume_dialogue_and_ambient() {
        let m = manager_with(AudioConfig {
            master_volume: 0.8,
            dialogue_volume: 0.5,
            ambient_volume: 0.25,
            ..Default::default()
        });
        let dv = m.effective_volume(AudioCategory::Dialogue);
        let av = m.effective_volume(AudioCategory::Ambient);
        assert!((dv - 0.4).abs() < 1e-6, "dialogue: expected 0.4, got {dv}");
        assert!((av - 0.2).abs() < 1e-6, "ambient: expected 0.2, got {av}");
    }

    // ── T-AUD-41: exceed max_sources returns Ok (graceful rejection) ────────

    #[test]
    fn t_aud_41_exceed_max_sources_graceful() {
        let mut m = manager_with(AudioConfig {
            max_sources: 2,
            ..Default::default()
        });
        let h1 = m
            .play(PlayRequest {
                path: "a.wav".into(),
                category: AudioCategory::Sfx,
                position: None,
                looping: false,
            })
            .unwrap();
        let h2 = m
            .play(PlayRequest {
                path: "b.wav".into(),
                category: AudioCategory::Sfx,
                position: None,
                looping: false,
            })
            .unwrap();
        assert_eq!(m.active_count, 2);
        assert_ne!(h1.id(), 0);
        assert_ne!(h2.id(), 0);

        // 3rd play — exceeds max_sources, should return Ok with a non-zero handle
        let h3 = m
            .play(PlayRequest {
                path: "c.wav".into(),
                category: AudioCategory::Sfx,
                position: None,
                looping: false,
            })
            .unwrap();
        assert_ne!(
            h3.id(),
            0,
            "silent allocation must still return non-zero handle"
        );
        assert_eq!(m.active_count, 2, "active_count must stay at max_sources");
    }

    // ── T-AUD-42: zero-distance spatial (exactly at listener) ────────────────

    #[test]
    fn t_aud_42_zero_distance_spatial() {
        let m = manager();
        // Listener defaults to origin — Vec3::ZERO should be within range
        assert!(
            m.is_within_range(Vec3::ZERO),
            "position at listener origin must be within range"
        );
        // Epsilon distance from listener — still within range
        assert!(
            m.is_within_range(Vec3::new(0.0001, 0.0, 0.0)),
            "epsilon distance from listener must be within range"
        );
    }

    // ── T-AUD-43: rapid play/stop cycle ──────────────────────────────────────

    #[test]
    fn t_aud_43_rapid_play_stop_cycle() {
        let mut m = manager();
        for _ in 0..100 {
            let h = m
                .play(PlayRequest {
                    path: "sound.wav".into(),
                    category: AudioCategory::Sfx,
                    position: None,
                    looping: false,
                })
                .unwrap();
            m.stop(h).unwrap();
        }
        assert_eq!(
            m.active_count, 0,
            "active_count must be 0 after all play/stop cycles"
        );
    }

    // ── T-AUD-44: volume above 1.0 via direct config (no clamping) ──────────

    #[test]
    fn t_aud_44_volume_clamping_above_one() {
        // effective_volume does raw multiplication — no clamping.
        // set_master_volume clamps, but direct config assignment does not.
        let m = manager_with(AudioConfig {
            master_volume: 1.5,
            sfx_volume: 1.0,
            ..Default::default()
        });
        let vol = m.effective_volume(AudioCategory::Sfx);
        // effective_volume returns master * category with no clamping
        assert!(
            (vol - 1.5).abs() < f32::EPSILON,
            "effective_volume should return raw product 1.5, got {vol}"
        );
    }

    // ── T-AUD-45: negative volume via direct config ──────────────────────────

    #[test]
    fn t_aud_45_volume_clamping_negative() {
        let m = manager_with(AudioConfig {
            master_volume: -0.5,
            sfx_volume: 1.0,
            ..Default::default()
        });
        let vol = m.effective_volume(AudioCategory::Sfx);
        // effective_volume returns raw product — no clamping on direct config
        assert!(
            (vol - (-0.5)).abs() < f32::EPSILON,
            "effective_volume should return raw product -0.5, got {vol}"
        );
    }

    // ── T-AUD-46: AudioCategory::from_str all valid values ───────────────────

    #[test]
    fn t_aud_46_category_from_str_all_valid() {
        assert!(AudioCategory::from_str("sfx").is_some());
        assert!(AudioCategory::from_str("dialogue").is_some());
        assert!(AudioCategory::from_str("music").is_some());
        assert!(AudioCategory::from_str("ambient").is_some());
    }

    // ── T-AUD-47: AudioCategory::from_str invalid inputs ─────────────────────

    #[test]
    fn t_aud_47_category_from_str_invalid() {
        assert!(AudioCategory::from_str("").is_none(), "empty string");
        assert!(AudioCategory::from_str("SFX").is_none(), "uppercase");
        assert!(
            AudioCategory::from_str("unknown").is_none(),
            "unknown category"
        );
        assert!(AudioCategory::from_str("sfx ").is_none(), "trailing space");
    }

    // ── T-AUD-48: NaN master volume does not propagate to config ─────────────
    #[test]
    fn t_aud_48_nan_master_volume_rejected() {
        let mut m = manager();
        m.set_master_volume(f32::NAN);
        assert!(
            m.config.master_volume.is_finite(),
            "NaN must be normalized, got {}",
            m.config.master_volume
        );
    }

    // ── T-AUD-49: set_volume rejects infinite/NaN values ─────────────────────
    #[test]
    fn t_aud_49_nan_category_volume_rejected() {
        let mut m = manager();
        m.set_volume("sfx", f32::INFINITY).unwrap();
        assert!(m.config.sfx_volume.is_finite());
        m.set_volume("music", f32::NAN).unwrap();
        assert!(m.config.music_volume.is_finite());
    }

    // ── Hardware tests — require a real audio device ──────────────────────────

    #[test]
    #[ignore = "requires audio hardware: run with --include-ignored"]
    fn t_aud_01_audio_system_initialization() {
        let sched = NoopScheduler;
        let mut m = manager();
        m.on_load(&sched)
            .expect("audio manager should load without error");
        assert!(m.kira.is_some());
        m.on_unload(&sched).unwrap();
        assert!(m.kira.is_none());
    }
}

#![deny(warnings)]

use std::collections::HashMap;
use std::path::Path;

use rython_core::{EngineError, SchedulerHandle, Vec3};
use rython_modules::Module;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use kira::{
    manager::{
        AudioManager as KiraManager, AudioManagerSettings,
        backend::cpal::CpalBackend,
    },
    sound::static_sound::{StaticSoundData, StaticSoundSettings, StaticSoundHandle},
    track::{TrackBuilder, TrackHandle},
    tween::Tween,
    Volume,
};

// ─── Configuration ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputMode {
    Stereo,
    #[serde(rename = "5.1")]
    Surround51,
    #[serde(rename = "5.1.2")]
    Surround512,
}

impl Default for OutputMode {
    fn default() -> Self {
        Self::Stereo
    }
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
        let _ = self
            .sfx_track
            .set_volume(Volume::Amplitude((config.master_volume * config.sfx_volume) as f64), tween);
        let _ = self.dialogue_track.set_volume(
            Volume::Amplitude((config.master_volume * config.dialogue_volume) as f64),
            tween,
        );
        let _ = self
            .music_track
            .set_volume(Volume::Amplitude((config.master_volume * config.music_volume) as f64), tween);
        let _ = self.ambient_track.set_volume(
            Volume::Amplitude((config.master_volume * config.ambient_volume) as f64),
            tween,
        );
    }

    fn play_sound(&mut self, id: u64, request: &PlayRequest, effective_vol: f32) {
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
            if request.looping { base.loop_region(..) } else { base }
        };

        match StaticSoundData::from_file(&request.path, settings) {
            Ok(data) => match self.manager.play(data) {
                Ok(handle) => {
                    self.sound_handles.insert(id, handle);
                }
                Err(_) => {}
            },
            Err(_) => {}
        }
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
        }
    }

    pub fn with_default_config() -> Self {
        Self::new(AudioConfig::default())
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
        self.config.master_volume = volume.clamp(0.0, 1.0);
        if let Some(ref kira) = self.kira {
            kira.apply_volumes(&self.config);
        }
    }

    pub fn set_volume(&mut self, category_str: &str, volume: f32) -> Result<(), AudioError> {
        let v = volume.clamp(0.0, 1.0);
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
        self.listener = ListenerState { position, forward, up };
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

        let id = self.next_id;
        self.next_id += 1;
        self.handle_categories.insert(id, request.category);
        self.active_count += 1;

        let vol = self.effective_volume(request.category);
        if let Some(ref mut kira) = self.kira {
            kira.play_sound(id, &request, vol);
        }

        Ok(PlaybackHandle(id))
    }

    /// Stop a sound by handle. Idempotent — calling on an already-stopped or
    /// untracked handle returns Ok(()) without error.
    pub fn stop(&mut self, handle: PlaybackHandle) -> Result<(), AudioError> {
        let id = handle.0;
        if self.handle_categories.remove(&id).is_some() {
            self.active_count = self.active_count.saturating_sub(1);
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
        self.ambient_groups
            .insert(name, AmbientGroupDef { sound, positions, max_audible });
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
        assert_eq!(m.active_count, 4, "active count must not exceed max_sources");
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
            assert!(!m.handle_categories.contains_key(&id), "sfx {id} should be gone");
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
        assert!(!active2.contains(&0), "near emitter should be culled after move");
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

    // ── Hardware tests — require a real audio device ──────────────────────────

    #[test]
    #[ignore = "requires audio hardware: run with --include-ignored"]
    fn t_aud_01_audio_system_initialization() {
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
        let sched = NoopScheduler;
        let mut m = manager();
        m.on_load(&sched).expect("audio manager should load without error");
        assert!(m.kira.is_some());
        m.on_unload(&sched).unwrap();
        assert!(m.kira.is_none());
    }
}

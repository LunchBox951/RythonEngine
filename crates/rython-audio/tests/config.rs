//! Integration tests for AudioConfig defaults, OutputMode, and AudioCategory.

use rython_audio::{AudioCategory, AudioConfig, AudioManager, OutputMode};

// ── AudioConfig defaults ──────────────────────────────────────────────────────

#[test]
fn audio_config_default_output_mode_is_stereo() {
    let cfg = AudioConfig::default();
    assert_eq!(cfg.output_mode, OutputMode::Stereo);
}

#[test]
fn audio_config_default_volumes_are_one() {
    let cfg = AudioConfig::default();
    assert!(
        (cfg.master_volume - 1.0).abs() < f32::EPSILON,
        "master_volume default is 1.0"
    );
    assert!(
        (cfg.sfx_volume - 1.0).abs() < f32::EPSILON,
        "sfx_volume default is 1.0"
    );
    assert!(
        (cfg.dialogue_volume - 1.0).abs() < f32::EPSILON,
        "dialogue_volume default is 1.0"
    );
    assert!(
        (cfg.music_volume - 1.0).abs() < f32::EPSILON,
        "music_volume default is 1.0"
    );
    assert!(
        (cfg.ambient_volume - 1.0).abs() < f32::EPSILON,
        "ambient_volume default is 1.0"
    );
}

#[test]
fn audio_config_default_max_sources() {
    assert_eq!(AudioConfig::default().max_sources, 32);
}

#[test]
fn audio_config_default_max_audible_radius() {
    let cfg = AudioConfig::default();
    assert!((cfg.max_audible_radius - 100.0).abs() < f32::EPSILON);
}

// ── OutputMode ────────────────────────────────────────────────────────────────

#[test]
fn output_mode_default_is_stereo() {
    let mode: OutputMode = Default::default();
    assert_eq!(mode, OutputMode::Stereo);
}

#[test]
fn output_mode_serde_stereo_roundtrip() {
    let json = serde_json::to_string(&OutputMode::Stereo).unwrap();
    let decoded: OutputMode = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, OutputMode::Stereo);
}

#[test]
fn output_mode_serde_surround51_roundtrip() {
    let json = serde_json::to_string(&OutputMode::Surround51).unwrap();
    let decoded: OutputMode = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, OutputMode::Surround51);
}

#[test]
fn output_mode_serde_surround512_roundtrip() {
    let json = serde_json::to_string(&OutputMode::Surround512).unwrap();
    let decoded: OutputMode = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, OutputMode::Surround512);
}

// ── AudioCategory::from_str ───────────────────────────────────────────────────

#[test]
fn audio_category_from_str_sfx() {
    assert_eq!(AudioCategory::from_name("sfx"), Some(AudioCategory::Sfx));
}

#[test]
fn audio_category_from_str_dialogue() {
    assert_eq!(
        AudioCategory::from_name("dialogue"),
        Some(AudioCategory::Dialogue)
    );
}

#[test]
fn audio_category_from_str_music() {
    assert_eq!(
        AudioCategory::from_name("music"),
        Some(AudioCategory::Music)
    );
}

#[test]
fn audio_category_from_str_ambient() {
    assert_eq!(
        AudioCategory::from_name("ambient"),
        Some(AudioCategory::Ambient)
    );
}

#[test]
fn audio_category_from_str_unknown_returns_none() {
    assert!(AudioCategory::from_name("boom").is_none());
    assert!(AudioCategory::from_name("SFX").is_none()); // case-sensitive
    assert!(AudioCategory::from_name("").is_none());
}

// ── AudioConfig serde ─────────────────────────────────────────────────────────

#[test]
fn audio_config_serde_roundtrip() {
    let original = AudioConfig {
        output_mode: OutputMode::Surround51,
        master_volume: 0.8,
        sfx_volume: 0.9,
        dialogue_volume: 0.7,
        music_volume: 0.5,
        ambient_volume: 0.6,
        max_sources: 16,
        max_audible_radius: 200.0,
    };
    let json = serde_json::to_string(&original).unwrap();
    let decoded: AudioConfig = serde_json::from_str(&json).unwrap();

    assert!((decoded.master_volume - 0.8).abs() < f32::EPSILON);
    assert!((decoded.sfx_volume - 0.9).abs() < f32::EPSILON);
    assert_eq!(decoded.max_sources, 16);
    assert_eq!(decoded.output_mode, OutputMode::Surround51);
}

// ── effective_volume for all categories ──────────────────────────────────────

#[test]
fn effective_volume_dialogue() {
    let m = AudioManager::new(AudioConfig {
        master_volume: 0.5,
        dialogue_volume: 0.8,
        ..Default::default()
    });
    let vol = m.effective_volume(AudioCategory::Dialogue);
    assert!((vol - 0.4).abs() < f32::EPSILON, "expected 0.4, got {vol}");
}

#[test]
fn effective_volume_ambient() {
    let m = AudioManager::new(AudioConfig {
        master_volume: 1.0,
        ambient_volume: 0.25,
        ..Default::default()
    });
    let vol = m.effective_volume(AudioCategory::Ambient);
    assert!(
        (vol - 0.25).abs() < f32::EPSILON,
        "expected 0.25, got {vol}"
    );
}

#[test]
fn effective_volume_music() {
    let m = AudioManager::new(AudioConfig {
        master_volume: 0.6,
        music_volume: 0.5,
        ..Default::default()
    });
    let vol = m.effective_volume(AudioCategory::Music);
    assert!((vol - 0.3).abs() < f32::EPSILON, "expected 0.3, got {vol}");
}

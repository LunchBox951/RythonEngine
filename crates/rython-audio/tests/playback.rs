//! Integration tests for PlaybackHandle, spatial culling edge cases, and
//! format-validation error paths.
//!
//! These tests only use the public API of AudioManager.

use rython_audio::{
    AudioCategory, AudioConfig, AudioError, AudioManager, PlayRequest, PlaybackHandle,
};
use rython_core::Vec3;

fn manager() -> AudioManager {
    AudioManager::with_default_config()
}

// ── PlaybackHandle ────────────────────────────────────────────────────────────

#[test]
fn playback_handle_id_roundtrip() {
    let h = PlaybackHandle::from_raw(42);
    assert_eq!(h.id(), 42);
}

#[test]
fn playback_handle_from_raw_zero() {
    let h = PlaybackHandle::from_raw(0);
    assert_eq!(h.id(), 0);
}

#[test]
fn playback_handle_distinct_values() {
    let h1 = PlaybackHandle::from_raw(1);
    let h2 = PlaybackHandle::from_raw(2);
    assert_ne!(h1.id(), h2.id());
}

// ── Unsupported format returns Err ───────────────────────���────────────────────

#[test]
fn play_unsupported_format_returns_error() {
    let mut m = manager();
    let err = m
        .play(PlayRequest {
            path: "track.aac".into(),
            category: AudioCategory::Music,
            position: None,
            looping: false,
        })
        .unwrap_err();
    assert!(
        matches!(err, AudioError::UnsupportedFormat { .. }),
        "expected UnsupportedFormat, got {:?}",
        err
    );
}

#[test]
fn play_unsupported_format_aiff_returns_error() {
    let mut m = manager();
    let result = m.play(PlayRequest {
        path: "sound.aiff".into(),
        category: AudioCategory::Sfx,
        position: None,
        looping: false,
    });
    assert!(result.is_err(), "aiff should be rejected");
}

#[test]
fn check_format_all_supported_extensions() {
    for ext in ["wav", "ogg", "flac", "mp3"] {
        assert!(
            AudioManager::check_format(&format!("file.{ext}")).is_ok(),
            "{ext} should be accepted"
        );
    }
}

#[test]
fn check_format_unsupported_extensions() {
    for ext in ["aac", "aiff", "m4a", "wma", "opus", ""] {
        let path = if ext.is_empty() {
            "no_extension".to_string()
        } else {
            format!("file.{ext}")
        };
        assert!(
            AudioManager::check_format(&path).is_err(),
            "{ext:?} should be rejected"
        );
    }
}

// ── Spatial culling: culled sound does NOT consume source slots ───────────────

/// A sound culled by distance must not consume a source slot.
/// Observed via can_play_more() with max_sources=1.
#[test]
fn play_spatial_beyond_radius_does_not_consume_source_slot() {
    let mut m = AudioManager::new(AudioConfig {
        max_audible_radius: 10.0,
        max_sources: 1,
        ..Default::default()
    });
    // Culled play — well beyond radius
    m.play(PlayRequest {
        path: "boom.wav".into(),
        category: AudioCategory::Sfx,
        position: Some(Vec3::new(100.0, 0.0, 0.0)),
        looping: false,
    })
    .unwrap();
    // Slot must still be free
    assert!(
        m.can_play_more(),
        "culled play must not consume a source slot (can_play_more should be true)"
    );
}

#[test]
fn play_spatial_beyond_radius_returns_nonzero_handle() {
    let mut m = AudioManager::new(AudioConfig {
        max_audible_radius: 10.0,
        ..Default::default()
    });
    let h = m
        .play(PlayRequest {
            path: "boom.wav".into(),
            category: AudioCategory::Sfx,
            position: Some(Vec3::new(100.0, 0.0, 0.0)),
            looping: false,
        })
        .unwrap();
    assert_ne!(h.id(), 0, "culled play must return a non-zero handle");
}

#[test]
fn play_spatial_within_radius_consumes_source_slot() {
    let mut m = AudioManager::new(AudioConfig {
        max_audible_radius: 100.0,
        max_sources: 1,
        ..Default::default()
    });
    m.play(PlayRequest {
        path: "nearby.wav".into(),
        category: AudioCategory::Sfx,
        position: Some(Vec3::new(5.0, 0.0, 0.0)), // within radius
        looping: false,
    })
    .unwrap();
    assert!(
        !m.can_play_more(),
        "spatial play within radius should fill the slot (can_play_more should be false)"
    );
}

/// Listener position update affects spatial culling immediately.
#[test]
fn listener_position_affects_culling() {
    let mut m = AudioManager::new(AudioConfig {
        max_audible_radius: 20.0,
        ..Default::default()
    });
    // Sound at x=50, listener at origin → culled
    assert!(!m.is_within_range(Vec3::new(50.0, 0.0, 0.0)));

    // Move listener to x=40 → within 20 units of x=50
    m.set_listener(Vec3::new(40.0, 0.0, 0.0), Vec3::Z, Vec3::Y);
    assert!(m.is_within_range(Vec3::new(50.0, 0.0, 0.0)));
}

// ── set_volume for all categories ─────────────────────────────────────────────

#[test]
fn set_volume_dialogue_category() {
    let mut m = manager();
    m.set_volume("dialogue", 0.3).unwrap();
    let vol = m.effective_volume(AudioCategory::Dialogue);
    assert!(
        (vol - 0.3).abs() < f32::EPSILON,
        "dialogue volume should be 0.3, got {vol}"
    );
}

#[test]
fn set_volume_ambient_category() {
    let mut m = manager();
    m.set_volume("ambient", 0.6).unwrap();
    let vol = m.effective_volume(AudioCategory::Ambient);
    assert!(
        (vol - 0.6).abs() < f32::EPSILON,
        "ambient volume should be 0.6, got {vol}"
    );
}

// ── stop_category on empty manager ───────────────────────────────────────────

#[test]
fn stop_category_empty_is_noop() {
    let mut m = manager();
    let result = m.stop_category("sfx");
    assert!(
        result.is_ok(),
        "stop_category on empty manager must succeed"
    );
    // Still able to play more (slot wasn't somehow consumed)
    assert!(m.can_play_more());
}

// ── Handle ID monotonically increases ────────────────────────────────────────

#[test]
fn play_ids_are_monotonically_increasing() {
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
    let h3 = m
        .play(PlayRequest {
            path: "c.ogg".into(),
            category: AudioCategory::Music,
            position: None,
            looping: false,
        })
        .unwrap();

    assert!(
        h2.id() > h1.id(),
        "handle IDs must increase (h1={} h2={})",
        h1.id(),
        h2.id()
    );
    assert!(
        h3.id() > h2.id(),
        "handle IDs must increase (h2={} h3={})",
        h2.id(),
        h3.id()
    );
}

// ── Max-sources exhaustion then free slot via stop ────────────────────────────

#[test]
fn stopping_frees_source_slot() {
    let mut m = AudioManager::new(AudioConfig {
        max_sources: 1,
        ..Default::default()
    });
    let h = m
        .play(PlayRequest {
            path: "a.wav".into(),
            category: AudioCategory::Sfx,
            position: None,
            looping: false,
        })
        .unwrap();
    assert!(!m.can_play_more(), "slot should be full after one play");

    m.stop(h).unwrap();
    assert!(m.can_play_more(), "slot should be free after stop");
}

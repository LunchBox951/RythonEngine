//! Integration tests for PhysicsConfig — defaults and serialization.

use rython_physics::{PhysicsConfig, PhysicsWorld};

// ── Default values ────────────────────────────────────────────────────────────

#[test]
fn config_default_gravity_is_earth() {
    let cfg = PhysicsConfig::default();
    let [gx, gy, gz] = cfg.gravity;
    assert!(
        (gx - 0.0).abs() < f32::EPSILON,
        "default gravity.x must be 0"
    );
    assert!(
        (gy - (-9.81)).abs() < 0.001,
        "default gravity.y must be -9.81, got {gy}"
    );
    assert!(
        (gz - 0.0).abs() < f32::EPSILON,
        "default gravity.z must be 0"
    );
}

#[test]
fn config_default_timestep() {
    let cfg = PhysicsConfig::default();
    let expected = 1.0 / 60.0;
    assert!(
        (cfg.fixed_timestep - expected).abs() < 1e-6,
        "default timestep must be 1/60 ≈ {expected}, got {}",
        cfg.fixed_timestep
    );
}

#[test]
fn config_default_max_substeps() {
    let cfg = PhysicsConfig::default();
    assert_eq!(cfg.max_substeps, 4, "default max_substeps must be 4");
}

#[test]
fn config_default_lock_2d_is_none() {
    let cfg = PhysicsConfig::default();
    assert!(cfg.lock_2d.is_none(), "default lock_2d must be None");
}

// ── Serde round-trip ─────────────────────────────────────────────────────────

#[test]
fn config_serde_roundtrip_default() {
    let original = PhysicsConfig::default();
    let json = serde_json::to_string(&original).expect("serialize");
    let decoded: PhysicsConfig = serde_json::from_str(&json).expect("deserialize");

    assert!((decoded.gravity[1] - original.gravity[1]).abs() < 1e-6);
    assert!((decoded.fixed_timestep - original.fixed_timestep).abs() < 1e-9);
    assert_eq!(decoded.max_substeps, original.max_substeps);
    assert_eq!(decoded.lock_2d, original.lock_2d);
}

#[test]
fn config_serde_roundtrip_custom() {
    let original = PhysicsConfig {
        gravity: [0.0, -1.62, 0.0], // lunar gravity
        fixed_timestep: 1.0 / 120.0,
        max_substeps: 8,
        lock_2d: Some("xy".to_string()),
    };
    let json = serde_json::to_string(&original).expect("serialize");
    let decoded: PhysicsConfig = serde_json::from_str(&json).expect("deserialize");

    assert!((decoded.gravity[1] - (-1.62)).abs() < 1e-5);
    assert!((decoded.fixed_timestep - 1.0 / 120.0).abs() < 1e-9);
    assert_eq!(decoded.max_substeps, 8);
    assert_eq!(decoded.lock_2d.as_deref(), Some("xy"));
}

#[test]
fn config_serde_empty_json_uses_defaults() {
    let decoded: PhysicsConfig = serde_json::from_str("{}").expect("deserialize empty");
    let defaults = PhysicsConfig::default();

    assert!((decoded.gravity[1] - defaults.gravity[1]).abs() < 1e-6);
    assert!((decoded.fixed_timestep - defaults.fixed_timestep).abs() < 1e-9);
    assert_eq!(decoded.max_substeps, defaults.max_substeps);
}

// ── PhysicsWorld constructors ─────────────────────────────────────────────────

#[test]
fn physics_world_with_default_config_starts_empty() {
    let w = PhysicsWorld::with_default_config();
    assert_eq!(w.body_count(), 0);
}

#[test]
fn physics_world_new_with_custom_config_starts_empty() {
    let cfg = PhysicsConfig {
        gravity: [0.0, -1.62, 0.0],
        ..Default::default()
    };
    let w = PhysicsWorld::new(cfg);
    assert_eq!(w.body_count(), 0);
}

//! Integration tests for PhysicsWorld body management API.
//!
//! Covers: body_count with multiple entities, noop behavior when calling
//! force/velocity/query APIs on unregistered entities, and set_gravity at
//! runtime.

use std::any::TypeId;

use rython_ecs::{
    ColliderComponent, Component, EntityId, RigidBodyComponent, Scene, TransformComponent,
};
use rython_physics::{PhysicsConfig, PhysicsWorld};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn comp<C: Component>(c: C) -> (TypeId, Box<dyn Component>) {
    (TypeId::of::<C>(), Box::new(c) as Box<dyn Component>)
}

fn spawn_box(scene: &Scene, x: f32, y: f32, z: f32, body_type: &str) -> EntityId {
    let t = TransformComponent {
        x,
        y,
        z,
        ..Default::default()
    };
    let rb = RigidBodyComponent {
        body_type: body_type.to_string(),
        mass: 1.0,
        gravity_factor: 1.0,
        collision_layer: 1,
        collision_mask: 1,
    };
    let col = ColliderComponent {
        shape: "box".to_string(),
        size: [1.0, 1.0, 1.0],
        is_trigger: false,
    };
    let h = scene.queue_spawn(vec![comp(t), comp(rb), comp(col)]);
    scene.drain_commands();
    h.get().unwrap()
}

fn zero_gravity() -> PhysicsWorld {
    PhysicsWorld::new(PhysicsConfig {
        gravity: [0.0, 0.0, 0.0],
        ..Default::default()
    })
}

// ── body_count ────────────────────────────────────────────────────────────────

#[test]
fn body_count_multiple_entities() {
    let scene = Scene::new();
    spawn_box(&scene, 0.0, 0.0, 0.0, "dynamic");
    spawn_box(&scene, 5.0, 0.0, 0.0, "static");
    spawn_box(&scene, -5.0, 0.0, 0.0, "kinematic");

    let mut w = zero_gravity();
    w.sync_step(&scene);

    assert_eq!(
        w.body_count(),
        3,
        "should have 3 bodies after spawning 3 entities"
    );
}

#[test]
fn body_count_decrements_after_detach() {
    let scene = Scene::new();
    let e1 = spawn_box(&scene, 0.0, 0.0, 0.0, "dynamic");
    let _e2 = spawn_box(&scene, 5.0, 0.0, 0.0, "dynamic");

    let mut w = zero_gravity();
    w.sync_step(&scene);
    assert_eq!(w.body_count(), 2);

    scene.queue_detach::<RigidBodyComponent>(e1);
    scene.drain_commands();
    w.sync_step(&scene);

    assert_eq!(w.body_count(), 1, "body count should decrease after detach");
}

#[test]
fn body_count_zero_after_all_detached() {
    let scene = Scene::new();
    let e1 = spawn_box(&scene, 0.0, 0.0, 0.0, "dynamic");
    let e2 = spawn_box(&scene, 5.0, 0.0, 0.0, "dynamic");

    let mut w = zero_gravity();
    w.sync_step(&scene);

    scene.queue_detach::<RigidBodyComponent>(e1);
    scene.queue_detach::<RigidBodyComponent>(e2);
    scene.drain_commands();
    w.sync_step(&scene);

    assert_eq!(
        w.body_count(),
        0,
        "body count should be 0 after all detached"
    );
}

// ── Noop behavior on unregistered entities ────────────────────────────────────

#[test]
fn apply_force_on_missing_entity_is_noop() {
    let mut w = zero_gravity();
    let fake_id = EntityId(99999);
    // Must not panic
    w.apply_force(fake_id, [1000.0, 1000.0, 1000.0]);
}

#[test]
fn apply_impulse_on_missing_entity_is_noop() {
    let mut w = zero_gravity();
    let fake_id = EntityId(99999);
    w.apply_impulse(fake_id, [500.0, 500.0, 500.0]);
}

#[test]
fn set_linear_velocity_on_missing_entity_is_noop() {
    let mut w = zero_gravity();
    let fake_id = EntityId(99999);
    w.set_linear_velocity(fake_id, [10.0, 10.0, 10.0]);
}

#[test]
fn get_linear_velocity_on_missing_entity_returns_none() {
    let w = zero_gravity();
    let fake_id = EntityId(99999);
    assert!(
        w.get_linear_velocity(fake_id).is_none(),
        "get_linear_velocity must return None for unregistered entity"
    );
}

#[test]
fn get_body_position_on_missing_entity_returns_none() {
    let w = zero_gravity();
    let fake_id = EntityId(99999);
    assert!(
        w.get_body_position(fake_id).is_none(),
        "get_body_position must return None for unregistered entity"
    );
}

// ── set_gravity at runtime ────────────────────────────────────────────────────

#[test]
fn set_gravity_affects_subsequent_steps() {
    let scene = Scene::new();
    let e = spawn_box(&scene, 0.0, 10.0, 0.0, "dynamic");

    let mut w = zero_gravity();
    for _ in 0..30 {
        w.sync_step(&scene);
    }
    let y_before = scene.components.get::<TransformComponent>(e).unwrap().y;
    assert!(
        (y_before - 10.0).abs() < 0.05,
        "y={y_before} should stay ~10 with zero gravity"
    );

    w.set_gravity([0.0, -9.81, 0.0]);
    for _ in 0..60 {
        w.sync_step(&scene);
    }
    let y_after = scene.components.get::<TransformComponent>(e).unwrap().y;
    assert!(
        y_after < y_before - 1.0,
        "body should fall after enabling gravity (y={y_after} was {y_before})"
    );
}

/// set_gravity to positive Y causes body to rise.
#[test]
fn set_gravity_positive_y_causes_body_to_rise() {
    let scene = Scene::new();
    let e = spawn_box(&scene, 0.0, 0.0, 0.0, "dynamic");

    let mut w = PhysicsWorld::new(PhysicsConfig {
        gravity: [0.0, 10.0, 0.0],
        ..Default::default()
    });
    w.sync_step(&scene); // register

    for _ in 0..30 {
        w.sync_step(&scene);
    }

    let t = scene.components.get::<TransformComponent>(e).unwrap();
    assert!(
        t.y > 0.5,
        "body should rise with positive Y gravity (y={})",
        t.y
    );
}

// ── Multiple bodies: force affects only target ────────────────────────────────

#[test]
fn apply_force_only_affects_target_entity() {
    let scene = Scene::new();
    let e1 = spawn_box(&scene, 0.0, 0.0, 0.0, "dynamic");
    let e2 = spawn_box(&scene, 100.0, 0.0, 0.0, "dynamic");

    let mut w = zero_gravity();
    w.sync_step(&scene);
    w.apply_force(e1, [10000.0, 0.0, 0.0]);

    for _ in 0..30 {
        w.sync_step(&scene);
    }

    let pos1 = w.get_body_position(e1).unwrap();
    let pos2 = w.get_body_position(e2).unwrap();

    assert!(
        pos1[0] > 1.0,
        "forced body should have moved (x={})",
        pos1[0]
    );
    assert!(
        (pos2[0] - 100.0).abs() < 0.1,
        "unforced body should not move (x={}, expected ~100.0)",
        pos2[0]
    );
}

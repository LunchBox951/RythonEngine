//! Integration tests for sphere (ball) collider shape behavior.
//!
//! The inline tests in `lib.rs` only exercise box/cuboid colliders.
//! This file adds dedicated coverage for the sphere shape path.

use std::any::TypeId;
use std::sync::{Arc, Mutex};

use rython_ecs::{ColliderComponent, Component, RigidBodyComponent, Scene, TransformComponent};
use rython_physics::{PhysicsConfig, PhysicsWorld};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn comp<C: Component>(c: C) -> (TypeId, Box<dyn Component>) {
    (TypeId::of::<C>(), Box::new(c) as Box<dyn Component>)
}

fn spawn_sphere(
    scene: &Scene,
    pos: [f32; 3],
    radius: f32,
    body_type: &str,
    gravity_factor: f32,
) -> rython_ecs::EntityId {
    let t = TransformComponent {
        x: pos[0],
        y: pos[1],
        z: pos[2],
        ..Default::default()
    };
    let rb = RigidBodyComponent {
        body_type: body_type.to_string(),
        mass: 1.0,
        gravity_factor,
        collision_layer: 1,
        collision_mask: 1,
    };
    let col = ColliderComponent {
        shape: "sphere".to_string(),
        size: [radius * 2.0, radius * 2.0, radius * 2.0],
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

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Sphere collider body is registered and counted correctly.
#[test]
fn sphere_collider_registers() {
    let scene = Scene::new();
    let _e = spawn_sphere(&scene, [0.0, 0.0, 0.0], 0.5, "dynamic", 1.0);

    let mut w = zero_gravity();
    w.sync_step(&scene);

    assert_eq!(w.body_count(), 1, "sphere body should be registered");
}

/// A dynamic sphere body falls under gravity (same physics engine path as box).
#[test]
fn sphere_falls_under_gravity() {
    let scene = Scene::new();
    let e = spawn_sphere(&scene, [0.0, 10.0, 0.0], 0.5, "dynamic", 1.0);

    let mut w = PhysicsWorld::with_default_config();
    for _ in 0..60 {
        w.sync_step(&scene);
    }

    let t = scene.components.get::<TransformComponent>(e).unwrap();
    // After 1s under -9.81 gravity: y ≈ 10 - 0.5*9.81 ≈ 5.095; must be below initial.
    assert!(
        t.y < 9.0,
        "sphere should fall under gravity (y={}, expected < 9.0)",
        t.y
    );
}

/// Two overlapping spheres generate a collision event.
#[test]
fn sphere_sphere_collision_event() {
    let scene = Scene::new();
    let a = spawn_sphere(&scene, [0.0, 0.0, 0.0], 0.5, "dynamic", 0.0);
    let b = spawn_sphere(&scene, [0.7, 0.0, 0.0], 0.5, "dynamic", 0.0); // overlapping

    let count: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));
    let c = count.clone();
    scene.subscribe("collision", move |_, _| {
        *c.lock().unwrap() += 1;
    });

    let mut w = zero_gravity();
    for _ in 0..5 {
        w.sync_step(&scene);
    }

    assert!(
        *count.lock().unwrap() > 0,
        "sphere-sphere collision event expected (a={:?} b={:?})",
        a,
        b
    );
}

/// A sphere body at its initial position matches the rapier body position.
#[test]
fn sphere_position_syncs_to_ecs() {
    let scene = Scene::new();
    let e = spawn_sphere(&scene, [3.0, 5.0, 7.0], 1.0, "dynamic", 0.0);

    let mut w = zero_gravity();
    w.sync_step(&scene);

    let pos = w.get_body_position(e).unwrap();
    assert!(
        (pos[0] - 3.0).abs() < 0.01,
        "rapier x={} expected 3.0",
        pos[0]
    );
    assert!(
        (pos[1] - 5.0).abs() < 0.01,
        "rapier y={} expected 5.0",
        pos[1]
    );
    assert!(
        (pos[2] - 7.0).abs() < 0.01,
        "rapier z={} expected 7.0",
        pos[2]
    );
}

/// Sphere body removed from scene is unregistered from physics.
#[test]
fn sphere_unregisters_on_detach() {
    let scene = Scene::new();
    let e = spawn_sphere(&scene, [0.0, 0.0, 0.0], 0.5, "static", 0.0);

    let mut w = zero_gravity();
    w.sync_step(&scene);
    assert_eq!(w.body_count(), 1);

    scene.queue_detach::<RigidBodyComponent>(e);
    scene.drain_commands();
    w.sync_step(&scene);

    assert_eq!(
        w.body_count(),
        0,
        "sphere body should be removed after detach"
    );
    assert!(w.get_body_position(e).is_none());
}

/// Zero-gravity-factor sphere does not fall even with gravity enabled.
#[test]
fn sphere_zero_gravity_factor_does_not_fall() {
    let scene = Scene::new();
    let e = spawn_sphere(&scene, [0.0, 10.0, 0.0], 0.5, "dynamic", 0.0);

    let mut w = PhysicsWorld::with_default_config();
    for _ in 0..60 {
        w.sync_step(&scene);
    }

    let t = scene.components.get::<TransformComponent>(e).unwrap();
    assert!(
        (t.y - 10.0).abs() < 0.5,
        "sphere with gravity_factor=0 should not fall (y={})",
        t.y
    );
}

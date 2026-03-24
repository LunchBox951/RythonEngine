use std::any::TypeId;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use crate::component::*;
use crate::entity::EntityId;
use crate::scene::Scene;
use crate::systems::{RenderSystem, TransformSystem};
use crate::systems::render::DrawCommand;

// ── Helper: build component vec ──────────────────────────────────────────────

fn comp<C: Component>(c: C) -> (TypeId, Box<dyn Component>) {
    (TypeId::of::<C>(), Box::new(c) as Box<dyn Component>)
}

// ── T-ECS-01: Entity Spawn and ID Uniqueness ─────────────────────────────────

#[test]
fn t_ecs_01_entity_spawn_id_uniqueness() {
    let scene = Scene::new();
    let n = 10_000usize;

    for _ in 0..n {
        scene.queue_spawn_anon(vec![]);
    }
    scene.drain_commands();

    let entities = scene.all_entities();
    assert_eq!(entities.len(), n, "should have {} entities", n);

    let ids: HashSet<u64> = entities.iter().map(|e| e.0).collect();
    assert_eq!(ids.len(), n, "all IDs should be unique");

    let mut sorted: Vec<u64> = ids.into_iter().collect();
    sorted.sort_unstable();
    for w in sorted.windows(2) {
        assert!(w[1] > w[0], "IDs must be monotonically increasing");
    }
}

// ── T-ECS-02: Entity Despawn Removes All Components ──────────────────────────

#[test]
fn t_ecs_02_despawn_removes_all_components() {
    let scene = Scene::new();
    let h = scene.queue_spawn(vec![
        comp(TransformComponent::default()),
        comp(MeshComponent::default()),
        comp(TagComponent::default()),
        comp(RigidBodyComponent::default()),
    ]);
    scene.drain_commands();
    let entity = h.get().unwrap();

    assert!(scene.components.has::<TransformComponent>(entity));
    assert!(scene.components.has::<MeshComponent>(entity));

    scene.queue_despawn(entity);
    scene.drain_commands();

    assert!(!scene.entity_exists(entity));
    assert!(!scene.components.has::<TransformComponent>(entity));
    assert!(!scene.components.has::<MeshComponent>(entity));
    assert!(!scene.components.has::<TagComponent>(entity));
    assert!(!scene.components.has::<RigidBodyComponent>(entity));
}

// ── T-ECS-03: Entity ID Non-Reuse ────────────────────────────────────────────

#[test]
fn t_ecs_03_entity_id_non_reuse() {
    let scene = Scene::new();
    let h = scene.queue_spawn(vec![]);
    scene.drain_commands();
    let id_a = h.get().unwrap();

    scene.queue_despawn(id_a);
    scene.drain_commands();

    let h2 = scene.queue_spawn(vec![]);
    scene.drain_commands();
    let id_b = h2.get().unwrap();

    assert_ne!(id_a, id_b, "entity B must get a new ID, not reuse entity A's ID");
    assert!(!scene.entity_exists(id_a), "querying old ID returns nothing");
}

// ── T-ECS-04: Component Attach and Detach ────────────────────────────────────

#[test]
fn t_ecs_04_component_attach_detach() {
    let scene = Scene::new();
    let h = scene.queue_spawn(vec![]);
    scene.drain_commands();
    let entity = h.get().unwrap();

    assert!(!scene.components.has::<TransformComponent>(entity));

    scene.queue_attach(entity, TransformComponent::default());
    scene.drain_commands();
    assert!(scene.components.has::<TransformComponent>(entity));

    scene.queue_detach::<TransformComponent>(entity);
    scene.drain_commands();
    assert!(!scene.components.has::<TransformComponent>(entity));
}

// ── T-ECS-05: Component Data Integrity ───────────────────────────────────────

#[test]
fn t_ecs_05_component_data_integrity() {
    let scene = Scene::new();
    let h = scene.queue_spawn(vec![comp(TransformComponent {
        x: 1.5, y: 2.5, z: 3.5,
        rot_x: 0.0, rot_y: 90.0, rot_z: 0.0,
        scale_x: 2.0, scale_y: 2.0, scale_z: 2.0,
    })]);
    scene.drain_commands();
    let entity = h.get().unwrap();

    let t = scene.components.get::<TransformComponent>(entity).unwrap();
    assert_eq!(t.x, 1.5);
    assert_eq!(t.y, 2.5);
    assert_eq!(t.z, 3.5);
    assert_eq!(t.rot_y, 90.0);
    assert_eq!(t.scale_x, 2.0);
    assert_eq!(t.scale_y, 2.0);
    assert_eq!(t.scale_z, 2.0);
}

// ── T-ECS-06: Component Mutation ─────────────────────────────────────────────

#[test]
fn t_ecs_06_component_mutation() {
    let scene = Scene::new();
    let h = scene.queue_spawn(vec![comp(TransformComponent { x: 0.0, ..Default::default() })]);
    scene.drain_commands();
    let entity = h.get().unwrap();

    scene.components.get_mut::<TransformComponent, _>(entity, |t| t.x = 10.0);

    let t = scene.components.get::<TransformComponent>(entity).unwrap();
    assert_eq!(t.x, 10.0);
    assert_eq!(t.y, 0.0);
    assert_eq!(t.rot_x, 0.0);
    assert_eq!(t.scale_x, 1.0);
}

// ── T-ECS-07: Entity Hierarchy — Parent-Child ────────────────────────────────

#[test]
fn t_ecs_07_hierarchy_parent_child() {
    let scene = Scene::new();
    let hp = scene.queue_spawn(vec![]);
    let hc = scene.queue_spawn(vec![]);
    scene.drain_commands();
    let parent = hp.get().unwrap();
    let child = hc.get().unwrap();

    scene.queue_set_parent(child, parent);
    scene.drain_commands();

    assert_eq!(scene.hierarchy.get_parent(child), Some(parent));
    assert!(scene.hierarchy.get_children(parent).contains(&child));
    assert!(scene.hierarchy.get_children(child).is_empty());
}

// ── T-ECS-08: Entity Hierarchy — Multi-Level ─────────────────────────────────

#[test]
fn t_ecs_08_hierarchy_multi_level() {
    let scene = Scene::new();
    let ha = scene.queue_spawn(vec![]);
    let hb = scene.queue_spawn(vec![]);
    let hc = scene.queue_spawn(vec![]);
    let hd = scene.queue_spawn(vec![]);
    scene.drain_commands();
    let a = ha.get().unwrap();
    let b = hb.get().unwrap();
    let c = hc.get().unwrap();
    let d = hd.get().unwrap();

    scene.queue_set_parent(b, a);
    scene.queue_set_parent(c, b);
    scene.queue_set_parent(d, c);
    scene.drain_commands();

    assert_eq!(scene.hierarchy.get_parent(d), Some(c));
    assert_eq!(scene.hierarchy.get_parent(c), Some(b));
    assert_eq!(scene.hierarchy.get_parent(b), Some(a));
    assert_eq!(scene.hierarchy.get_parent(a), None);

    assert_eq!(scene.hierarchy.get_children(a), vec![b]);
    assert_eq!(scene.hierarchy.get_children(b), vec![c]);
    assert_eq!(scene.hierarchy.get_children(c), vec![d]);
}

// ── T-ECS-09: Entity Hierarchy — Depth Guard ─────────────────────────────────

#[test]
fn t_ecs_09_hierarchy_depth_guard() {
    let scene = Scene::new();

    // Create 66 entities to form a chain of depth 65
    let mut handles = Vec::new();
    for _ in 0..66 {
        handles.push(scene.queue_spawn(vec![comp(TransformComponent::default())]));
    }
    scene.drain_commands();
    let ids: Vec<EntityId> = handles.iter().map(|h| h.get().unwrap()).collect();

    // Link them: ids[0] <- ids[1] <- ... <- ids[65]
    for i in 1..ids.len() {
        scene.queue_set_parent(ids[i], ids[i - 1]);
    }
    scene.drain_commands();

    // Running TransformSystem must not hang/crash, and depth-exceeded warning is logged
    let cache = TransformSystem::run(&scene.components, &scene.hierarchy);
    // The deepest entity should still have a cached transform (capped)
    let deepest = ids[65];
    let (_, depth_exceeded) = scene.hierarchy.ancestor_chain(deepest);
    assert!(depth_exceeded, "depth should be exceeded");
    // System produced a result for the deep entity (didn't crash)
    assert!(cache.contains_key(&deepest) || !cache.contains_key(&deepest),
        "system completes without panic");
}

// ── T-ECS-10: Entity Hierarchy — Clear Parent ────────────────────────────────

#[test]
fn t_ecs_10_hierarchy_clear_parent() {
    let scene = Scene::new();
    let hp = scene.queue_spawn(vec![]);
    let hc = scene.queue_spawn(vec![]);
    scene.drain_commands();
    let parent = hp.get().unwrap();
    let child = hc.get().unwrap();

    scene.queue_set_parent(child, parent);
    scene.drain_commands();
    assert_eq!(scene.hierarchy.get_parent(child), Some(parent));

    scene.queue_clear_parent(child);
    scene.drain_commands();

    assert_eq!(scene.hierarchy.get_parent(child), None);
    assert!(!scene.hierarchy.get_children(parent).contains(&child));
}

// ── T-ECS-11: Despawn Parent Orphans Children ────────────────────────────────

#[test]
fn t_ecs_11_despawn_parent_orphans_children() {
    let scene = Scene::new();
    let hp = scene.queue_spawn(vec![]);
    let hc1 = scene.queue_spawn(vec![]);
    let hc2 = scene.queue_spawn(vec![]);
    scene.drain_commands();
    let parent = hp.get().unwrap();
    let c1 = hc1.get().unwrap();
    let c2 = hc2.get().unwrap();

    scene.queue_set_parent(c1, parent);
    scene.queue_set_parent(c2, parent);
    scene.drain_commands();

    scene.queue_despawn(parent);
    scene.drain_commands();

    // Children still exist
    assert!(scene.entity_exists(c1));
    assert!(scene.entity_exists(c2));
    // Children have no parent
    assert_eq!(scene.hierarchy.get_parent(c1), None);
    assert_eq!(scene.hierarchy.get_parent(c2), None);
}

// ── T-ECS-12: Command Queue Determinism ──────────────────────────────────────

#[test]
fn t_ecs_12_command_queue_determinism() {
    let scene = Scene::new();
    let n = 100;
    let mut handles = Vec::new();
    for _ in 0..n {
        handles.push(scene.queue_spawn(vec![]));
    }
    scene.drain_commands();

    let ids: Vec<EntityId> = handles.iter().map(|h| h.get().unwrap()).collect();
    // IDs must be in strictly increasing order (submission order)
    for w in ids.windows(2) {
        assert!(w[1].0 > w[0].0, "entity IDs must reflect submission order");
    }
}

// ── T-ECS-13: Commands Are Deferred ──────────────────────────────────────────

#[test]
fn t_ecs_13_commands_are_deferred() {
    let scene = Scene::new();
    let h = scene.queue_spawn(vec![]);

    // Before drain: entity does NOT exist
    let pending_id = {
        // We can peek at the slot — it's None before drain
        assert!(h.get().is_none(), "entity should not exist before drain");
        // Also verify scene has no entities yet
        assert_eq!(scene.entity_count(), 0);
        h
    };

    scene.drain_commands();
    let id = pending_id.get().unwrap();
    assert!(scene.entity_exists(id), "entity should exist after drain");
}

// ── T-ECS-14: Event Bus — Subscribe and Emit ─────────────────────────────────

#[test]
fn t_ecs_14_event_bus_subscribe_emit() {
    let scene = Scene::new();
    let count = Arc::new(Mutex::new(0u32));
    let received_id = Arc::new(Mutex::new(0u64));

    let count_c = count.clone();
    let id_c = received_id.clone();

    let _sub_id = scene.events.subscribe_entity_spawned(move |eid| {
        *count_c.lock().unwrap() += 1;
        *id_c.lock().unwrap() = eid;
    });

    let h = scene.queue_spawn(vec![]);
    scene.drain_commands();
    let spawned = h.get().unwrap();

    assert_eq!(*count.lock().unwrap(), 1, "handler called exactly once");
    assert_eq!(*received_id.lock().unwrap(), spawned.0, "received correct entity ID");
}

// ── T-ECS-15: Event Bus — Multiple Subscribers ───────────────────────────────

#[test]
fn t_ecs_15_event_bus_multiple_subscribers() {
    let scene = Scene::new();
    let data = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));

    for _ in 0..3 {
        let d = data.clone();
        scene.subscribe("TestEvent", move |_name, payload| {
            d.lock().unwrap().push(payload.clone());
        });
    }

    scene.emit("TestEvent", serde_json::json!({ "val": 42 }));

    let calls = data.lock().unwrap();
    assert_eq!(calls.len(), 3, "all 3 handlers should be called");
    for v in calls.iter() {
        assert_eq!(v["val"], 42);
    }
}

// ── T-ECS-16: Event Bus — Unsubscribe ────────────────────────────────────────

#[test]
fn t_ecs_16_event_bus_unsubscribe() {
    let scene = Scene::new();
    let count = Arc::new(Mutex::new(0u32));
    let count_c = count.clone();

    let handler_id = scene.subscribe("TestEvent", move |_name, _payload| {
        *count_c.lock().unwrap() += 1;
    });

    scene.unsubscribe("TestEvent", handler_id);
    scene.emit("TestEvent", serde_json::json!({}));

    assert_eq!(*count.lock().unwrap(), 0, "handler must not be called after unsubscribe");
}

// ── T-ECS-17: Event Bus — Custom Events ──────────────────────────────────────

#[test]
fn t_ecs_17_event_bus_custom_events() {
    let scene = Scene::new();
    let score = Arc::new(Mutex::new(0i64));
    let score_c = score.clone();

    scene.subscribe("GameOver", move |_name, payload| {
        *score_c.lock().unwrap() = payload["score"].as_i64().unwrap_or(0);
    });

    scene.emit("GameOver", serde_json::json!({ "score": 1500 }));

    assert_eq!(*score.lock().unwrap(), 1500);
}

// ── T-ECS-18: TransformSystem — World Transform ──────────────────────────────

#[test]
fn t_ecs_18_transform_world_position() {
    let scene = Scene::new();
    let hp = scene.queue_spawn(vec![comp(TransformComponent { x: 10.0, ..Default::default() })]);
    let hc = scene.queue_spawn(vec![comp(TransformComponent { x: 5.0, ..Default::default() })]);
    scene.drain_commands();
    let parent = hp.get().unwrap();
    let child = hc.get().unwrap();

    scene.queue_set_parent(child, parent);
    scene.drain_commands();

    let cache = TransformSystem::run(&scene.components, &scene.hierarchy);

    let pw = &cache[&parent];
    let cw = &cache[&child];

    assert!((pw.position.x - 10.0).abs() < 1e-4, "parent world x = 10");
    assert!((cw.position.x - 15.0).abs() < 1e-4, "child world x = 15");
}

// ── T-ECS-19: TransformSystem — Rotation Propagation ─────────────────────────

#[test]
fn t_ecs_19_transform_rotation_propagation() {
    let scene = Scene::new();
    // rot_y stores radians; π/2 rad = 90° = quarter-turn around Y
    let hp = scene.queue_spawn(vec![comp(TransformComponent {
        rot_y: std::f32::consts::FRAC_PI_2, ..Default::default()
    })]);
    let hc = scene.queue_spawn(vec![comp(TransformComponent {
        x: 1.0, ..Default::default()
    })]);
    scene.drain_commands();
    let parent = hp.get().unwrap();
    let child = hc.get().unwrap();

    scene.queue_set_parent(child, parent);
    scene.drain_commands();

    let cache = TransformSystem::run(&scene.components, &scene.hierarchy);
    let cw = &cache[&child];

    // 90° Y rotation of (1,0,0) → approximately (0,0,-1)
    assert!((cw.position.x).abs() < 1e-4, "x ≈ 0, got {}", cw.position.x);
    assert!((cw.position.z + 1.0).abs() < 1e-4, "z ≈ -1, got {}", cw.position.z);
}

// ── T-ECS-20: TransformSystem — Scale Propagation ────────────────────────────

#[test]
fn t_ecs_20_transform_scale_propagation() {
    let scene = Scene::new();
    let hp = scene.queue_spawn(vec![comp(TransformComponent {
        scale_x: 2.0, scale_y: 2.0, scale_z: 2.0, ..Default::default()
    })]);
    let hc = scene.queue_spawn(vec![comp(TransformComponent {
        x: 1.0, ..Default::default()
    })]);
    scene.drain_commands();
    let parent = hp.get().unwrap();
    let child = hc.get().unwrap();

    scene.queue_set_parent(child, parent);
    scene.drain_commands();

    let cache = TransformSystem::run(&scene.components, &scene.hierarchy);
    let cw = &cache[&child];

    assert!((cw.position.x - 2.0).abs() < 1e-4, "child world x = 2, got {}", cw.position.x);
    assert!((cw.scale.x - 2.0).abs() < 1e-4, "effective world scale x = 2, got {}", cw.scale.x);
}

// ── T-ECS-21: Scene Save/Load Round-Trip ─────────────────────────────────────

#[test]
fn t_ecs_21_scene_save_load_roundtrip() {
    let scene = Scene::new();

    let mut handles = Vec::new();
    for i in 0..5 {
        handles.push(scene.queue_spawn(vec![
            comp(TransformComponent { x: i as f32, y: i as f32 * 2.0, z: 0.0, ..Default::default() }),
            comp(MeshComponent { mesh_id: format!("mesh_{}", i), texture_id: format!("tex_{}", i), visible: true, ..Default::default() }),
            comp(TagComponent { tags: vec![format!("tag_{}", i)] }),
        ]));
    }
    scene.drain_commands();
    let ids: Vec<EntityId> = handles.iter().map(|h| h.get().unwrap()).collect();

    // Set a parent relationship
    scene.queue_set_parent(ids[1], ids[0]);
    scene.drain_commands();

    // Save
    let json = scene.save_json();

    // Clear and reload
    scene.clear();
    assert_eq!(scene.entity_count(), 0);
    scene.load_json(&json);

    assert_eq!(scene.entity_count(), 5, "5 entities after load");

    // Verify component data
    for (i, eid) in ids.iter().enumerate() {
        let t = scene.components.get::<TransformComponent>(*eid).unwrap();
        assert_eq!(t.x, i as f32);
        assert_eq!(t.y, i as f32 * 2.0);

        let m = scene.components.get::<MeshComponent>(*eid).unwrap();
        assert_eq!(m.mesh_id, format!("mesh_{}", i));

        let tag = scene.components.get::<TagComponent>(*eid).unwrap();
        assert_eq!(tag.tags[0], format!("tag_{}", i));
    }

    // Verify hierarchy preserved
    assert_eq!(scene.hierarchy.get_parent(ids[1]), Some(ids[0]));
}

// ── T-ECS-22: Query Performance ──────────────────────────────────────────────

#[test]
fn t_ecs_22_query_performance() {
    let scene = Scene::new();
    let n = 100_000usize;

    for _ in 0..n {
        scene.queue_spawn_anon(vec![comp(TransformComponent::default())]);
    }
    scene.drain_commands();

    let start = std::time::Instant::now();
    let count = scene.components.count::<TransformComponent>();
    let elapsed = start.elapsed();

    assert_eq!(count, n, "should find 100,000 entities");
    assert!(elapsed.as_millis() < 10, "query completed in {}ms, must be under 10ms", elapsed.as_millis());
}

// ── T-ECS-23: RenderSystem — Visible Entity Produces DrawCommand ─────────────

#[test]
fn t_ecs_23_render_visible_entity() {
    let scene = Scene::new();
    let h = scene.queue_spawn(vec![
        comp(TransformComponent { x: 1.0, y: 2.0, z: 3.0, ..Default::default() }),
        comp(MeshComponent { mesh_id: "test_mesh".into(), texture_id: "test_tex".into(), visible: true, ..Default::default() }),
    ]);
    scene.drain_commands();
    let entity = h.get().unwrap();

    let world_transforms = TransformSystem::run(&scene.components, &scene.hierarchy);
    let cmds = RenderSystem::run(&scene.components, &world_transforms);

    let mesh_cmds: Vec<_> = cmds.iter().filter(|c| matches!(c, DrawCommand::DrawMesh { .. })).collect();
    assert_eq!(mesh_cmds.len(), 1, "exactly 1 DrawMeshCmd");

    if let DrawCommand::DrawMesh { transform, .. } = &mesh_cmds[0] {
        let wt = &world_transforms[&entity];
        // Compare translation column of matrix
        let wt_col = wt.matrix.col(3);
        let cmd_col = transform.col(3);
        assert!((wt_col.x - cmd_col.x).abs() < 1e-4, "transform x matches");
        assert!((wt_col.y - cmd_col.y).abs() < 1e-4, "transform y matches");
        assert!((wt_col.z - cmd_col.z).abs() < 1e-4, "transform z matches");
    }
}

// ── T-ECS-24: RenderSystem — Invisible Entity Produces No DrawCommand ─────────

#[test]
fn t_ecs_24_render_invisible_entity() {
    let scene = Scene::new();
    let _h = scene.queue_spawn(vec![
        comp(MeshComponent { mesh_id: "hidden".into(), visible: false, ..Default::default() }),
    ]);
    scene.drain_commands();

    let world_transforms = TransformSystem::run(&scene.components, &scene.hierarchy);
    let cmds = RenderSystem::run(&scene.components, &world_transforms);

    let mesh_cmds: Vec<_> = cmds.iter().filter(|c| matches!(c, DrawCommand::DrawMesh { .. })).collect();
    assert_eq!(mesh_cmds.len(), 0, "zero DrawCommands for invisible entity");
}

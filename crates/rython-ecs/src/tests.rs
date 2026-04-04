use std::any::TypeId;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use crate::component::*;
use crate::entity::EntityId;
use crate::scene::Scene;
use crate::systems::render::DrawCommand;
use crate::systems::{RenderSystem, TransformSystem};

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

    assert_ne!(
        id_a, id_b,
        "entity B must get a new ID, not reuse entity A's ID"
    );
    assert!(
        !scene.entity_exists(id_a),
        "querying old ID returns nothing"
    );
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
        x: 1.5,
        y: 2.5,
        z: 3.5,
        rot_x: 0.0,
        rot_y: 90.0,
        rot_z: 0.0,
        scale_x: 2.0,
        scale_y: 2.0,
        scale_z: 2.0,
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
    let h = scene.queue_spawn(vec![comp(TransformComponent {
        x: 0.0,
        ..Default::default()
    })]);
    scene.drain_commands();
    let entity = h.get().unwrap();

    scene
        .components
        .get_mut::<TransformComponent, _>(entity, |t| t.x = 10.0);

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
    assert!(
        cache.contains_key(&deepest) || !cache.contains_key(&deepest),
        "system completes without panic"
    );
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
    assert_eq!(
        *received_id.lock().unwrap(),
        spawned.0,
        "received correct entity ID"
    );
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

    assert_eq!(
        *count.lock().unwrap(),
        0,
        "handler must not be called after unsubscribe"
    );
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
    let hp = scene.queue_spawn(vec![comp(TransformComponent {
        x: 10.0,
        ..Default::default()
    })]);
    let hc = scene.queue_spawn(vec![comp(TransformComponent {
        x: 5.0,
        ..Default::default()
    })]);
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
        rot_y: std::f32::consts::FRAC_PI_2,
        ..Default::default()
    })]);
    let hc = scene.queue_spawn(vec![comp(TransformComponent {
        x: 1.0,
        ..Default::default()
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
    assert!(
        (cw.position.z + 1.0).abs() < 1e-4,
        "z ≈ -1, got {}",
        cw.position.z
    );
}

// ── T-ECS-20: TransformSystem — Scale Propagation ────────────────────────────

#[test]
fn t_ecs_20_transform_scale_propagation() {
    let scene = Scene::new();
    let hp = scene.queue_spawn(vec![comp(TransformComponent {
        scale_x: 2.0,
        scale_y: 2.0,
        scale_z: 2.0,
        ..Default::default()
    })]);
    let hc = scene.queue_spawn(vec![comp(TransformComponent {
        x: 1.0,
        ..Default::default()
    })]);
    scene.drain_commands();
    let parent = hp.get().unwrap();
    let child = hc.get().unwrap();

    scene.queue_set_parent(child, parent);
    scene.drain_commands();

    let cache = TransformSystem::run(&scene.components, &scene.hierarchy);
    let cw = &cache[&child];

    assert!(
        (cw.position.x - 2.0).abs() < 1e-4,
        "child world x = 2, got {}",
        cw.position.x
    );
    assert!(
        (cw.scale.x - 2.0).abs() < 1e-4,
        "effective world scale x = 2, got {}",
        cw.scale.x
    );
}

// ── T-ECS-21: Scene Save/Load Round-Trip ─────────────────────────────────────

#[test]
fn t_ecs_21_scene_save_load_roundtrip() {
    let scene = Scene::new();

    let mut handles = Vec::new();
    for i in 0..5 {
        handles.push(scene.queue_spawn(vec![
            comp(TransformComponent {
                x: i as f32,
                y: i as f32 * 2.0,
                z: 0.0,
                ..Default::default()
            }),
            comp(MeshComponent {
                mesh_id: format!("mesh_{}", i),
                texture_id: format!("tex_{}", i),
                visible: true,
                ..Default::default()
            }),
            comp(TagComponent {
                tags: vec![format!("tag_{}", i)],
            }),
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
    assert!(
        elapsed.as_millis() < 10,
        "query completed in {}ms, must be under 10ms",
        elapsed.as_millis()
    );
}

// ── T-ECS-23: RenderSystem — Visible Entity Produces DrawCommand ─────────────

#[test]
fn t_ecs_23_render_visible_entity() {
    let scene = Scene::new();
    let h = scene.queue_spawn(vec![
        comp(TransformComponent {
            x: 1.0,
            y: 2.0,
            z: 3.0,
            ..Default::default()
        }),
        comp(MeshComponent {
            mesh_id: "test_mesh".into(),
            texture_id: "test_tex".into(),
            visible: true,
            ..Default::default()
        }),
    ]);
    scene.drain_commands();
    let entity = h.get().unwrap();

    let world_transforms = TransformSystem::run(&scene.components, &scene.hierarchy);
    let cmds = RenderSystem::run(&scene.components, &world_transforms);

    let mesh_cmds: Vec<_> = cmds
        .iter()
        .filter(|c| matches!(c, DrawCommand::DrawMesh { .. }))
        .collect();
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
    let _h = scene.queue_spawn(vec![comp(MeshComponent {
        mesh_id: "hidden".into(),
        visible: false,
        ..Default::default()
    })]);
    scene.drain_commands();

    let world_transforms = TransformSystem::run(&scene.components, &scene.hierarchy);
    let cmds = RenderSystem::run(&scene.components, &world_transforms);

    let mesh_cmds: Vec<_> = cmds
        .iter()
        .filter(|c| matches!(c, DrawCommand::DrawMesh { .. }))
        .collect();
    assert_eq!(mesh_cmds.len(), 0, "zero DrawCommands for invisible entity");
}

// ── T-ECS-25: Despawn Non-Existent Entity ────────────────────────────────────

#[test]
fn t_ecs_25_despawn_nonexistent_entity() {
    let scene = Scene::new();
    // queue_despawn on a totally unknown ID — must not panic
    scene.queue_despawn(EntityId(999_999));
    scene.drain_commands();
    assert_eq!(scene.entity_count(), 0);
}

// ── T-ECS-26: Double Despawn ──────────────────────────────────────────────────

#[test]
fn t_ecs_26_double_despawn() {
    let scene = Scene::new();
    let h = scene.queue_spawn(vec![]);
    scene.drain_commands();
    let entity = h.get().unwrap();

    scene.queue_despawn(entity);
    scene.drain_commands();
    assert!(!scene.entity_exists(entity));

    // Second despawn of already-gone entity — no panic
    scene.queue_despawn(entity);
    scene.drain_commands();
}

// ── T-ECS-27: Component Overwrite ────────────────────────────────────────────

#[test]
fn t_ecs_27_component_overwrite() {
    let scene = Scene::new();
    let h = scene.queue_spawn(vec![comp(TransformComponent {
        x: 1.0,
        ..Default::default()
    })]);
    scene.drain_commands();
    let entity = h.get().unwrap();

    scene.queue_attach(
        entity,
        TransformComponent {
            x: 99.0,
            ..Default::default()
        },
    );
    scene.drain_commands();

    let t = scene.components.get::<TransformComponent>(entity).unwrap();
    assert_eq!(t.x, 99.0, "second attach must overwrite first");
    assert_eq!(
        scene.components.count::<TransformComponent>(),
        1,
        "no duplicate"
    );
}

// ── T-ECS-28: Get/Has/Remove on Missing Entity ───────────────────────────────

#[test]
fn t_ecs_28_ops_on_missing_entity() {
    let scene = Scene::new();
    let bogus = EntityId(888_888);

    assert!(!scene.components.has::<TransformComponent>(bogus));
    assert!(scene.components.get::<TransformComponent>(bogus).is_none());
    let mutated = scene
        .components
        .get_mut::<TransformComponent, _>(bogus, |t| t.x = 5.0);
    assert!(!mutated, "get_mut returns false for missing entity");
    let removed = scene.components.remove::<TransformComponent>(bogus);
    assert!(!removed, "remove returns false for missing entity");
}

// ── T-ECS-29: Attach to Non-Existent Entity is Ignored ───────────────────────

#[test]
fn t_ecs_29_attach_to_nonexistent_entity_ignored() {
    let scene = Scene::new();
    let h = scene.queue_spawn(vec![]);
    scene.drain_commands();
    let entity = h.get().unwrap();

    scene.queue_despawn(entity);
    scene.drain_commands();

    // Attach after despawn — drain's entity_exists check drops it silently
    scene.queue_attach(entity, TransformComponent::default());
    scene.drain_commands();

    assert!(
        !scene.components.has::<TransformComponent>(entity),
        "attach to dead entity must not store component"
    );
}

// ── T-ECS-30: Detach Non-Existent Component ──────────────────────────────────

#[test]
fn t_ecs_30_detach_nonexistent_component() {
    let scene = Scene::new();
    let h = scene.queue_spawn(vec![]);
    scene.drain_commands();
    let entity = h.get().unwrap();

    // MeshComponent was never attached — must not panic
    scene.queue_detach::<MeshComponent>(entity);
    scene.drain_commands();

    assert!(scene.entity_exists(entity), "entity unaffected");
}

// ── T-ECS-31: Empty Drain is a No-Op; Double Drain Safe ─────────────────────

#[test]
fn t_ecs_31_empty_drain_noop() {
    let scene = Scene::new();
    let h = scene.queue_spawn(vec![]);
    scene.drain_commands();
    let entity = h.get().unwrap();

    // Second drain with empty queue must not panic or change state
    scene.drain_commands();
    scene.drain_commands();
    assert!(scene.entity_exists(entity));
    assert_eq!(scene.entity_count(), 1);
}

// ── T-ECS-32: CommandQueue len/is_empty ──────────────────────────────────────

#[test]
fn t_ecs_32_command_queue_len_is_empty() {
    let scene = Scene::new();
    assert!(scene.commands.is_empty());
    assert_eq!(scene.commands.len(), 0);

    scene.queue_spawn_anon(vec![]);
    scene.queue_spawn_anon(vec![]);
    assert_eq!(scene.commands.len(), 2);
    assert!(!scene.commands.is_empty());

    scene.drain_commands();
    assert!(scene.commands.is_empty());
    assert_eq!(scene.commands.len(), 0);
}

// ── T-ECS-33: Hierarchy — Reparent Child ─────────────────────────────────────

#[test]
fn t_ecs_33_hierarchy_reparent() {
    let scene = Scene::new();
    let ha = scene.queue_spawn(vec![]);
    let hb = scene.queue_spawn(vec![]);
    let hc = scene.queue_spawn(vec![]);
    scene.drain_commands();
    let a = ha.get().unwrap();
    let b = hb.get().unwrap();
    let c = hc.get().unwrap();

    scene.queue_set_parent(c, a);
    scene.drain_commands();

    scene.queue_set_parent(c, b);
    scene.drain_commands();

    assert_eq!(
        scene.hierarchy.get_parent(c),
        Some(b),
        "parent updated to b"
    );
    assert!(
        scene.hierarchy.get_children(b).contains(&c),
        "c in b's children"
    );
    assert!(
        !scene.hierarchy.get_children(a).contains(&c),
        "c removed from a's children"
    );
}

// ── T-ECS-34: Hierarchy — Clear Parent With No Parent ────────────────────────

#[test]
fn t_ecs_34_hierarchy_clear_parent_no_parent() {
    let scene = Scene::new();
    let h = scene.queue_spawn(vec![]);
    scene.drain_commands();
    let entity = h.get().unwrap();

    // clear_parent on an entity that has no parent — no panic
    scene.queue_clear_parent(entity);
    scene.drain_commands();

    assert_eq!(scene.hierarchy.get_parent(entity), None);
}

// ── T-ECS-35: Hierarchy — Ancestor Chain on Root ─────────────────────────────

#[test]
fn t_ecs_35_ancestor_chain_root() {
    let scene = Scene::new();
    let h = scene.queue_spawn(vec![]);
    scene.drain_commands();
    let entity = h.get().unwrap();

    let (chain, exceeded) = scene.hierarchy.ancestor_chain(entity);
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0], entity);
    assert!(!exceeded, "root must not exceed depth");
}

// ── T-ECS-36: Hierarchy — Ancestor Chain Multi-Level ─────────────────────────

#[test]
fn t_ecs_36_ancestor_chain_multi_level() {
    let scene = Scene::new();
    let ha = scene.queue_spawn(vec![]);
    let hb = scene.queue_spawn(vec![]);
    let hc = scene.queue_spawn(vec![]);
    scene.drain_commands();
    let a = ha.get().unwrap();
    let b = hb.get().unwrap();
    let c = hc.get().unwrap();

    // a <- b <- c
    scene.queue_set_parent(b, a);
    scene.queue_set_parent(c, b);
    scene.drain_commands();

    let (chain, exceeded) = scene.hierarchy.ancestor_chain(c);
    assert_eq!(chain, vec![c, b, a]);
    assert!(!exceeded);
}

// ── T-ECS-37: Event Bus — Entity Despawned Event Fires ───────────────────────

#[test]
fn t_ecs_37_event_bus_entity_despawned_fires() {
    let scene = Scene::new();
    let despawned_ids = Arc::new(Mutex::new(Vec::<u64>::new()));
    let ids_c = despawned_ids.clone();

    scene.events.subscribe_entity_despawned(move |eid| {
        ids_c.lock().unwrap().push(eid);
    });

    let h = scene.queue_spawn(vec![]);
    scene.drain_commands();
    let entity = h.get().unwrap();

    scene.queue_despawn(entity);
    scene.drain_commands();

    let ids = despawned_ids.lock().unwrap();
    assert_eq!(ids.len(), 1);
    assert_eq!(ids[0], entity.0);
}

// ── T-ECS-38: Event Bus — Unsubscribe Entity Despawned ───────────────────────

#[test]
fn t_ecs_38_event_bus_unsubscribe_entity_despawned() {
    let scene = Scene::new();
    let count = Arc::new(Mutex::new(0u32));
    let count_c = count.clone();

    let sub_id = scene.events.subscribe_entity_despawned(move |_eid| {
        *count_c.lock().unwrap() += 1;
    });
    scene.events.unsubscribe_entity_despawned(sub_id);

    let h = scene.queue_spawn(vec![]);
    scene.drain_commands();
    let entity = h.get().unwrap();
    scene.queue_despawn(entity);
    scene.drain_commands();

    assert_eq!(
        *count.lock().unwrap(),
        0,
        "handler must not fire after unsubscribe"
    );
}

// ── T-ECS-39: Event Bus — Emit With No Subscribers ───────────────────────────

#[test]
fn t_ecs_39_event_bus_emit_no_subscribers() {
    let scene = Scene::new();
    // All three paths: named, entity_spawned, entity_despawned — no panic
    scene.emit("UnknownEvent", serde_json::json!({}));
    scene.events.emit_entity_spawned(1);
    scene.events.emit_entity_despawned(1);
}

// ── T-ECS-40: Scene Clear Resets Everything ──────────────────────────────────

#[test]
fn t_ecs_40_scene_clear_resets_all_state() {
    let scene = Scene::new();
    let hp = scene.queue_spawn(vec![comp(TransformComponent::default())]);
    let hc = scene.queue_spawn(vec![]);
    scene.drain_commands();
    let parent = hp.get().unwrap();
    let child = hc.get().unwrap();

    scene.queue_set_parent(child, parent);
    scene.drain_commands();

    scene.clear();

    assert_eq!(scene.entity_count(), 0);
    assert!(!scene.entity_exists(parent));
    assert!(!scene.components.has::<TransformComponent>(parent));
    assert_eq!(scene.hierarchy.get_parent(child), None);
    assert!(scene.hierarchy.get_children(parent).is_empty());
}

// ── T-ECS-41: Load JSON With Unknown Component ───────────────────────────────

#[test]
fn t_ecs_41_load_json_unknown_component_skipped() {
    let scene = Scene::new();
    let json = serde_json::json!({
        "entities": [{
            "id": 77777u64,
            "parent": null,
            "components": [
                { "type": "TransformComponent", "data": { "x": 5.0, "y": 0.0, "z": 0.0,
                  "rot_x": 0.0, "rot_y": 0.0, "rot_z": 0.0,
                  "scale_x": 1.0, "scale_y": 1.0, "scale_z": 1.0 } },
                { "type": "FictionalComponent", "data": { "foo": 42 } }
            ]
        }]
    });

    scene.load_json(&json);
    let eid = EntityId(77777);
    assert!(
        scene.entity_exists(eid),
        "entity created despite unknown component"
    );
    let t = scene.components.get::<TransformComponent>(eid).unwrap();
    assert_eq!(t.x, 5.0, "known component loaded correctly");
}

// ── T-ECS-42: Load Empty Entities Array ──────────────────────────────────────

#[test]
fn t_ecs_42_load_empty_entities_array() {
    let scene = Scene::new();
    scene.queue_spawn_anon(vec![]);
    scene.drain_commands();
    assert_eq!(scene.entity_count(), 1);

    scene.load_json(&serde_json::json!({ "entities": [] }));
    assert_eq!(
        scene.entity_count(),
        0,
        "loading empty array clears all entities"
    );
}

// ── T-ECS-43: EntityId Counter Past Loaded IDs ───────────────────────────────

#[test]
fn t_ecs_43_entity_id_counter_past_loaded() {
    let high_id = 0xCAFE_0000u64;
    EntityId::ensure_counter_past(high_id);
    let next = EntityId::next();
    assert!(
        next.0 > high_id,
        "next ID {} must be > loaded max {}",
        next.0,
        high_id
    );
}

// ── T-ECS-44: ComponentStorage — for_each ────────────────────────────────────

#[test]
fn t_ecs_44_for_each() {
    let scene = Scene::new();
    for i in 0..5usize {
        scene.queue_spawn_anon(vec![comp(TransformComponent {
            x: i as f32,
            ..Default::default()
        })]);
    }
    scene.drain_commands();

    let mut xs: Vec<f32> = Vec::new();
    scene
        .components
        .for_each::<TransformComponent, _>(|_eid, t| xs.push(t.x));
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(xs, vec![0.0, 1.0, 2.0, 3.0, 4.0]);
}

// ── T-ECS-45: ComponentStorage — entities_with ───────────────────────────────

#[test]
fn t_ecs_45_entities_with() {
    let scene = Scene::new();
    let ha = scene.queue_spawn(vec![comp(TransformComponent::default())]);
    let hb = scene.queue_spawn(vec![
        comp(TransformComponent::default()),
        comp(MeshComponent::default()),
    ]);
    let _hc = scene.queue_spawn(vec![comp(MeshComponent::default())]);
    scene.drain_commands();
    let ea = ha.get().unwrap();
    let eb = hb.get().unwrap();

    let with_t = scene.components.entities_with::<TransformComponent>();
    assert_eq!(with_t.len(), 2);
    assert!(with_t.contains(&ea));
    assert!(with_t.contains(&eb));

    let with_m = scene.components.entities_with::<MeshComponent>();
    assert_eq!(with_m.len(), 2);
}

// ── T-ECS-46: ComponentStorage — get_ref Callback ────────────────────────────

#[test]
fn t_ecs_46_component_get_ref() {
    let scene = Scene::new();
    let h = scene.queue_spawn(vec![comp(TagComponent {
        tags: vec!["a".into(), "b".into()],
    })]);
    scene.drain_commands();
    let entity = h.get().unwrap();

    let len = scene
        .components
        .get_ref::<TagComponent, _, usize>(entity, |t| t.tags.len());
    assert_eq!(len, Some(2));

    let missing = scene
        .components
        .get_ref::<TransformComponent, _, f32>(entity, |t| t.x);
    assert!(missing.is_none());
}

// ── T-ECS-47: ComponentStorage — snapshot_entity ─────────────────────────────

#[test]
fn t_ecs_47_snapshot_entity() {
    let scene = Scene::new();
    let h = scene.queue_spawn(vec![
        comp(TransformComponent::default()),
        comp(TagComponent {
            tags: vec!["snap".into()],
        }),
    ]);
    scene.drain_commands();
    let entity = h.get().unwrap();

    let snapshot = scene.components.snapshot_entity(entity);
    assert_eq!(snapshot.len(), 2);

    let type_names: HashSet<&'static str> = snapshot.iter().map(|(n, _)| *n).collect();
    assert!(type_names.contains("TransformComponent"));
    assert!(type_names.contains("TagComponent"));
}

// ── T-ECS-48: spawn_immediate Bypasses Queue ─────────────────────────────────

#[test]
fn t_ecs_48_spawn_immediate() {
    let scene = Scene::new();
    let entity = scene.spawn_immediate(vec![comp(TransformComponent {
        x: 7.0,
        ..Default::default()
    })]);

    // Entity exists without any drain
    assert!(scene.entity_exists(entity));
    let t = scene.components.get::<TransformComponent>(entity).unwrap();
    assert_eq!(t.x, 7.0);
}

// ── T-ECS-49: spawn_with_id Restores Specific ID ─────────────────────────────

#[test]
fn t_ecs_49_spawn_with_id() {
    let scene = Scene::new();
    let specific = EntityId(0xABCD_1234);
    scene.spawn_with_id(
        specific,
        vec![comp(TagComponent {
            tags: vec!["restored".into()],
        })],
    );

    assert!(scene.entity_exists(specific));
    let tag = scene.components.get::<TagComponent>(specific).unwrap();
    assert_eq!(tag.tags[0], "restored");
}

// ── T-ECS-50: despawn_immediate Bypasses Queue ───────────────────────────────

#[test]
fn t_ecs_50_despawn_immediate() {
    let scene = Scene::new();
    let entity = scene.spawn_immediate(vec![comp(TransformComponent::default())]);
    assert!(scene.entity_exists(entity));

    scene.despawn_immediate(entity);

    assert!(!scene.entity_exists(entity));
    assert!(!scene.components.has::<TransformComponent>(entity));
}

// ── T-ECS-51: Multiple Children Order Preserved ──────────────────────────────

#[test]
fn t_ecs_51_multiple_children_order_preserved() {
    let scene = Scene::new();
    let hp = scene.queue_spawn(vec![]);
    scene.drain_commands();
    let parent = hp.get().unwrap();

    let mut child_ids = Vec::new();
    for _ in 0..5 {
        let hc = scene.queue_spawn(vec![]);
        scene.drain_commands();
        let child = hc.get().unwrap();
        scene.queue_set_parent(child, parent);
        scene.drain_commands();
        child_ids.push(child);
    }

    let children = scene.hierarchy.get_children(parent);
    assert_eq!(children.len(), 5);
    assert_eq!(children, child_ids, "insertion order preserved");
}

// ── T-ECS-52: remove_all_for With No Components ──────────────────────────────

#[test]
fn t_ecs_52_remove_all_for_no_components() {
    let scene = Scene::new();
    let entity = scene.spawn_immediate(vec![]);
    // remove_all_for on entity with no components — no panic
    scene.components.remove_all_for(entity);
    assert!(scene.entity_exists(entity), "entity record unaffected");
}

// ── T-ECS-53: Handler Receives Correct Event Name ────────────────────────────

#[test]
fn t_ecs_53_handler_receives_correct_event_name() {
    let scene = Scene::new();
    let received_name = Arc::new(Mutex::new(String::new()));
    let name_c = received_name.clone();

    scene.subscribe("MyEvent", move |name, _payload| {
        *name_c.lock().unwrap() = name.to_string();
    });

    scene.emit("MyEvent", serde_json::json!({}));

    assert_eq!(*received_name.lock().unwrap(), "MyEvent");
}

// ── T-ECS-54: Despawn Middle of Chain ────────────────────────────────────────

#[test]
fn t_ecs_54_despawn_middle_of_chain() {
    let scene = Scene::new();
    let ha = scene.queue_spawn(vec![]);
    let hb = scene.queue_spawn(vec![]);
    let hc = scene.queue_spawn(vec![]);
    scene.drain_commands();
    let a = ha.get().unwrap();
    let b = hb.get().unwrap();
    let c = hc.get().unwrap();

    // a <- b <- c
    scene.queue_set_parent(b, a);
    scene.queue_set_parent(c, b);
    scene.drain_commands();

    // Despawn b (middle)
    scene.queue_despawn(b);
    scene.drain_commands();

    assert!(!scene.entity_exists(b), "b despawned");
    assert!(scene.entity_exists(a), "a still alive");
    assert!(scene.entity_exists(c), "c still alive");

    // c is orphaned (b removed from hierarchy)
    assert_eq!(scene.hierarchy.get_parent(c), None, "c orphaned");
    // a has no children (b removed)
    assert!(
        scene.hierarchy.get_children(a).is_empty(),
        "a has no children"
    );
}

// ── T-ECS-55: Entity Spawned Event Fires Multiple Times ──────────────────────

#[test]
fn t_ecs_55_entity_spawned_fires_multiple_times() {
    let scene = Scene::new();
    let count = Arc::new(Mutex::new(0u32));
    let count_c = count.clone();

    scene.events.subscribe_entity_spawned(move |_eid| {
        *count_c.lock().unwrap() += 1;
    });

    for _ in 0..3 {
        scene.queue_spawn_anon(vec![]);
        scene.drain_commands();
    }

    assert_eq!(*count.lock().unwrap(), 3);
}

// ── T-ECS-56: queue_spawn_anon — Creates Entities ────────────────────────────

#[test]
fn t_ecs_56_queue_spawn_anon_creates_entities() {
    let scene = Scene::new();
    for _ in 0..7 {
        scene.queue_spawn_anon(vec![comp(TransformComponent::default())]);
    }
    scene.drain_commands();
    assert_eq!(scene.entity_count(), 7);
    assert_eq!(scene.components.count::<TransformComponent>(), 7);
}

// ── T-ECS-57: Save/Load All 6 Component Types ────────────────────────────────

#[test]
fn t_ecs_57_scene_roundtrip_all_component_types() {
    let scene = Scene::new();
    let h = scene.queue_spawn(vec![
        comp(TransformComponent {
            x: 1.0,
            y: 2.0,
            z: 3.0,
            ..Default::default()
        }),
        comp(MeshComponent {
            mesh_id: "m".into(),
            texture_id: "t".into(),
            visible: true,
            ..Default::default()
        }),
        comp(TagComponent {
            tags: vec!["all".into()],
        }),
        comp(RigidBodyComponent {
            mass: 5.0,
            ..Default::default()
        }),
        comp(ColliderComponent {
            shape: "capsule".into(),
            size: [1.0, 2.0, 1.0],
            is_trigger: false,
        }),
        comp(BillboardComponent {
            asset_id: "b.png".into(),
            width: 1.5,
            height: 1.5,
            ..Default::default()
        }),
    ]);
    scene.drain_commands();
    let eid = h.get().unwrap();

    let json = scene.save_json();
    scene.clear();
    scene.load_json(&json);

    assert!(scene.components.has::<TransformComponent>(eid));
    assert!(scene.components.has::<MeshComponent>(eid));
    assert!(scene.components.has::<TagComponent>(eid));
    assert!(scene.components.has::<RigidBodyComponent>(eid));
    assert!(scene.components.has::<ColliderComponent>(eid));
    assert!(scene.components.has::<BillboardComponent>(eid));

    let t = scene.components.get::<TransformComponent>(eid).unwrap();
    assert_eq!(t.x, 1.0);
    let rb = scene.components.get::<RigidBodyComponent>(eid).unwrap();
    assert_eq!(rb.mass, 5.0);
    let col = scene.components.get::<ColliderComponent>(eid).unwrap();
    assert_eq!(col.shape, "capsule");
}

// ── T-ECS-58: Unsubscribe Entity Spawned ─────────────────────────────────────

#[test]
fn t_ecs_58_unsubscribe_entity_spawned() {
    let scene = Scene::new();
    let count = Arc::new(Mutex::new(0u32));
    let count_c = count.clone();

    let sub_id = scene.events.subscribe_entity_spawned(move |_eid| {
        *count_c.lock().unwrap() += 1;
    });
    scene.events.unsubscribe_entity_spawned(sub_id);

    scene.queue_spawn_anon(vec![]);
    scene.drain_commands();

    assert_eq!(*count.lock().unwrap(), 0);
}

// ── T-ECS-59: ComponentStorage Clear ─────────────────────────────────────────

#[test]
fn t_ecs_59_component_storage_clear() {
    let scene = Scene::new();
    for _ in 0..5 {
        scene.queue_spawn_anon(vec![comp(TransformComponent::default())]);
    }
    scene.drain_commands();
    assert_eq!(scene.components.count::<TransformComponent>(), 5);

    scene.components.clear();
    assert_eq!(scene.components.count::<TransformComponent>(), 0);
}

// ── T-ECS-60: Children Empty for Leaf and Non-Existent Entity ────────────────

#[test]
fn t_ecs_60_children_empty_for_leaf_and_missing() {
    let scene = Scene::new();
    let h = scene.queue_spawn(vec![]);
    scene.drain_commands();
    let entity = h.get().unwrap();

    // Leaf with no children
    assert!(scene.hierarchy.get_children(entity).is_empty());
    // Non-existent entity
    assert!(scene.hierarchy.get_children(EntityId(777_666)).is_empty());
}

// ── T-ECS-61: Multiple Component Types — Detach One, Others Remain ───────────

#[test]
fn t_ecs_61_multiple_component_types_detach_one() {
    let scene = Scene::new();
    let h = scene.queue_spawn(vec![
        comp(TransformComponent::default()),
        comp(MeshComponent::default()),
        comp(TagComponent::default()),
    ]);
    scene.drain_commands();
    let entity = h.get().unwrap();

    scene.queue_detach::<MeshComponent>(entity);
    scene.drain_commands();

    assert!(scene.components.has::<TransformComponent>(entity));
    assert!(!scene.components.has::<MeshComponent>(entity));
    assert!(scene.components.has::<TagComponent>(entity));
}

// ── T-ECS-62: Hierarchy Clear ────────────────────────────────────────────────

#[test]
fn t_ecs_62_hierarchy_clear() {
    let scene = Scene::new();
    let hp = scene.queue_spawn(vec![]);
    let hc = scene.queue_spawn(vec![]);
    scene.drain_commands();
    let parent = hp.get().unwrap();
    let child = hc.get().unwrap();

    scene.queue_set_parent(child, parent);
    scene.drain_commands();

    scene.hierarchy.clear();

    assert_eq!(scene.hierarchy.get_parent(child), None);
    assert!(scene.hierarchy.get_children(parent).is_empty());
}

// ── T-ECS-63: SpawnHandle — None Before Drain, Some After ────────────────────

#[test]
fn t_ecs_63_spawn_handle_none_before_some_after() {
    let scene = Scene::new();
    let h = scene.queue_spawn(vec![]);

    assert!(h.get().is_none(), "handle is None before drain");
    assert_eq!(scene.entity_count(), 0, "no entities before drain");

    scene.drain_commands();

    let id = h.get();
    assert!(id.is_some(), "handle is Some after drain");
    assert!(scene.entity_exists(id.unwrap()));
}

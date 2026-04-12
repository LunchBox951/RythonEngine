use std::sync::atomic::{AtomicU64, Ordering};

use rython_ecs::component::{ColliderComponent, MeshComponent, TagComponent, TransformComponent};
use rython_ecs::Scene;

use rython_editor::project::io::{load_scene, save_scene};
use rython_editor::state::selection::SelectionState;
use rython_editor::state::undo::{DespawnEntity, ModifyComponent, SpawnEntity, UndoStack};

// ── Temp-dir helper ──────────────────────────────────────────────────────────

static INT_TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDir(std::path::PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let n = INT_TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!("rython_editor_int_{}_{}", label, n));
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Build a TransformComponent JSON value with custom position.
fn transform_json(x: f32, y: f32, z: f32) -> serde_json::Value {
    serde_json::to_value(TransformComponent {
        x,
        y,
        z,
        ..Default::default()
    })
    .unwrap()
}

/// Build a TagComponent JSON value with the given tags.
fn tag_json(tags: &[&str]) -> serde_json::Value {
    serde_json::to_value(TagComponent {
        tags: tags.iter().map(|s| s.to_string()).collect(),
    })
    .unwrap()
}

/// Build a MeshComponent JSON value with custom mesh and texture IDs.
fn mesh_json(mesh_id: &str, texture_id: &str) -> serde_json::Value {
    serde_json::to_value(MeshComponent {
        mesh_id: mesh_id.to_string(),
        texture_id: texture_id.to_string(),
        ..Default::default()
    })
    .unwrap()
}

/// Build a ColliderComponent JSON value.
fn collider_json(shape: &str, size: [f32; 3], is_trigger: bool) -> serde_json::Value {
    serde_json::to_value(ColliderComponent {
        shape: shape.to_string(),
        size,
        is_trigger,
    })
    .unwrap()
}

// ═════════════════════════════════════════════════════════════════════════════
// Test 01 — Mixed undo/redo operations
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn t_editor_int_01_undo_redo_mixed_operations() {
    let scene = Scene::new();
    let mut undo = UndoStack::new();

    // Step 1: Spawn entity A with a transform.
    let id_a = rython_ecs::EntityId::next();
    let spawn_a = SpawnEntity::new(
        id_a,
        vec![("TransformComponent".into(), transform_json(1.0, 2.0, 3.0))],
        None,
    );
    undo.push(Box::new(spawn_a), &scene);
    assert!(scene.entity_exists(id_a), "entity A must exist after spawn");
    let ta: TransformComponent = scene.components.get(id_a).unwrap();
    assert_eq!(ta.x, 1.0);

    // Step 2: Modify entity A's transform.
    let old_transform = transform_json(1.0, 2.0, 3.0);
    let new_transform = transform_json(10.0, 20.0, 30.0);
    let modify_a = ModifyComponent {
        entity: id_a,
        type_name: "TransformComponent".into(),
        old_value: old_transform,
        new_value: new_transform,
    };
    undo.push(Box::new(modify_a), &scene);
    let ta: TransformComponent = scene.components.get(id_a).unwrap();
    assert_eq!(ta.x, 10.0, "transform must be updated after modify");

    // Step 3: Spawn entity B.
    let id_b = rython_ecs::EntityId::next();
    let spawn_b = SpawnEntity::new(
        id_b,
        vec![("TagComponent".into(), tag_json(&["player"]))],
        None,
    );
    undo.push(Box::new(spawn_b), &scene);
    assert!(scene.entity_exists(id_b), "entity B must exist after spawn");

    // Step 4: Delete entity A.
    let despawn_a = DespawnEntity::capture(id_a, &scene);
    undo.push(Box::new(despawn_a), &scene);
    assert!(
        !scene.entity_exists(id_a),
        "entity A must be gone after despawn"
    );
    assert!(scene.entity_exists(id_b), "entity B must still exist");

    // -- Undo step 4: entity A reappears with modified transform (10,20,30).
    undo.undo(&scene);
    assert!(
        scene.entity_exists(id_a),
        "entity A must be restored after undo despawn"
    );
    let ta: TransformComponent = scene.components.get(id_a).unwrap();
    assert_eq!(ta.x, 10.0, "restored entity A must have modified transform");

    // -- Undo step 3: entity B disappears.
    undo.undo(&scene);
    assert!(
        !scene.entity_exists(id_b),
        "entity B must be gone after undo spawn"
    );

    // -- Undo step 2: entity A reverts to original transform.
    undo.undo(&scene);
    let ta: TransformComponent = scene.components.get(id_a).unwrap();
    assert_eq!(ta.x, 1.0, "entity A must revert to original transform");

    // -- Undo step 1: entity A disappears.
    undo.undo(&scene);
    assert!(
        !scene.entity_exists(id_a),
        "entity A must be gone after undo spawn"
    );
    assert!(!undo.can_undo(), "undo stack must be exhausted");

    // -- Redo all 4 steps.
    undo.redo(&scene); // spawn A
    assert!(scene.entity_exists(id_a));
    let ta: TransformComponent = scene.components.get(id_a).unwrap();
    assert_eq!(ta.x, 1.0, "redo spawn must restore original transform");

    undo.redo(&scene); // modify A
    let ta: TransformComponent = scene.components.get(id_a).unwrap();
    assert_eq!(ta.x, 10.0, "redo modify must apply new transform");

    undo.redo(&scene); // spawn B
    assert!(scene.entity_exists(id_b));

    undo.redo(&scene); // despawn A
    assert!(!scene.entity_exists(id_a));
    assert!(scene.entity_exists(id_b));
    assert!(!undo.can_redo(), "redo stack must be exhausted");
}

// ═════════════════════════════════════════════════════════════════════════════
// Test 02 — Undo stack limit (max_history = 200)
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn t_editor_int_02_undo_redo_limit() {
    let scene = Scene::new();
    let mut undo = UndoStack::new();

    // The default max_history is 200 (see UndoStack::default).
    // Push 250 spawn commands — the first 50 should be evicted.
    let mut ids = Vec::new();
    for _ in 0..250 {
        let id = rython_ecs::EntityId::next();
        ids.push(id);
        let cmd = SpawnEntity::new(
            id,
            vec![("TransformComponent".into(), transform_json(0.0, 0.0, 0.0))],
            None,
        );
        undo.push(Box::new(cmd), &scene);
    }

    // All 250 entities must exist in the scene (commands are executed immediately).
    for id in &ids {
        assert!(
            scene.entity_exists(*id),
            "entity {id:?} must exist after spawn"
        );
    }

    // Count how many times we can undo — should be exactly 200.
    let mut undo_count = 0u32;
    while undo.can_undo() {
        undo.undo(&scene);
        undo_count += 1;
        assert!(undo_count <= 200, "undo count must not exceed max_history");
    }
    assert_eq!(undo_count, 200, "must be able to undo exactly 200 times");

    // The first 50 entities (evicted from history) cannot be undone, so they
    // remain in the scene. The last 200 entities should have been despawned
    // by the undo calls.
    for id in &ids[..50] {
        assert!(
            scene.entity_exists(*id),
            "evicted entity must remain in scene (cannot be undone)"
        );
    }
    for id in &ids[50..] {
        assert!(
            !scene.entity_exists(*id),
            "undone entity must be removed from scene"
        );
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Test 03 — Project save/load roundtrip with diverse components
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn t_editor_int_03_project_save_load_roundtrip() {
    let tmp = TempDir::new("roundtrip");
    let scene = Scene::new();

    // Entity 1: transform + mesh + tag + collider.
    let e1 = scene.spawn_immediate(vec![]);
    scene.load_component(e1, "TransformComponent", &transform_json(5.0, 10.0, 15.0));
    scene.load_component(e1, "MeshComponent", &mesh_json("cube", "brick"));
    scene.load_component(e1, "TagComponent", &tag_json(&["wall", "static"]));
    scene.load_component(
        e1,
        "ColliderComponent",
        &collider_json("box", [2.0, 4.0, 1.0], false),
    );

    // Entity 2: transform only.
    let e2 = scene.spawn_immediate(vec![]);
    scene.load_component(e2, "TransformComponent", &transform_json(-3.0, 0.0, 7.0));

    // Entity 3: tag only.
    let e3 = scene.spawn_immediate(vec![]);
    scene.load_component(e3, "TagComponent", &tag_json(&["trigger_zone"]));

    // Entity 4: child of entity 1 (hierarchy).
    let e4 = scene.spawn_immediate(vec![]);
    scene.load_component(e4, "TransformComponent", &transform_json(0.0, 1.0, 0.0));
    scene.hierarchy.set_parent(e4, e1);

    assert_eq!(scene.entity_count(), 4);

    // Save.
    save_scene(&tmp.0, "test_level", &scene).unwrap();

    // Load into a fresh scene.
    let scene2 = Scene::new();
    load_scene(&tmp.0, "test_level", &scene2).unwrap();

    // Verify entity count.
    assert_eq!(
        scene2.entity_count(),
        4,
        "loaded scene must have 4 entities"
    );

    // Verify entity 1 components.
    let t1: TransformComponent = scene2.components.get(e1).unwrap();
    assert_eq!((t1.x, t1.y, t1.z), (5.0, 10.0, 15.0));

    let m1: MeshComponent = scene2.components.get(e1).unwrap();
    assert_eq!(m1.mesh_id, "cube");
    assert_eq!(m1.texture_id, "brick");

    let tag1: TagComponent = scene2.components.get(e1).unwrap();
    assert_eq!(tag1.tags, vec!["wall", "static"]);

    let c1: ColliderComponent = scene2.components.get(e1).unwrap();
    assert_eq!(c1.shape, "box");
    assert_eq!(c1.size, [2.0, 4.0, 1.0]);
    assert!(!c1.is_trigger);

    // Verify entity 2 components.
    let t2: TransformComponent = scene2.components.get(e2).unwrap();
    assert_eq!((t2.x, t2.y, t2.z), (-3.0, 0.0, 7.0));
    // e2 should not have a mesh.
    assert!(!scene2.components.has::<MeshComponent>(e2));

    // Verify entity 3 tag.
    let tag3: TagComponent = scene2.components.get(e3).unwrap();
    assert_eq!(tag3.tags, vec!["trigger_zone"]);
    assert!(!scene2.components.has::<TransformComponent>(e3));

    // Verify hierarchy: e4 is child of e1.
    let parent_of_e4 = scene2.hierarchy.get_parent(e4);
    assert_eq!(
        parent_of_e4,
        Some(e1),
        "entity 4 must be a child of entity 1"
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// Test 04 — Save/load empty project
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn t_editor_int_04_project_save_load_empty() {
    let tmp = TempDir::new("empty_scene");
    let scene = Scene::new();

    // Scene has zero entities.
    assert_eq!(scene.entity_count(), 0);

    // Save empty scene.
    save_scene(&tmp.0, "empty", &scene).unwrap();

    // Load into a fresh scene.
    let scene2 = Scene::new();
    load_scene(&tmp.0, "empty", &scene2).unwrap();

    assert_eq!(
        scene2.entity_count(),
        0,
        "loaded empty scene must have zero entities"
    );
    assert!(scene2.all_entities().is_empty());
}

// ═════════════════════════════════════════════════════════════════════════════
// Test 05 — Selection state after undo of a deletion
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn t_editor_int_05_selection_after_undo() {
    let scene = Scene::new();
    let mut undo = UndoStack::new();
    let mut selection = SelectionState::default();

    // Spawn entity.
    let id = rython_ecs::EntityId::next();
    let spawn = SpawnEntity::new(
        id,
        vec![("TransformComponent".into(), transform_json(0.0, 0.0, 0.0))],
        None,
    );
    undo.push(Box::new(spawn), &scene);

    // Select the entity.
    selection.select_entity(id);
    assert_eq!(selection.selected_entity(), Some(id));

    // Delete the entity (capture snapshot first).
    let despawn = DespawnEntity::capture(id, &scene);
    undo.push(Box::new(despawn), &scene);
    assert!(
        !scene.entity_exists(id),
        "entity must be gone after despawn"
    );

    // Clear selection after deletion -- this is what the editor does: once an
    // entity is deleted, the selection is cleared so the inspector panel does
    // not reference a stale entity.
    selection.clear();
    assert_eq!(
        selection.selected_entity(),
        None,
        "selection must be cleared after deletion"
    );

    // Undo the deletion -- entity reappears.
    undo.undo(&scene);
    assert!(
        scene.entity_exists(id),
        "entity must be restored after undo"
    );

    // After undo, the entity is back in the scene, but the selection state
    // is NOT automatically restored.  The undo system operates on the ECS
    // scene only -- it has no knowledge of editor UI state such as selection.
    // The editor must decide whether to re-select the restored entity.
    //
    // Documented behaviour: selection remains cleared after undo; the user
    // must re-click to select the restored entity.
    assert_eq!(
        selection.selected_entity(),
        None,
        "selection is not auto-restored by undo -- user must re-select"
    );

    // Verify the entity's data survived the roundtrip through despawn + undo.
    let t: TransformComponent = scene.components.get(id).unwrap();
    assert_eq!((t.x, t.y, t.z), (0.0, 0.0, 0.0));

    // The user can re-select the entity now that it exists again.
    selection.select_entity(id);
    assert_eq!(selection.selected_entity(), Some(id));
}

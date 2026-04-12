//! Acceptance tests for rython-scripting (T-SCRIPT-01 through T-SCRIPT-20).
//!
//! Hot-reload tests (T-SCRIPT-14..17) use #[ignore] because they depend on
//! file watcher timing and the `dev-reload` feature.

use std::ffi::CString;
use std::sync::{Arc, Mutex};

use pyo3::prelude::*;
use rython_ecs::component::{MeshComponent, TagComponent, TransformComponent};
use rython_ecs::Scene;
use rython_renderer::DrawCommand;
use rython_scripting::{
    clear_recurring_callbacks, drain_draw_commands, ensure_rython_module,
    flush_python_bg_completions, flush_python_bg_tasks, flush_python_par_tasks,
    flush_python_seq_tasks, flush_recurring_callbacks, gil_dispatch_count, register_script_class,
    reset_gil_dispatch_count, reset_quit_requested, set_active_scene, set_elapsed_secs,
    was_quit_requested, ScriptComponent, ScriptSystem,
};

// ─── Test serialisation guard ─────────────────────────────────────────────────

static TEST_MUTEX: std::sync::LazyLock<Mutex<()>> = std::sync::LazyLock::new(|| Mutex::new(()));

fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner())
}

fn setup(scene: &Arc<Scene>) {
    set_active_scene(Arc::clone(scene));
    Python::attach(|py| {
        ensure_rython_module(py, Arc::clone(scene)).expect("ensure_rython_module");
    });
}

fn spawn_with_script(scene: &Arc<Scene>, class_name: &str) -> rython_ecs::EntityId {
    use std::any::TypeId;
    let handle = scene.queue_spawn(vec![(
        TypeId::of::<ScriptComponent>(),
        Box::new(ScriptComponent {
            class_name: class_name.to_string(),
        }) as Box<dyn rython_ecs::component::Component>,
    )]);
    scene.drain_commands();
    handle.get().expect("entity id")
}

fn spawn_empty(scene: &Arc<Scene>) -> rython_ecs::EntityId {
    let handle = scene.queue_spawn(vec![]);
    scene.drain_commands();
    handle.get().expect("entity id")
}

fn spawn_with_transform_and_script(
    scene: &Arc<Scene>,
    class_name: &str,
    tc: TransformComponent,
) -> rython_ecs::EntityId {
    use std::any::TypeId;
    let handle = scene.queue_spawn(vec![
        (
            TypeId::of::<TransformComponent>(),
            Box::new(tc) as Box<dyn rython_ecs::component::Component>,
        ),
        (
            TypeId::of::<ScriptComponent>(),
            Box::new(ScriptComponent {
                class_name: class_name.to_string(),
            }) as Box<dyn rython_ecs::component::Component>,
        ),
    ]);
    scene.drain_commands();
    handle.get().expect("entity id")
}

// ─── T-SCRIPT-01: Python Module Import ───────────────────────────────────────

#[test]
fn t_script_01_python_module_import() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    Python::attach(|py| {
        py.run(c"import rython", None, None)
            .expect("import rython failed");

        let rython = py.import("rython").expect("rython");
        for attr in [
            "scene",
            "renderer",
            "physics",
            "audio",
            "input",
            "ui",
            "resources",
            "scheduler",
            "modules",
            "camera",
            "throttle",
        ] {
            let val = rython
                .getattr(attr)
                .unwrap_or_else(|_| panic!("rython.{attr} missing"));
            assert!(!val.is_none(), "rython.{attr} should not be None");
        }
    });
}

// ─── T-SCRIPT-02: Script Class Instantiation ─────────────────────────────────

#[test]
fn t_script_02_script_class_instantiation() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    let sys = ScriptSystem::new(Arc::clone(&scene));
    setup(&scene);

    Python::attach(|py| {
        py.run(
            c"
class TestScript02:
    def __init__(self, entity):
        self.entity = entity
",
            None,
            None,
        )
        .unwrap();

        let main = py.import("__main__").unwrap();
        register_script_class(
            "TestScript02",
            main.getattr("TestScript02").unwrap().unbind(),
        );

        let entity_id = spawn_with_script(&scene, "TestScript02");
        sys.flush(py);

        let inst = sys.get_instance(entity_id).expect("instance should exist");
        let py_id: u64 = inst
            .bind(py)
            .getattr("entity")
            .unwrap()
            .getattr("id")
            .unwrap()
            .extract()
            .unwrap();
        assert_eq!(py_id, entity_id.0);
    });
}

// ─── T-SCRIPT-03: on_spawn Callback ──────────────────────────────────────────

#[test]
fn t_script_03_on_spawn_callback() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    let sys = ScriptSystem::new(Arc::clone(&scene));
    setup(&scene);

    Python::attach(|py| {
        py.run(
            c"
class SpawnScript:
    def __init__(self, entity):
        self.entity = entity
        self.spawned = False
    def on_spawn(self):
        self.spawned = True
",
            None,
            None,
        )
        .unwrap();

        let main = py.import("__main__").unwrap();
        register_script_class("SpawnScript", main.getattr("SpawnScript").unwrap().unbind());

        let entity_id = spawn_with_script(&scene, "SpawnScript");
        sys.flush(py);

        let inst = sys.get_instance(entity_id).expect("instance");
        let spawned: bool = inst.bind(py).getattr("spawned").unwrap().extract().unwrap();
        assert!(spawned, "on_spawn should have been called");
    });
}

// ─── T-SCRIPT-04: on_despawn Callback ────────────────────────────────────────

#[test]
fn t_script_04_on_despawn_callback() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    let sys = ScriptSystem::new(Arc::clone(&scene));
    setup(&scene);

    Python::attach(|py| {
        py.run(
            c"
despawn_flag_04 = False
class DespawnScript:
    def __init__(self, entity):
        self.entity = entity
    def on_despawn(self):
        global despawn_flag_04
        despawn_flag_04 = True
",
            None,
            None,
        )
        .unwrap();

        let main = py.import("__main__").unwrap();
        register_script_class(
            "DespawnScript",
            main.getattr("DespawnScript").unwrap().unbind(),
        );

        let entity_id = spawn_with_script(&scene, "DespawnScript");
        sys.flush(py);

        scene.queue_despawn(entity_id);
        scene.drain_commands();
        sys.flush(py);

        let flag: bool = main.getattr("despawn_flag_04").unwrap().extract().unwrap();
        assert!(flag, "on_despawn should have been called");
    });
}

// ─── T-SCRIPT-05: on_collision Handler Wiring ────────────────────────────────

#[test]
fn t_script_05_on_collision_handler() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    let sys = ScriptSystem::new(Arc::clone(&scene));
    setup(&scene);

    Python::attach(|py| {
        py.run(
            c"
collision_called_05 = False
collision_other_id_05 = None
collision_normal_x_05 = 0.0
class CollisionScript:
    def __init__(self, entity):
        self.entity = entity
    def on_collision(self, other, normal):
        global collision_called_05, collision_other_id_05, collision_normal_x_05
        collision_called_05 = True
        collision_other_id_05 = other.id
        collision_normal_x_05 = normal.x
",
            None,
            None,
        )
        .unwrap();

        let main = py.import("__main__").unwrap();
        register_script_class(
            "CollisionScript",
            main.getattr("CollisionScript").unwrap().unbind(),
        );

        let entity_a = spawn_with_script(&scene, "CollisionScript");
        let entity_b = spawn_empty(&scene);
        sys.flush(py);

        sys.queue_collision(entity_a, entity_b, [1.0, 0.0, 0.0]);
        sys.flush(py);

        let called: bool = main
            .getattr("collision_called_05")
            .unwrap()
            .extract()
            .unwrap();
        assert!(called, "on_collision should have been called");
        let other_id: u64 = main
            .getattr("collision_other_id_05")
            .unwrap()
            .extract()
            .unwrap();
        assert_eq!(other_id, entity_b.0);
        let nx: f32 = main
            .getattr("collision_normal_x_05")
            .unwrap()
            .extract()
            .unwrap();
        assert!((nx - 1.0).abs() < 1e-5, "normal.x should be 1.0");
    });
}

// ─── T-SCRIPT-06: on_trigger_enter / on_trigger_exit ─────────────────────────

#[test]
fn t_script_06_trigger_enter_exit() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    let sys = ScriptSystem::new(Arc::clone(&scene));
    setup(&scene);

    Python::attach(|py| {
        py.run(
            c"
trigger_enter_06 = 0
trigger_exit_06 = 0
class TriggerScript:
    def __init__(self, entity):
        self.entity = entity
    def on_trigger_enter(self, other):
        global trigger_enter_06
        trigger_enter_06 += 1
    def on_trigger_exit(self, other):
        global trigger_exit_06
        trigger_exit_06 += 1
",
            None,
            None,
        )
        .unwrap();

        let main = py.import("__main__").unwrap();
        register_script_class(
            "TriggerScript",
            main.getattr("TriggerScript").unwrap().unbind(),
        );

        let entity = spawn_with_script(&scene, "TriggerScript");
        let other = spawn_empty(&scene);
        sys.flush(py);

        sys.queue_trigger_enter(entity, other);
        sys.flush(py);
        sys.queue_trigger_exit(entity, other);
        sys.flush(py);

        let enter: i64 = main.getattr("trigger_enter_06").unwrap().extract().unwrap();
        let exit: i64 = main.getattr("trigger_exit_06").unwrap().extract().unwrap();
        assert_eq!(enter, 1, "on_trigger_enter fires exactly once");
        assert_eq!(exit, 1, "on_trigger_exit fires exactly once");
    });
}

// ─── T-SCRIPT-07: on_input_action ────────────────────────────────────────────

#[test]
fn t_script_07_on_input_action() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    let sys = ScriptSystem::new(Arc::clone(&scene));
    setup(&scene);

    Python::attach(|py| {
        py.run(
            c"
input_name_07 = None
input_val_07 = None
class InputScript:
    def __init__(self, entity):
        self.entity = entity
    def on_input_action(self, action, value):
        global input_name_07, input_val_07
        input_name_07 = action
        input_val_07 = value
",
            None,
            None,
        )
        .unwrap();

        let main = py.import("__main__").unwrap();
        register_script_class("InputScript", main.getattr("InputScript").unwrap().unbind());

        let entity = spawn_with_script(&scene, "InputScript");
        sys.flush(py);

        sys.queue_input_action(entity, "jump", 1.0);
        sys.flush(py);

        let name: String = main.getattr("input_name_07").unwrap().extract().unwrap();
        let val: f32 = main.getattr("input_val_07").unwrap().extract().unwrap();
        assert_eq!(name, "jump");
        assert!((val - 1.0).abs() < 1e-5);
    });
}

// ─── T-SCRIPT-08: Custom Event from Python ───────────────────────────────────

#[test]
fn t_script_08_custom_event_from_python() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    Python::attach(|py| {
        py.run(
            c"
import rython
received_data_08 = None
def on_my_event(**kwargs):
    global received_data_08
    received_data_08 = kwargs.get('data')

rython.scene.subscribe('MyEvent08', on_my_event)
rython.scene.emit('MyEvent08', data=42)
",
            None,
            None,
        )
        .unwrap();

        let main = py.import("__main__").unwrap();
        let data: i64 = main.getattr("received_data_08").unwrap().extract().unwrap();
        assert_eq!(data, 42);
    });
}

// ─── T-SCRIPT-09: Entity Wrapper — Transform Read/Write ──────────────────────

#[test]
fn t_script_09_entity_transform_read_write() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    use std::any::TypeId;
    let handle = scene.queue_spawn(vec![(
        TypeId::of::<TransformComponent>(),
        Box::new(TransformComponent::default()) as Box<dyn rython_ecs::component::Component>,
    )]);
    scene.drain_commands();
    let entity_id = handle.get().expect("entity id");

    Python::attach(|py| {
        let code = format!(
            "import rython\nentity = rython.Entity.__new__(rython.Entity)\nentity.id = {}\nentity.transform.x = 15.0\nassert abs(entity.transform.x - 15.0) < 1e-5",
            entity_id.0
        );
        let cstr = CString::new(code).unwrap();
        py.run(cstr.as_c_str(), None, None).expect("transform r/w");
    });

    let x = scene
        .components
        .get_ref::<TransformComponent, _, _>(entity_id, |t| t.x)
        .expect("transform component");
    assert!((x - 15.0).abs() < 1e-5, "ECS x should be 15.0, got {x}");
}

// ─── T-SCRIPT-10: Entity Wrapper — Tag Operations ────────────────────────────

#[test]
fn t_script_10_entity_tag_operations() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    let handle = scene.queue_spawn(vec![]);
    scene.drain_commands();
    let entity_id = handle.get().expect("entity id");

    Python::attach(|py| {
        let code = format!(
            "import rython\nentity = rython.Entity.__new__(rython.Entity)\nentity.id = {eid}\nentity.add_tag('test')\nassert entity.has_tag('test')\nassert not entity.has_tag('nonexistent')",
            eid = entity_id.0
        );
        let cstr = CString::new(code).unwrap();
        py.run(cstr.as_c_str(), None, None).expect("tag operations");
    });
}

// ─── T-SCRIPT-11: Vec3 Wrapper — Arithmetic ──────────────────────────────────

#[test]
fn t_script_11_vec3_arithmetic() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    Python::attach(|py| {
        py.run(
            c"
import rython
a = rython.Vec3(1, 2, 3)
b = rython.Vec3(4, 5, 6)
c = a + b
assert c.x == 5
assert c.y == 7
assert c.z == 9
scaled = a * 2.0
assert abs(scaled.x - 2.0) < 1e-5
length = rython.Vec3(3, 4, 0).length()
assert abs(length - 5.0) < 1e-5
",
            None,
            None,
        )
        .expect("Vec3 arithmetic failed");
    });
}

// ─── T-SCRIPT-12: Python Exception Does Not Crash Engine ─────────────────────

#[test]
fn t_script_12_python_exception_no_crash() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    let sys = ScriptSystem::new(Arc::clone(&scene));
    setup(&scene);

    Python::attach(|py| {
        py.run(
            c"
class CrashScript:
    def __init__(self, entity):
        self.entity = entity
    def on_collision(self, other, normal):
        raise ValueError('test error')
",
            None,
            None,
        )
        .unwrap();

        let main = py.import("__main__").unwrap();
        register_script_class("CrashScript", main.getattr("CrashScript").unwrap().unbind());

        let entity_a = spawn_with_script(&scene, "CrashScript");
        let entity_b = spawn_empty(&scene);
        sys.flush(py);

        sys.queue_collision(entity_a, entity_b, [0.0, 1.0, 0.0]);
        sys.flush(py); // should not panic

        let errors = sys.drain_errors();
        assert!(!errors.is_empty(), "error should be logged");
        let combined = errors.join("\n");
        assert!(
            combined.contains("ValueError"),
            "log must contain 'ValueError'"
        );
        assert!(
            combined.contains("test error"),
            "log must contain 'test error'"
        );
    });
}

// ─── T-SCRIPT-13: Multiple Script Errors Per Frame ───────────────────────────

#[test]
fn t_script_13_multiple_errors_per_frame() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    let sys = ScriptSystem::new(Arc::clone(&scene));
    setup(&scene);

    Python::attach(|py| {
        py.run(
            c"
class MultiErrorScript:
    def __init__(self, entity):
        self.entity = entity
    def on_collision(self, other, normal):
        raise RuntimeError('multi error')
",
            None,
            None,
        )
        .unwrap();

        let main = py.import("__main__").unwrap();
        register_script_class(
            "MultiErrorScript",
            main.getattr("MultiErrorScript").unwrap().unbind(),
        );

        let dummy = spawn_empty(&scene);
        let entities: Vec<_> = (0..3)
            .map(|_| spawn_with_script(&scene, "MultiErrorScript"))
            .collect();
        sys.flush(py);

        for &eid in &entities {
            sys.queue_collision(eid, dummy, [0.0, 0.0, 1.0]);
        }
        sys.flush(py);

        let errors = sys.drain_errors();
        let error_count = errors.iter().filter(|e| e.contains("multi error")).count();
        assert!(error_count >= 3, "expected >= 3 errors, got {error_count}");
    });
}

// ─── T-SCRIPT-14: Hot-Reload — File Change Detection ─────────────────────────

#[test]
#[ignore = "timing-sensitive: requires dev-reload feature and file watcher"]
fn t_script_14_hot_reload_file_change_detection() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    let sys = ScriptSystem::new(Arc::clone(&scene));
    setup(&scene);

    Python::attach(|py| {
        // Create a temp file with version 1 of a script class
        let tmp = tempfile::tempdir().expect("tempdir");
        let script_path = tmp.path().join("hot_reload_14.py");
        std::fs::write(
            &script_path,
            b"class HotReload14:\n    VERSION = 1\n    def __init__(self, entity):\n        self.entity = entity\n",
        )
        .unwrap();

        // Load the script file into Python and register the class
        let code_v1 = std::fs::read_to_string(&script_path).unwrap();
        let cstr_v1 = CString::new(code_v1).unwrap();
        py.run(cstr_v1.as_c_str(), None, None).unwrap();

        let main = py.import("__main__").unwrap();
        register_script_class("HotReload14", main.getattr("HotReload14").unwrap().unbind());

        let entity = spawn_with_script(&scene, "HotReload14");
        sys.flush(py);

        // Verify V1 is active
        let v1: i64 = main
            .getattr("HotReload14")
            .unwrap()
            .getattr("VERSION")
            .unwrap()
            .extract()
            .unwrap();
        assert_eq!(v1, 1, "version 1 should be loaded initially");

        // Simulate file change: write version 2 to the same file
        std::fs::write(
            &script_path,
            b"class HotReload14:\n    VERSION = 2\n    def __init__(self, entity):\n        self.entity = entity\n",
        )
        .unwrap();

        // Re-read and re-execute the updated file (simulating what the file watcher does)
        let code_v2 = std::fs::read_to_string(&script_path).unwrap();
        let cstr_v2 = CString::new(code_v2).unwrap();
        py.run(cstr_v2.as_c_str(), None, None).unwrap();

        let class_v2 = main.getattr("HotReload14").unwrap().unbind();
        sys.reload_entity_script(py, entity, class_v2)
            .expect("reload v2");

        // Verify V2 is now active
        let v2: i64 = main
            .getattr("HotReload14")
            .unwrap()
            .getattr("VERSION")
            .unwrap()
            .extract()
            .unwrap();
        assert_eq!(v2, 2, "version 2 should be loaded after file change");
    });
}

// ─── T-SCRIPT-15: Hot-Reload — Handler Rebinding ─────────────────────────────

#[test]
#[ignore = "requires dev-reload feature"]
fn t_script_15_hot_reload_handler_rebinding() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    let sys = ScriptSystem::new(Arc::clone(&scene));
    setup(&scene);

    Python::attach(|py| {
        py.run(
            c"
_flag_v1_15 = False
_flag_v2_15 = False
class HotScriptV1:
    def __init__(self, entity):
        self.entity = entity
    def on_collision(self, other, normal):
        global _flag_v1_15
        _flag_v1_15 = True

class HotScriptV2:
    def __init__(self, entity):
        self.entity = entity
    def on_collision(self, other, normal):
        global _flag_v2_15
        _flag_v2_15 = True
",
            None,
            None,
        )
        .unwrap();

        let main = py.import("__main__").unwrap();
        register_script_class("HotScriptV1", main.getattr("HotScriptV1").unwrap().unbind());

        let entity = spawn_with_script(&scene, "HotScriptV1");
        let dummy = spawn_empty(&scene);
        sys.flush(py);

        let class_v2 = main.getattr("HotScriptV2").unwrap().unbind();
        sys.reload_entity_script(py, entity, class_v2)
            .expect("reload");

        sys.queue_collision(entity, dummy, [1.0, 0.0, 0.0]);
        sys.flush(py);

        let v2: bool = main.getattr("_flag_v2_15").unwrap().extract().unwrap();
        let v1: bool = main.getattr("_flag_v1_15").unwrap().extract().unwrap();
        assert!(v2, "new handler (v2) should be active");
        assert!(!v1, "old handler (v1) should be gone");
    });
}

// ─── T-SCRIPT-16: Hot-Reload — Syntax Error Resilience ───────────────────────

#[test]
#[ignore = "requires dev-reload feature and file watcher"]
fn t_script_16_hot_reload_syntax_error_resilience() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    let sys = ScriptSystem::new(Arc::clone(&scene));
    setup(&scene);

    Python::attach(|py| {
        // Create a temp file with valid Python syntax
        let tmp = tempfile::tempdir().expect("tempdir");
        let script_path = tmp.path().join("resilience_16.py");
        std::fs::write(
            &script_path,
            b"class Resilience16:\n    VERSION = 1\n    def __init__(self, entity):\n        self.entity = entity\n    def on_collision(self, other, normal):\n        pass\n",
        )
        .unwrap();

        // Load and register the valid class
        let code_v1 = std::fs::read_to_string(&script_path).unwrap();
        let cstr_v1 = CString::new(code_v1).unwrap();
        py.run(cstr_v1.as_c_str(), None, None).unwrap();

        let main = py.import("__main__").unwrap();
        register_script_class(
            "Resilience16",
            main.getattr("Resilience16").unwrap().unbind(),
        );

        let entity = spawn_with_script(&scene, "Resilience16");
        let dummy = spawn_empty(&scene);
        sys.flush(py);

        // Verify the valid script works (collision dispatches without error)
        sys.queue_collision(entity, dummy, [0.0, 1.0, 0.0]);
        sys.flush(py);
        let errors_before = sys.drain_errors();
        assert!(
            errors_before.is_empty(),
            "valid script should produce no errors"
        );

        // Write invalid Python syntax to the same file
        std::fs::write(
            &script_path,
            b"class Resilience16:\n    def __init__(self BROKEN\n",
        )
        .unwrap();

        // Attempt to reload from the broken file — py.run should fail
        let code_bad = std::fs::read_to_string(&script_path).unwrap();
        let cstr_bad = CString::new(code_bad).unwrap();
        let reload_result = py.run(cstr_bad.as_c_str(), None, None);

        // The reload should fail due to syntax error
        assert!(
            reload_result.is_err(),
            "loading invalid syntax should return an error"
        );

        // The old valid script should still be active — test by dispatching another collision
        sys.queue_collision(entity, dummy, [1.0, 0.0, 0.0]);
        sys.flush(py);

        // The old instance should still be present and handle events
        let instance = sys.get_instance(entity);
        assert!(
            instance.is_some(),
            "old script instance should still be active after failed reload"
        );
    });
}

// ─── T-SCRIPT-17: Hot-Reload — Entity State Preserved ────────────────────────

#[test]
#[ignore = "requires dev-reload feature"]
fn t_script_17_hot_reload_entity_state_preserved() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    let sys = ScriptSystem::new(Arc::clone(&scene));
    setup(&scene);

    Python::attach(|py| {
        py.run(
            c"
class PersistScript17:
    def __init__(self, entity):
        self.entity = entity
",
            None,
            None,
        )
        .unwrap();

        let main = py.import("__main__").unwrap();
        register_script_class(
            "PersistScript17",
            main.getattr("PersistScript17").unwrap().unbind(),
        );

        let entity = spawn_with_transform_and_script(
            &scene,
            "PersistScript17",
            TransformComponent {
                x: 10.0,
                y: 20.0,
                z: 30.0,
                ..Default::default()
            },
        );
        sys.flush(py);

        let class2 = main.getattr("PersistScript17").unwrap().unbind();
        sys.reload_entity_script(py, entity, class2)
            .expect("reload");

        let x = scene
            .components
            .get_ref::<TransformComponent, _, _>(entity, |t| t.x)
            .unwrap();
        let y = scene
            .components
            .get_ref::<TransformComponent, _, _>(entity, |t| t.y)
            .unwrap();
        let z = scene
            .components
            .get_ref::<TransformComponent, _, _>(entity, |t| t.z)
            .unwrap();
        assert!((x - 10.0).abs() < 1e-5);
        assert!((y - 20.0).abs() < 1e-5);
        assert!((z - 30.0).abs() < 1e-5);
    });
}

// ─── T-SCRIPT-18: Release Mode — Bundle Loading ──────────────────────────────

#[test]
fn t_script_18_release_bundle_loading() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    let tmp = tempfile::tempdir().expect("tempdir");
    let module_path = tmp.path().join("bundle_test_18.py");
    let bundle_path = tmp.path().join("scripts18.zip");
    std::fs::write(&module_path, b"VALUE_18 = 99\n").unwrap();

    let bundle_str = bundle_path.to_str().unwrap().to_string();
    let module_str = module_path.to_str().unwrap().to_string();

    Python::attach(|py| {
        // Build zip using Python
        let code = format!(
            "import zipfile\nwith zipfile.ZipFile(r'{b}', 'w') as z:\n    z.write(r'{s}', 'bundle_test_18.py')\n",
            b = bundle_str, s = module_str
        );
        let cstr = CString::new(code).unwrap();
        py.run(cstr.as_c_str(), None, None).expect("zip build");

        rython_scripting::load_bundle(py, &bundle_str).expect("load_bundle");
        let module = py.import("bundle_test_18").expect("import from bundle");
        let val: i64 = module.getattr("VALUE_18").unwrap().extract().unwrap();
        assert_eq!(val, 99);

        let rython = py.import("rython").expect("rython");
        assert!(
            !rython.getattr("scene").unwrap().is_none(),
            "rython.scene accessible"
        );

        // Clean up
        let sys = py.import("sys").unwrap();
        sys.getattr("path")
            .unwrap()
            .call_method1("remove", (&bundle_str,))
            .ok();
        sys.getattr("modules")
            .unwrap()
            .del_item("bundle_test_18")
            .ok();
    });
}

// ─── T-SCRIPT-19: GIL Batch Acquisition ──────────────────────────────────────

#[test]
fn t_script_19_gil_batch_acquisition() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    let sys = ScriptSystem::new(Arc::clone(&scene));
    setup(&scene);

    Python::attach(|py| {
        py.run(
            c"
class BatchScript19:
    def __init__(self, entity):
        self.entity = entity
    def on_collision(self, other, normal):
        pass
",
            None,
            None,
        )
        .unwrap();

        let main = py.import("__main__").unwrap();
        register_script_class(
            "BatchScript19",
            main.getattr("BatchScript19").unwrap().unbind(),
        );

        let entity = spawn_with_script(&scene, "BatchScript19");
        let dummy = spawn_empty(&scene);
        sys.flush(py);

        reset_gil_dispatch_count();
        for _ in 0..50 {
            sys.queue_collision(entity, dummy, [0.0, 1.0, 0.0]);
        }

        // GAME_UPDATE batch
        sys.flush(py);
        // GAME_LATE batch (nothing queued)
        sys.flush(py);

        let count = gil_dispatch_count();
        assert!(
            count <= 2,
            "GIL acquired at most 2 times per frame, got {count}"
        );
    });
}

// ─── T-SCRIPT-21: spawn with mesh kwarg → MeshComponent ──────────────────────

#[test]
fn t_script_21_spawn_with_mesh_kwarg() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    Python::attach(|py| {
        py.run(
            c"
import rython
entity_21 = rython.scene.spawn(
    transform=rython.Transform(x=1.0),
    mesh='cube_mesh',
)
",
            None,
            None,
        )
        .expect("spawn with mesh kwarg");

        let main = py.import("__main__").unwrap();
        let eid: u64 = main
            .getattr("entity_21")
            .unwrap()
            .getattr("id")
            .unwrap()
            .extract()
            .unwrap();
        let mesh = scene
            .components
            .get::<MeshComponent>(rython_ecs::EntityId(eid));
        assert!(mesh.is_some(), "MeshComponent should be present");
        assert_eq!(mesh.unwrap().mesh_id, "cube_mesh");
    });
}

// ─── T-SCRIPT-22: spawn with tags kwarg → TagComponent ───────────────────────

#[test]
fn t_script_22_spawn_with_tags_kwarg() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    Python::attach(|py| {
        py.run(
            c"
import rython
entity_22 = rython.scene.spawn(tags=['player', 'cube'])
",
            None,
            None,
        )
        .expect("spawn with tags kwarg");

        let main = py.import("__main__").unwrap();
        let eid: u64 = main
            .getattr("entity_22")
            .unwrap()
            .getattr("id")
            .unwrap()
            .extract()
            .unwrap();
        let tags = scene
            .components
            .get::<TagComponent>(rython_ecs::EntityId(eid));
        assert!(tags.is_some(), "TagComponent should be present");
        let tags = tags.unwrap();
        assert!(
            tags.tags.contains(&"player".to_string()),
            "should have 'player' tag"
        );
        assert!(
            tags.tags.contains(&"cube".to_string()),
            "should have 'cube' tag"
        );
    });
}

// ─── T-SCRIPT-23: CameraPy set_position / set_look_at ────────────────────────

#[test]
fn t_script_23_camera_set_position_and_look_at() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    Python::attach(|py| {
        py.run(
            c"
import rython
rython.camera.set_position(0.0, 5.0, -10.0)
rython.camera.set_look_at(0.0, 0.0, 0.0)
cam_px = rython.camera.pos_x
cam_py = rython.camera.pos_y
cam_pz = rython.camera.pos_z
",
            None,
            None,
        )
        .expect("camera API");

        let main = py.import("__main__").unwrap();
        let px: f32 = main.getattr("cam_px").unwrap().extract().unwrap();
        let py_: f32 = main.getattr("cam_py").unwrap().extract().unwrap();
        let pz: f32 = main.getattr("cam_pz").unwrap().extract().unwrap();
        assert!((px - 0.0).abs() < 1e-5, "pos_x should be 0.0");
        assert!((py_ - 5.0).abs() < 1e-5, "pos_y should be 5.0");
        assert!((pz - (-10.0)).abs() < 1e-5, "pos_z should be -10.0");
    });
}

// ─── T-SCRIPT-24: scheduler.register_recurring fires per flush ────────────────

#[test]
fn t_script_24_scheduler_register_recurring() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);
    clear_recurring_callbacks();

    Python::attach(|py| {
        py.run(
            c"
import rython
tick_count_24 = 0
def on_tick_24():
    global tick_count_24
    tick_count_24 += 1
rython.scheduler.register_recurring(on_tick_24)
",
            None,
            None,
        )
        .expect("register_recurring");

        flush_recurring_callbacks(py);
        flush_recurring_callbacks(py);

        let main = py.import("__main__").unwrap();
        let count: i64 = main.getattr("tick_count_24").unwrap().extract().unwrap();
        assert_eq!(count, 2, "callback should fire once per flush");
    });

    clear_recurring_callbacks();
}

// ─── T-SCRIPT-25: renderer.draw_text enqueues a DrawCommand ──────────────────

#[test]
fn t_script_25_renderer_draw_text() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);
    // drain any leftover commands from prior tests
    let _ = drain_draw_commands();

    Python::attach(|py| {
        py.run(
            c"
import rython
rython.renderer.draw_text('Hello World', font_id='main', x=0.1, y=0.9, size=24)
",
            None,
            None,
        )
        .expect("draw_text");
    });

    let cmds = drain_draw_commands();
    assert_eq!(cmds.len(), 1, "one draw command expected");
    match &cmds[0] {
        DrawCommand::Text(dt) => {
            assert_eq!(dt.text, "Hello World");
            assert_eq!(dt.font_id, "main");
            assert_eq!(dt.size, 24);
        }
        other => panic!("expected DrawCommand::Text, got {:?}", other),
    }
}

// ─── T-SCRIPT-26: rython.time.elapsed returns set value ──────────────────────

#[test]
fn t_script_26_time_elapsed() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    #[allow(clippy::approx_constant)]
    const TEST_VALUE: f64 = 3.14;
    set_elapsed_secs(TEST_VALUE);

    Python::attach(|py| {
        py.run(
            c"import rython; elapsed_26 = rython.time.elapsed",
            None,
            None,
        )
        .expect("time.elapsed");

        let main = py.import("__main__").unwrap();
        let t: f64 = main.getattr("elapsed_26").unwrap().extract().unwrap();
        assert!(
            (t - TEST_VALUE).abs() < 1e-6,
            "elapsed should be {TEST_VALUE}, got {t}"
        );
    });
}

// ─── T-SCRIPT-27: engine.request_quit sets the quit flag ─────────────────────

#[test]
fn t_script_27_engine_request_quit() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);
    reset_quit_requested();

    assert!(!was_quit_requested(), "quit flag should start false");

    Python::attach(|py| {
        py.run(c"import rython; rython.engine.request_quit()", None, None)
            .expect("request_quit");
    });

    assert!(
        was_quit_requested(),
        "quit flag should be set after request_quit()"
    );
    reset_quit_requested(); // clean up for future tests
}

// ─── T-SCRIPT-28: scene.unsubscribe removes a handler ────────────────────────

#[test]
fn t_script_28_scene_unsubscribe() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    Python::attach(|py| {
        py.run(
            c"
import rython
count_28 = 0
def handler_28(**kwargs):
    global count_28
    count_28 += 1

hid = rython.scene.subscribe('test_28', handler_28)
rython.scene.emit('test_28')
rython.scene.unsubscribe('test_28', hid)
rython.scene.emit('test_28')
",
            None,
            None,
        )
        .expect("unsubscribe test");

        let main = py.import("__main__").unwrap();
        let count: i64 = main.getattr("count_28").unwrap().extract().unwrap();
        assert_eq!(
            count, 1,
            "handler should fire once before unsubscribe, not after"
        );
    });
}

// ─── T-SCRIPT-29: scheduler.on_timer fires at the scheduled time ─────────────

#[test]
fn t_script_29_scheduler_on_timer() {
    use rython_scripting::flush_timers;

    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    set_elapsed_secs(10.0);

    Python::attach(|py| {
        py.run(
            c"
import rython
timer_fired_29 = 0
def on_timer_29():
    global timer_fired_29
    timer_fired_29 += 1
# At elapsed=10.0, schedule to fire in 2 seconds (deadline = 12.0)
rython.scheduler.on_timer(2.0, on_timer_29)
",
            None,
            None,
        )
        .expect("on_timer setup");

        // elapsed=11.0 — should NOT fire yet
        set_elapsed_secs(11.0);
        flush_timers(py);
        let main = py.import("__main__").unwrap();
        let count: i64 = main.getattr("timer_fired_29").unwrap().extract().unwrap();
        assert_eq!(count, 0, "timer must not fire before deadline");

        // elapsed=12.0 — should fire exactly once
        set_elapsed_secs(12.0);
        flush_timers(py);
        let count: i64 = main.getattr("timer_fired_29").unwrap().extract().unwrap();
        assert_eq!(count, 1, "timer must fire at deadline");

        // another flush — must NOT re-fire
        flush_timers(py);
        let count: i64 = main.getattr("timer_fired_29").unwrap().extract().unwrap();
        assert_eq!(count, 1, "timer must fire exactly once");
    });
}

// ─── T-SCRIPT-30: scheduler.on_event fires the callback exactly once ─────────

#[test]
fn t_script_30_scheduler_on_event() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    Python::attach(|py| {
        py.run(
            c"
import rython
event_count_30 = 0
def on_my_event_30(**kwargs):
    global event_count_30
    event_count_30 += 1

rython.scheduler.on_event('test_event_30', on_my_event_30)
rython.scene.emit('test_event_30')   # should fire
rython.scene.emit('test_event_30')   # should NOT fire (one-shot)
",
            None,
            None,
        )
        .expect("on_event test");

        let main = py.import("__main__").unwrap();
        let count: i64 = main.getattr("event_count_30").unwrap().extract().unwrap();
        assert_eq!(count, 1, "on_event callback must fire exactly once");
    });
}

// ─── T-SCRIPT-31: PlayerController emits axis-change events over deadzone ─────

#[test]
fn t_script_31_axis_change_events() {
    use rython_input::{AxisBinding, InputMap, PlayerController};
    use rython_window::{KeyCode, RawInputEvent};

    let _lock = test_lock();

    let mut pc = PlayerController::new(0);
    let mut map = InputMap::new("test31");
    map.bind_axis(
        "horizontal",
        AxisBinding::KBAxis {
            negative: KeyCode::D,
            positive: KeyCode::A,
        },
    );
    pc.register_map(map);

    // Tick with no input — prime previous state, clear events
    pc.tick(&[]);
    pc.pending_events().lock().unwrap().clear();

    // Press A → axis 0.0 → 1.0, crosses deadzone → axis event expected
    pc.tick(&[RawInputEvent::KeyPressed(KeyCode::A)]);
    {
        let evs = pc.pending_events();
        let guard = evs.lock().unwrap();
        let axis_evs: Vec<_> = guard
            .iter()
            .filter(|e| e.action.starts_with("axis:"))
            .collect();
        assert!(
            !axis_evs.is_empty(),
            "axis change event expected when key pressed above deadzone"
        );
        let ev = axis_evs[0];
        assert_eq!(ev.action, "axis:horizontal");
        assert!((ev.value - 1.0).abs() < 1e-5, "axis value should be 1.0");
    }
    pc.pending_events().lock().unwrap().clear();

    // No new events (A still held, value unchanged)
    pc.tick(&[]);
    {
        let evs = pc.pending_events();
        let guard = evs.lock().unwrap();
        let axis_evs: Vec<_> = guard
            .iter()
            .filter(|e| e.action.starts_with("axis:"))
            .collect();
        assert!(axis_evs.is_empty(), "no axis event when value unchanged");
    }
    pc.pending_events().lock().unwrap().clear();

    // Release A → axis 1.0 → 0.0, crosses back below deadzone → axis event expected
    pc.tick(&[RawInputEvent::KeyReleased(KeyCode::A)]);
    {
        let evs = pc.pending_events();
        let guard = evs.lock().unwrap();
        let axis_evs: Vec<_> = guard
            .iter()
            .filter(|e| e.action.starts_with("axis:"))
            .collect();
        assert!(
            !axis_evs.is_empty(),
            "axis change event expected when key released"
        );
        let ev = axis_evs[0];
        assert_eq!(ev.action, "axis:horizontal");
        assert!(
            ev.value.abs() < 1e-5,
            "axis value should be 0.0 after release"
        );
    }
}

// ─── T-SCRIPT-32: Per-entity event subscription via Python API ───────────────

#[test]
fn t_script_32_per_entity_event_subscription() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    Python::attach(|py| {
        py.run(
            c"
import rython
received_32 = None
def on_collision_42(**kwargs):
    global received_32
    received_32 = kwargs.get('entity_a')

rython.scene.subscribe('collision:42', on_collision_42)
rython.scene.emit('collision:42', entity_a=42, entity_b=99)
",
            None,
            None,
        )
        .expect("per-entity event test");

        let main = py.import("__main__").unwrap();
        let eid: i64 = main.getattr("received_32").unwrap().extract().unwrap();
        assert_eq!(eid, 42, "per-entity event must deliver correct payload");
    });
}

// ─── T-SCRIPT-20: Entry Point Execution ──────────────────────────────────────

#[test]
fn t_script_20_entry_point_execution() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    let tmp = tempfile::tempdir().expect("tempdir");
    let main_py = tmp.path().join("ep_main_20.py");
    std::fs::write(
        &main_py,
        b"_init_called_20 = False\ndef init():\n    global _init_called_20\n    _init_called_20 = True\n",
    ).unwrap();

    let dir_str = tmp.path().to_str().unwrap().to_string();

    Python::attach(|py| {
        let sys = py.import("sys").unwrap();
        sys.getattr("path")
            .unwrap()
            .call_method1("insert", (0i32, &dir_str))
            .unwrap();

        rython_scripting::call_entry_point(py, "ep_main_20").expect("entry point");

        let module = py.import("ep_main_20").unwrap();
        let called: bool = module
            .getattr("_init_called_20")
            .unwrap()
            .extract()
            .unwrap();
        assert!(called, "init() should have been called");

        sys.getattr("path")
            .unwrap()
            .call_method1("remove", (&dir_str,))
            .ok();
        sys.getattr("modules").unwrap().del_item("ep_main_20").ok();
    });
}

// ─── T-SCRIPT-33: json_val_to_py — Null becomes Python None ──────────────────

#[test]
fn t_script_33_json_null_to_py_none() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    Python::attach(|py| {
        let val = serde_json::json!({ "n": null });
        let dict = rython_scripting::bridge::json_to_py_dict(py, &val).expect("json_to_py_dict");
        let n = dict.get_item("n").expect("no PyErr").expect("key 'n'");
        assert!(n.is_none(), "JSON null must become Python None");
    });
}

// ─── T-SCRIPT-34: json_val_to_py — Bool values become Python booleans ────────

#[test]
fn t_script_34_json_bool_to_py() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    Python::attach(|py| {
        let val = serde_json::json!({ "t": true, "f": false });
        let dict = rython_scripting::bridge::json_to_py_dict(py, &val).expect("json_to_py_dict");
        let t: bool = dict
            .get_item("t")
            .unwrap()
            .expect("key 't'")
            .extract()
            .unwrap();
        let f: bool = dict
            .get_item("f")
            .unwrap()
            .expect("key 'f'")
            .extract()
            .unwrap();
        assert!(t, "JSON true must become Python True");
        assert!(!f, "JSON false must become Python False");
    });
}

// ─── T-SCRIPT-35: json_val_to_py — Numbers: i64 stays int, float stays float ─

#[test]
fn t_script_35_json_number_to_py() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    Python::attach(|py| {
        #[allow(clippy::approx_constant)]
        const JSON_FLOAT: f64 = 3.14;
        let val = serde_json::json!({ "i": 42i64, "fl": JSON_FLOAT });
        let dict = rython_scripting::bridge::json_to_py_dict(py, &val).expect("json_to_py_dict");
        let i: i64 = dict
            .get_item("i")
            .unwrap()
            .expect("key 'i'")
            .extract()
            .unwrap();
        let fl: f64 = dict
            .get_item("fl")
            .unwrap()
            .expect("key 'fl'")
            .extract()
            .unwrap();
        assert_eq!(i, 42, "JSON integer must map to Python int");
        assert!(
            (fl - JSON_FLOAT).abs() < 1e-9,
            "JSON float must map to Python float"
        );
    });
}

// ─── T-SCRIPT-36: json_val_to_py — String value becomes Python str ────────────

#[test]
fn t_script_36_json_string_to_py() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    Python::attach(|py| {
        let val = serde_json::json!({ "s": "hello world" });
        let dict = rython_scripting::bridge::json_to_py_dict(py, &val).expect("json_to_py_dict");
        let s: String = dict
            .get_item("s")
            .unwrap()
            .expect("key 's'")
            .extract()
            .unwrap();
        assert_eq!(s, "hello world", "JSON string must map to Python str");
    });
}

// ─── T-SCRIPT-37: json_val_to_py — Array with mixed types becomes Python list ─

#[test]
fn t_script_37_json_array_to_py() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    Python::attach(|py| {
        let val = serde_json::json!({ "arr": [1, true, "x", null] });
        let dict = rython_scripting::bridge::json_to_py_dict(py, &val).expect("json_to_py_dict");
        let arr = dict.get_item("arr").unwrap().expect("key 'arr'");
        assert_eq!(arr.len().unwrap(), 4, "array must have 4 elements");
        let e0: i64 = arr.get_item(0).unwrap().extract().unwrap();
        assert_eq!(e0, 1);
        let e1: bool = arr.get_item(1).unwrap().extract().unwrap();
        assert!(e1);
        let e2: String = arr.get_item(2).unwrap().extract().unwrap();
        assert_eq!(e2, "x");
        assert!(
            arr.get_item(3).unwrap().is_none(),
            "null array element must be Python None"
        );
    });
}

// ─── T-SCRIPT-38: json_val_to_py — Nested Object becomes dict-in-dict ─────────

#[test]
fn t_script_38_json_nested_object_to_py() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    Python::attach(|py| {
        let val = serde_json::json!({ "inner": { "x": 7i64, "label": "deep" } });
        let dict = rython_scripting::bridge::json_to_py_dict(py, &val).expect("json_to_py_dict");
        let inner = dict.get_item("inner").unwrap().expect("key 'inner'");
        let x: i64 = inner.get_item("x").unwrap().extract().unwrap();
        let label: String = inner.get_item("label").unwrap().extract().unwrap();
        assert_eq!(x, 7, "nested object integer field");
        assert_eq!(label, "deep", "nested object string field");
    });
}

// ─── T-SCRIPT-39: Event — Multiple subscribers both fire on same emit ─────────

#[test]
fn t_script_39_multiple_event_subscribers() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    Python::attach(|py| {
        py.run(
            c"
import rython
count_a_39 = 0
count_b_39 = 0
def handler_a_39(**kwargs):
    global count_a_39
    count_a_39 += 1
def handler_b_39(**kwargs):
    global count_b_39
    count_b_39 += 1
rython.scene.subscribe('multi_39', handler_a_39)
rython.scene.subscribe('multi_39', handler_b_39)
rython.scene.emit('multi_39')
",
            None,
            None,
        )
        .expect("multiple subscribers");

        let main = py.import("__main__").unwrap();
        let a: i64 = main.getattr("count_a_39").unwrap().extract().unwrap();
        let b: i64 = main.getattr("count_b_39").unwrap().extract().unwrap();
        assert_eq!(a, 1, "handler_a must fire");
        assert_eq!(b, 1, "handler_b must fire");
    });
}

// ─── T-SCRIPT-40: Event — Emit with no subscribers is a no-op ────────────────

#[test]
fn t_script_40_emit_no_subscribers() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    Python::attach(|py| {
        // Must not panic
        py.run(
            c"import rython; rython.scene.emit('orphan_event_40')",
            None,
            None,
        )
        .expect("emit with no subscribers must not panic");
    });
}

// ─── T-SCRIPT-41: Event — subscribe returns unique IDs ───────────────────────

#[test]
fn t_script_41_subscribe_unique_ids() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    Python::attach(|py| {
        py.run(
            c"
import rython
def noop_41(**kwargs): pass
hid1 = rython.scene.subscribe('unique_id_41', noop_41)
hid2 = rython.scene.subscribe('unique_id_41', noop_41)
",
            None,
            None,
        )
        .expect("subscribe unique IDs");

        let main = py.import("__main__").unwrap();
        let id1: i64 = main.getattr("hid1").unwrap().extract().unwrap();
        let id2: i64 = main.getattr("hid2").unwrap().extract().unwrap();
        assert_ne!(id1, id2, "two subscriptions must return different IDs");
    });
}

// ─── T-SCRIPT-42: Timer — Partial expiry: only past-deadline timers fire ──────

#[test]
fn t_script_42_timer_partial_expiry() {
    use rython_scripting::flush_timers;

    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    set_elapsed_secs(5.0);

    Python::attach(|py| {
        py.run(
            c"
import rython
fired_early_42 = 0
fired_late_42 = 0
def on_early_42():
    global fired_early_42
    fired_early_42 += 1
def on_late_42():
    global fired_late_42
    fired_late_42 += 1
# fire_at = 5.0 + 1.0 = 6.0
rython.scheduler.on_timer(1.0, on_early_42)
# fire_at = 5.0 + 5.0 = 10.0
rython.scheduler.on_timer(5.0, on_late_42)
",
            None,
            None,
        )
        .expect("timer setup");

        // elapsed=7.0: early fires, late does not
        set_elapsed_secs(7.0);
        flush_timers(py);

        let main = py.import("__main__").unwrap();
        let early: i64 = main.getattr("fired_early_42").unwrap().extract().unwrap();
        let late: i64 = main.getattr("fired_late_42").unwrap().extract().unwrap();
        assert_eq!(
            early, 1,
            "early timer (fire_at=6.0) must fire at elapsed=7.0"
        );
        assert_eq!(
            late, 0,
            "late timer (fire_at=10.0) must NOT fire at elapsed=7.0"
        );

        // elapsed=11.0: late fires
        set_elapsed_secs(11.0);
        flush_timers(py);
        let late: i64 = main.getattr("fired_late_42").unwrap().extract().unwrap();
        assert_eq!(late, 1, "late timer must fire when elapsed >= fire_at");
    });
}

// ─── T-SCRIPT-43: Timer — Exact deadline equality triggers fire ───────────────

#[test]
fn t_script_43_timer_exact_deadline() {
    use rython_scripting::flush_timers;

    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    set_elapsed_secs(100.0);

    Python::attach(|py| {
        py.run(
            c"
import rython
exact_fired_43 = 0
def on_exact_43():
    global exact_fired_43
    exact_fired_43 += 1
# fire_at = 100.0 + 2.5 = 102.5
rython.scheduler.on_timer(2.5, on_exact_43)
",
            None,
            None,
        )
        .expect("timer setup");

        // elapsed == fire_at exactly: must fire (>= check)
        set_elapsed_secs(102.5);
        flush_timers(py);
        let main = py.import("__main__").unwrap();
        let fired: i64 = main.getattr("exact_fired_43").unwrap().extract().unwrap();
        assert_eq!(
            fired, 1,
            "timer must fire when elapsed == fire_at (>= boundary)"
        );
    });
}

// ─── T-SCRIPT-44: Timer — Exception in one callback doesn't block others ──────

#[test]
fn t_script_44_timer_exception_continues() {
    use rython_scripting::flush_timers;

    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    set_elapsed_secs(200.0);

    Python::attach(|py| {
        py.run(
            c"
import rython
safe_fired_44 = 0
def bad_timer_44():
    raise RuntimeError('timer exploded')
def safe_timer_44():
    global safe_fired_44
    safe_fired_44 += 1
rython.scheduler.on_timer(0.0, bad_timer_44)
rython.scheduler.on_timer(0.0, safe_timer_44)
",
            None,
            None,
        )
        .expect("timer setup");

        flush_timers(py); // must not panic despite bad_timer raising

        let main = py.import("__main__").unwrap();
        let safe: i64 = main.getattr("safe_fired_44").unwrap().extract().unwrap();
        assert_eq!(
            safe, 1,
            "safe timer must fire even after another timer raised an exception"
        );
    });
}

// ─── T-SCRIPT-45: Bridge — get_script_class returns None for unknown name ─────

#[test]
fn t_script_45_get_script_class_unknown() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    let result = rython_scripting::get_script_class("CompletelyUnknownClass_45");
    assert!(
        result.is_none(),
        "get_script_class for unregistered name must return None"
    );
}

// ─── T-SCRIPT-46: Bridge — drain_draw_commands second drain is empty ──────────

#[test]
fn t_script_46_drain_draw_commands_idempotent() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);
    let _ = drain_draw_commands(); // clear any residual from prior tests

    Python::attach(|py| {
        py.run(
            c"import rython; rython.renderer.draw_text('X', font_id='f', x=0.0, y=0.0, size=10)",
            None,
            None,
        )
        .expect("draw_text");
    });

    let first = drain_draw_commands();
    assert_eq!(
        first.len(),
        1,
        "first drain must contain the enqueued command"
    );

    let second = drain_draw_commands();
    assert!(
        second.is_empty(),
        "second drain must return empty after first consumed all"
    );
}

// ─── T-SCRIPT-47: Bridge — register_script_class overwrites duplicate name ────

#[test]
fn t_script_47_register_script_class_overwrite() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    Python::attach(|py| {
        py.run(
            c"
class ScriptV1_47:
    VERSION = 1
    def __init__(self, entity): self.entity = entity

class ScriptV2_47:
    VERSION = 2
    def __init__(self, entity): self.entity = entity
",
            None,
            None,
        )
        .unwrap();

        let main = py.import("__main__").unwrap();
        let v1 = main.getattr("ScriptV1_47").unwrap().unbind();
        let v2 = main.getattr("ScriptV2_47").unwrap().unbind();

        register_script_class("OverwriteTarget_47", v1);
        register_script_class("OverwriteTarget_47", v2);

        let cls = rython_scripting::get_script_class("OverwriteTarget_47")
            .expect("class must exist after registration");
        let version: i64 = cls.bind(py).getattr("VERSION").unwrap().extract().unwrap();
        assert_eq!(version, 2, "second registration must overwrite the first");
    });
}

// ─── T-SCRIPT-48: submit_parallel — handle is_done after flush ───────────────

#[test]
fn t_script_48_submit_parallel_done_same_tick() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    Python::attach(|py| {
        py.run(
            c"
import rython
called_48 = False
def task_48():
    global called_48
    called_48 = True
handle_48 = rython.scheduler.submit_parallel(task_48)
",
            None,
            None,
        )
        .expect("submit_parallel setup");

        let main = py.import("__main__").unwrap();

        // Before flush: still pending
        let pending_before: bool = main
            .getattr("handle_48")
            .unwrap()
            .getattr("is_pending")
            .unwrap()
            .extract()
            .unwrap();
        assert!(pending_before, "handle must be pending before flush");

        flush_python_par_tasks(py);

        // After flush: done and callable was invoked
        let done: bool = main
            .getattr("handle_48")
            .unwrap()
            .getattr("is_done")
            .unwrap()
            .extract()
            .unwrap();
        assert!(done, "handle must be done after flush_python_par_tasks");

        let called: bool = main.getattr("called_48").unwrap().extract().unwrap();
        assert!(called, "parallel task function must have been invoked");
    });
}

// ─── T-SCRIPT-49: submit_background — pending immediately, done next frame ───

#[test]
fn t_script_49_submit_background_async() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    // Submit the task; verify pending before spawn.
    Python::attach(|py| {
        py.run(
            c"
import rython
bg_called_49 = False
def bg_task_49():
    global bg_called_49
    bg_called_49 = True
handle_49 = rython.scheduler.submit_background(bg_task_49)
",
            None,
            None,
        )
        .expect("submit_background setup");

        let main = py.import("__main__").unwrap();
        let pending: bool = main
            .getattr("handle_49")
            .unwrap()
            .getattr("is_pending")
            .unwrap()
            .extract()
            .unwrap();
        assert!(pending, "background handle must start as pending");
    });

    // Spawn the task while the GIL is free so the rayon thread can acquire it.
    flush_python_bg_tasks();

    // Poll: each `Python::attach` releases the GIL on exit, giving the rayon
    // thread a window to acquire it and run the Python callback.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let done = Python::attach(|py| {
            flush_python_bg_completions(py);
            py.import("__main__")
                .unwrap()
                .getattr("handle_49")
                .unwrap()
                .getattr("is_done")
                .unwrap()
                .extract::<bool>()
                .unwrap()
        });
        if done {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "background task timed out"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    Python::attach(|py| {
        let called: bool = py
            .import("__main__")
            .unwrap()
            .getattr("bg_called_49")
            .unwrap()
            .extract()
            .unwrap();
        assert!(called, "background task function must have been called");
    });
}

// ─── T-SCRIPT-50: run_sequential — fires on next flush ───────────────────────

#[test]
fn t_script_50_run_sequential_next_tick() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    Python::attach(|py| {
        py.run(
            c"
import rython
seq_called_50 = False
def seq_task_50():
    global seq_called_50
    seq_called_50 = True
rython.scheduler.run_sequential(seq_task_50)
",
            None,
            None,
        )
        .expect("run_sequential setup");

        let main = py.import("__main__").unwrap();

        // Not called yet
        let called_before: bool = main.getattr("seq_called_50").unwrap().extract().unwrap();
        assert!(!called_before, "sequential task must not run until flush");

        // Run the sequential phase
        flush_python_seq_tasks(py);

        let called: bool = main.getattr("seq_called_50").unwrap().extract().unwrap();
        assert!(
            called,
            "sequential task must run after flush_python_seq_tasks"
        );

        // A second flush must not re-run it (one-shot)
        py.run(c"seq_called_50 = False", None, None).unwrap();
        flush_python_seq_tasks(py);
        let called_again: bool = main.getattr("seq_called_50").unwrap().extract().unwrap();
        assert!(!called_again, "sequential task must not run a second time");
    });
}

// ─── T-SCRIPT-51: JobHandle.on_complete fires after parallel task done ────────

#[test]
fn t_script_51_job_handle_on_complete_parallel() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    Python::attach(|py| {
        py.run(
            c"
import rython
complete_called_51 = False
def on_done_51():
    global complete_called_51
    complete_called_51 = True
def task_51(): pass
handle_51 = rython.scheduler.submit_parallel(task_51)
handle_51.on_complete(on_done_51)
",
            None,
            None,
        )
        .expect("on_complete setup");

        let main = py.import("__main__").unwrap();

        flush_python_par_tasks(py);

        let called: bool = main
            .getattr("complete_called_51")
            .unwrap()
            .extract()
            .unwrap();
        assert!(
            called,
            "on_complete callback must fire after parallel task completes"
        );
    });
}

// ─── T-SCRIPT-52: JobHandle.is_failed when task raises an exception ──────────

#[test]
fn t_script_52_job_handle_failed_on_exception() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    Python::attach(|py| {
        py.run(
            c"
import rython
def failing_task_52():
    raise ValueError('boom 52')
handle_52 = rython.scheduler.submit_parallel(failing_task_52)
",
            None,
            None,
        )
        .expect("failing task setup");

        let main = py.import("__main__").unwrap();

        flush_python_par_tasks(py);

        let failed: bool = main
            .getattr("handle_52")
            .unwrap()
            .getattr("is_failed")
            .unwrap()
            .extract()
            .unwrap();
        assert!(failed, "handle must be marked failed when task raises");

        let error: Option<String> = main
            .getattr("handle_52")
            .unwrap()
            .getattr("error")
            .unwrap()
            .extract()
            .unwrap();
        assert!(
            error.is_some(),
            "error message must be populated for failed task"
        );
        assert!(
            error.unwrap().contains("boom 52"),
            "error message must include original exception text"
        );
    });
}

// ─── T-SCRIPT-53: Recursive Event Emission ──────────────────────────────────

#[test]
fn t_script_53_recursive_event_emission() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    Python::attach(|py| {
        py.run(
            c"
import rython

depth_a_53 = 0
depth_b_53 = 0

def on_event_a_53(**kwargs):
    global depth_a_53
    depth_a_53 += 1
    # Handler for event_a emits event_b
    rython.scene.emit('event_b_53', src='a')

def on_event_b_53(**kwargs):
    global depth_b_53
    depth_b_53 += 1
    # Handler for event_b emits event_c (no handler, terminates chain)
    rython.scene.emit('event_c_53', src='b')

rython.scene.subscribe('event_a_53', on_event_a_53)
rython.scene.subscribe('event_b_53', on_event_b_53)

# Kick off the chain
rython.scene.emit('event_a_53', src='root')
",
            None,
            None,
        )
        .expect("recursive event emission must not crash");

        let main = py.import("__main__").unwrap();
        let a: i64 = main.getattr("depth_a_53").unwrap().extract().unwrap();
        let b: i64 = main.getattr("depth_b_53").unwrap().extract().unwrap();
        assert_eq!(a, 1, "event_a handler must fire once");
        assert_eq!(
            b, 1,
            "event_b handler must fire once (triggered by event_a handler)"
        );
    });
}

// ─── T-SCRIPT-54: Stale Entity Ref After Despawn ────────────────────────────

#[test]
fn t_script_54_stale_entity_ref_after_despawn() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    Python::attach(|py| {
        let code = "\
import rython
entity_54 = rython.scene.spawn(transform=rython.Transform(x=5.0, y=10.0, z=15.0))
eid_54 = entity_54.id
"
        .to_string();
        let cstr = CString::new(code).unwrap();
        py.run(cstr.as_c_str(), None, None).expect("spawn entity");

        let main = py.import("__main__").unwrap();
        let eid: u64 = main.getattr("eid_54").unwrap().extract().unwrap();

        // Despawn the entity through the ECS
        scene.queue_despawn(rython_ecs::EntityId(eid));
        scene.drain_commands();

        // Accessing transform on a despawned entity must not crash — should return defaults
        py.run(
            c"
tx_54 = entity_54.transform.x
ty_54 = entity_54.transform.y
tz_54 = entity_54.transform.z
",
            None,
            None,
        )
        .expect("stale entity transform access must not crash");

        let tx: f64 = main.getattr("tx_54").unwrap().extract().unwrap();
        let ty: f64 = main.getattr("ty_54").unwrap().extract().unwrap();
        let tz: f64 = main.getattr("tz_54").unwrap().extract().unwrap();

        // After despawn, the entity is gone — transform returns default (zero) values
        assert!(
            (tx - 0.0).abs() < 1e-5 && (ty - 0.0).abs() < 1e-5 && (tz - 0.0).abs() < 1e-5,
            "despawned entity transform should return defaults, got ({tx}, {ty}, {tz})"
        );
    });
}

// ─── T-SCRIPT-55: Throttle Hz=1 Boundary ────────────────────────────────────

#[test]
fn t_script_55_throttle_hz_one_boundary() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    Python::attach(|py| {
        py.run(
            c"
import rython

call_count_55 = 0

@rython.throttle(hz=1)
def throttled_fn_55():
    global call_count_55
    call_count_55 += 1
",
            None,
            None,
        )
        .expect("throttle setup");

        let main = py.import("__main__").unwrap();

        // Simulate 120 frames at 60 fps (2 seconds of game time)
        let dt = 1.0 / 60.0;
        for frame in 0..120 {
            let t = (frame as f64) * dt;
            set_elapsed_secs(t);
            py.run(c"throttled_fn_55()", None, None)
                .expect("throttle call");
        }

        let count: i64 = main.getattr("call_count_55").unwrap().extract().unwrap();
        // At hz=1, should fire approximately once per second → ~2 times over 2 seconds
        // Allow some tolerance for boundary effects (first call fires immediately at t=0)
        assert!(
            (2..=3).contains(&count),
            "throttle(hz=1) over 2 seconds should fire ~2-3 times, got {count}"
        );
        assert!(
            count < 120,
            "throttle must prevent firing every frame; got {count} (expected << 120)"
        );
    });
}

// ─── T-SCRIPT-56: Event Unsubscribe During Dispatch ─────────────────────────

#[test]
fn t_script_56_event_unsubscribe_during_dispatch() {
    let _lock = test_lock();
    let scene = Arc::new(Scene::new());
    setup(&scene);

    Python::attach(|py| {
        py.run(
            c"
import rython

fire_count_56 = 0
hid_56 = None

def self_removing_handler_56(**kwargs):
    global fire_count_56, hid_56
    fire_count_56 += 1
    # Unsubscribe ourselves during dispatch
    rython.scene.unsubscribe('self_remove_56', hid_56)

hid_56 = rython.scene.subscribe('self_remove_56', self_removing_handler_56)
",
            None,
            None,
        )
        .expect("event unsubscribe setup");

        let main = py.import("__main__").unwrap();

        // First emit — handler fires and unsubscribes itself
        py.run(c"rython.scene.emit('self_remove_56')", None, None)
            .expect("first emit must not crash");
        let count: i64 = main.getattr("fire_count_56").unwrap().extract().unwrap();
        assert_eq!(count, 1, "handler must fire once on first emit");

        // Second emit — handler should be gone
        py.run(c"rython.scene.emit('self_remove_56')", None, None)
            .expect("second emit must not crash");
        let count: i64 = main.getattr("fire_count_56").unwrap().extract().unwrap();
        assert_eq!(
            count, 1,
            "handler must not fire again after unsubscribing during dispatch"
        );
    });
}

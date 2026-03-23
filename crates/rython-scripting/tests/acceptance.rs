//! Acceptance tests for rython-scripting (T-SCRIPT-01 through T-SCRIPT-20).
//!
//! Hot-reload tests (T-SCRIPT-14..17) use #[ignore] because they depend on
//! file watcher timing and the `dev-reload` feature.

use std::ffi::CString;
use std::sync::{Arc, Mutex};

use pyo3::prelude::*;
use rython_ecs::Scene;
use rython_ecs::component::{MeshComponent, TagComponent, TransformComponent};
use rython_renderer::DrawCommand;
use rython_scripting::{
    clear_recurring_callbacks, drain_draw_commands, ensure_rython_module, flush_recurring_callbacks,
    gil_dispatch_count, register_script_class, reset_gil_dispatch_count, reset_quit_requested,
    set_active_scene, set_elapsed_secs, was_quit_requested, ScriptComponent, ScriptSystem,
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
    let handle = scene.queue_spawn(vec![
        (
            TypeId::of::<ScriptComponent>(),
            Box::new(ScriptComponent { class_name: class_name.to_string() })
                as Box<dyn rython_ecs::component::Component>,
        ),
    ]);
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
            Box::new(ScriptComponent { class_name: class_name.to_string() })
                as Box<dyn rython_ecs::component::Component>,
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
        py.run(c"import rython", None, None).expect("import rython failed");

        let rython = py.import("rython").expect("rython");
        for attr in ["scene", "renderer", "physics", "audio", "input", "ui",
                     "resources", "scheduler", "modules", "camera"] {
            let val = rython.getattr(attr).unwrap_or_else(|_| panic!("rython.{attr} missing"));
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
            None, None,
        ).unwrap();

        let main = py.import("__main__").unwrap();
        register_script_class("TestScript02", main.getattr("TestScript02").unwrap().unbind());

        let entity_id = spawn_with_script(&scene, "TestScript02");
        sys.flush(py);

        let inst = sys.get_instance(entity_id).expect("instance should exist");
        let py_id: u64 = inst.bind(py)
            .getattr("entity").unwrap()
            .getattr("id").unwrap()
            .extract().unwrap();
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
            None, None,
        ).unwrap();

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
            None, None,
        ).unwrap();

        let main = py.import("__main__").unwrap();
        register_script_class("DespawnScript", main.getattr("DespawnScript").unwrap().unbind());

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
            None, None,
        ).unwrap();

        let main = py.import("__main__").unwrap();
        register_script_class("CollisionScript", main.getattr("CollisionScript").unwrap().unbind());

        let entity_a = spawn_with_script(&scene, "CollisionScript");
        let entity_b = spawn_empty(&scene);
        sys.flush(py);

        sys.queue_collision(entity_a, entity_b, [1.0, 0.0, 0.0]);
        sys.flush(py);

        let called: bool = main.getattr("collision_called_05").unwrap().extract().unwrap();
        assert!(called, "on_collision should have been called");
        let other_id: u64 = main.getattr("collision_other_id_05").unwrap().extract().unwrap();
        assert_eq!(other_id, entity_b.0);
        let nx: f32 = main.getattr("collision_normal_x_05").unwrap().extract().unwrap();
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
            None, None,
        ).unwrap();

        let main = py.import("__main__").unwrap();
        register_script_class("TriggerScript", main.getattr("TriggerScript").unwrap().unbind());

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
            None, None,
        ).unwrap();

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
            None, None,
        ).unwrap();

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
    let handle = scene.queue_spawn(vec![
        (TypeId::of::<TransformComponent>(), Box::new(TransformComponent::default()) as Box<dyn rython_ecs::component::Component>),
    ]);
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

    let x = scene.components
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
            None, None,
        ).expect("Vec3 arithmetic failed");
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
            None, None,
        ).unwrap();

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
        assert!(combined.contains("ValueError"), "log must contain 'ValueError'");
        assert!(combined.contains("test error"), "log must contain 'test error'");
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
            None, None,
        ).unwrap();

        let main = py.import("__main__").unwrap();
        register_script_class("MultiErrorScript", main.getattr("MultiErrorScript").unwrap().unbind());

        let dummy = spawn_empty(&scene);
        let entities: Vec<_> = (0..3).map(|_| spawn_with_script(&scene, "MultiErrorScript")).collect();
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
fn t_script_14_hot_reload_file_change_detection() {}

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
            None, None,
        ).unwrap();

        let main = py.import("__main__").unwrap();
        register_script_class("HotScriptV1", main.getattr("HotScriptV1").unwrap().unbind());

        let entity = spawn_with_script(&scene, "HotScriptV1");
        let dummy = spawn_empty(&scene);
        sys.flush(py);

        let class_v2 = main.getattr("HotScriptV2").unwrap().unbind();
        sys.reload_entity_script(py, entity, class_v2).expect("reload");

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
fn t_script_16_hot_reload_syntax_error_resilience() {}

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
            None, None,
        ).unwrap();

        let main = py.import("__main__").unwrap();
        register_script_class("PersistScript17", main.getattr("PersistScript17").unwrap().unbind());

        let entity = spawn_with_transform_and_script(
            &scene,
            "PersistScript17",
            TransformComponent { x: 10.0, y: 20.0, z: 30.0, ..Default::default() },
        );
        sys.flush(py);

        let class2 = main.getattr("PersistScript17").unwrap().unbind();
        sys.reload_entity_script(py, entity, class2).expect("reload");

        let x = scene.components.get_ref::<TransformComponent, _, _>(entity, |t| t.x).unwrap();
        let y = scene.components.get_ref::<TransformComponent, _, _>(entity, |t| t.y).unwrap();
        let z = scene.components.get_ref::<TransformComponent, _, _>(entity, |t| t.z).unwrap();
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
        assert!(!rython.getattr("scene").unwrap().is_none(), "rython.scene accessible");

        // Clean up
        let sys = py.import("sys").unwrap();
        sys.getattr("path").unwrap().call_method1("remove", (&bundle_str,)).ok();
        sys.getattr("modules").unwrap().del_item("bundle_test_18").ok();
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
            None, None,
        ).unwrap();

        let main = py.import("__main__").unwrap();
        register_script_class("BatchScript19", main.getattr("BatchScript19").unwrap().unbind());

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
        assert!(count <= 2, "GIL acquired at most 2 times per frame, got {count}");
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
            None, None,
        ).expect("spawn with mesh kwarg");

        let main = py.import("__main__").unwrap();
        let eid: u64 = main.getattr("entity_21").unwrap().getattr("id").unwrap().extract().unwrap();
        let mesh = scene.components.get::<MeshComponent>(rython_ecs::EntityId(eid));
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
            None, None,
        ).expect("spawn with tags kwarg");

        let main = py.import("__main__").unwrap();
        let eid: u64 = main.getattr("entity_22").unwrap().getattr("id").unwrap().extract().unwrap();
        let tags = scene.components.get::<TagComponent>(rython_ecs::EntityId(eid));
        assert!(tags.is_some(), "TagComponent should be present");
        let tags = tags.unwrap();
        assert!(tags.tags.contains(&"player".to_string()), "should have 'player' tag");
        assert!(tags.tags.contains(&"cube".to_string()), "should have 'cube' tag");
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
            None, None,
        ).expect("camera API");

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
            None, None,
        ).expect("register_recurring");

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
            None, None,
        ).expect("draw_text");
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

    set_elapsed_secs(3.14);

    Python::attach(|py| {
        py.run(c"import rython; elapsed_26 = rython.time.elapsed", None, None)
            .expect("time.elapsed");

        let main = py.import("__main__").unwrap();
        let t: f64 = main.getattr("elapsed_26").unwrap().extract().unwrap();
        assert!((t - 3.14).abs() < 1e-6, "elapsed should be 3.14, got {t}");
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

    assert!(was_quit_requested(), "quit flag should be set after request_quit()");
    reset_quit_requested(); // clean up for future tests
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
        sys.getattr("path").unwrap().call_method1("insert", (0i32, &dir_str)).unwrap();

        rython_scripting::call_entry_point(py, "ep_main_20").expect("entry point");

        let module = py.import("ep_main_20").unwrap();
        let called: bool = module.getattr("_init_called_20").unwrap().extract().unwrap();
        assert!(called, "init() should have been called");

        sys.getattr("path").unwrap().call_method1("remove", (&dir_str,)).ok();
        sys.getattr("modules").unwrap().del_item("ep_main_20").ok();
    });
}

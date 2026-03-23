//! PyO3 bridge — module root.  All global state lives here; per-class
//! implementations are split across the sibling sub-modules.

pub mod audio;
pub mod camera;
pub mod engine;
pub mod entity;
pub mod input;
pub mod physics;
pub mod renderer;
pub mod resources;
pub mod scene;
pub mod scheduler;
pub mod time;
pub mod types;
pub mod ui;

// ── Re-exports so lib.rs stays unchanged ──────────────────────────────────────
pub use audio::set_active_audio;
pub use camera::CameraPy;
pub use entity::EntityPy;
pub use input::set_active_input;
pub use resources::set_active_resources;
pub use types::{TransformPy, Vec3Py};
pub use ui::{drain_ui_draw_commands, set_active_ui};
// call_entry_point, clear_recurring_callbacks, drain_draw_commands,
// ensure_rython_module, flush_recurring_callbacks, get_script_class,
// json_to_py_dict, load_bundle, register_script_class, reset_quit_requested,
// set_active_scene, set_elapsed_secs, was_quit_requested  — all defined below.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule, PyString};
use rython_ecs::Scene;
use rython_renderer::command::DrawCommand;

// ─── Active scene ─────────────────────────────────────────────────────────────

static ACTIVE_SCENE: OnceLock<Arc<Mutex<Option<Arc<Scene>>>>> = OnceLock::new();

pub(crate) fn scene_store() -> &'static Arc<Mutex<Option<Arc<Scene>>>> {
    ACTIVE_SCENE.get_or_init(|| Arc::new(Mutex::new(None)))
}

pub fn set_active_scene(scene: Arc<Scene>) {
    *scene_store().lock() = Some(scene);
}

// ─── Script class registry ────────────────────────────────────────────────────

static SCRIPT_CLASSES: OnceLock<Arc<Mutex<HashMap<String, Py<PyAny>>>>> = OnceLock::new();

pub(crate) fn class_registry() -> &'static Arc<Mutex<HashMap<String, Py<PyAny>>>> {
    SCRIPT_CLASSES.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
}

pub fn register_script_class(name: impl Into<String>, class: Py<PyAny>) {
    class_registry().lock().insert(name.into(), class);
}

pub fn get_script_class(name: &str) -> Option<Py<PyAny>> {
    Python::attach(|py| class_registry().lock().get(name).map(|p| p.clone_ref(py)))
}

// ─── Draw command buffer ──────────────────────────────────────────────────────

static DRAW_COMMANDS: OnceLock<Arc<Mutex<Vec<DrawCommand>>>> = OnceLock::new();

pub(crate) fn draw_commands_store() -> &'static Arc<Mutex<Vec<DrawCommand>>> {
    DRAW_COMMANDS.get_or_init(|| Arc::new(Mutex::new(Vec::new())))
}

pub fn drain_draw_commands() -> Vec<DrawCommand> {
    std::mem::take(&mut draw_commands_store().lock())
}

// ─── Recurring callbacks ──────────────────────────────────────────────────────

static RECURRING_CALLBACKS: OnceLock<Arc<Mutex<Vec<Py<PyAny>>>>> = OnceLock::new();

pub(crate) fn recurring_callbacks_store() -> &'static Arc<Mutex<Vec<Py<PyAny>>>> {
    RECURRING_CALLBACKS.get_or_init(|| Arc::new(Mutex::new(Vec::new())))
}

pub fn flush_recurring_callbacks(py: Python<'_>) {
    let callbacks: Vec<Py<PyAny>> = {
        let guard = recurring_callbacks_store().lock();
        guard.iter().map(|cb| cb.clone_ref(py)).collect()
    };
    for cb in &callbacks {
        if let Err(e) = cb.bind(py).call0() {
            e.print_and_set_sys_last_vars(py);
        }
    }
}

pub fn clear_recurring_callbacks() {
    recurring_callbacks_store().lock().clear();
}

// ─── Engine time ──────────────────────────────────────────────────────────────

static ELAPSED_SECS_BITS: AtomicU64 = AtomicU64::new(0);

pub fn set_elapsed_secs(secs: f64) {
    ELAPSED_SECS_BITS.store(secs.to_bits(), Ordering::Relaxed);
}

pub(crate) fn get_elapsed_secs() -> f64 {
    f64::from_bits(ELAPSED_SECS_BITS.load(Ordering::Relaxed))
}

// ─── Quit flag ────────────────────────────────────────────────────────────────

pub(crate) static QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn was_quit_requested() -> bool {
    QUIT_REQUESTED.load(Ordering::Relaxed)
}

pub fn reset_quit_requested() {
    QUIT_REQUESTED.store(false, Ordering::Relaxed);
}

// ─── Stub namespace ───────────────────────────────────────────────────────────

#[pyclass(name = "SubModule")]
pub struct SubModulePy {
    name: String,
}

#[pymethods]
impl SubModulePy {
    fn __repr__(&self) -> String {
        format!("rython.{}", self.name)
    }

    fn __getattr__(&self, _attr: &str) -> PyResult<Py<PyAny>> {
        Err(PyErr::new::<PyValueError, _>(format!("rython.{} is a stub", self.name)))
    }
}

// ─── JSON helpers (pub(crate) so scene.rs can use them) ───────────────────────

pub(crate) fn py_value_to_json(val: &Bound<'_, PyAny>) -> PyResult<serde_json::Value> {
    if val.is_none() {
        Ok(serde_json::Value::Null)
    } else if let Ok(b) = val.extract::<bool>() {
        Ok(serde_json::Value::Bool(b))
    } else if let Ok(i) = val.extract::<i64>() {
        Ok(serde_json::Value::Number(i.into()))
    } else if let Ok(f) = val.extract::<f64>() {
        Ok(serde_json::json!(f))
    } else if let Ok(s) = val.extract::<String>() {
        Ok(serde_json::Value::String(s))
    } else {
        Ok(serde_json::Value::Null)
    }
}

pub fn json_to_py_dict<'py>(
    py: Python<'py>,
    val: &serde_json::Value,
) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    if let serde_json::Value::Object(map) = val {
        for (k, v) in map {
            let py_val = json_val_to_py(py, v)?;
            dict.set_item(PyString::new(py, k), py_val)?;
        }
    }
    Ok(dict)
}

fn json_val_to_py<'py>(py: Python<'py>, val: &serde_json::Value) -> PyResult<Bound<'py, PyAny>> {
    use pyo3::types::{PyBool, PyNone};
    Ok(match val {
        serde_json::Value::Null => PyNone::get(py).as_any().to_owned(),
        serde_json::Value::Bool(b) => PyBool::new(py, *b).as_any().to_owned(),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.into_pyobject(py)?.into_any()
            } else {
                n.as_f64().unwrap_or(0.0).into_pyobject(py)?.into_any()
            }
        }
        serde_json::Value::String(s) => PyString::new(py, s).into_any(),
        _ => PyNone::get(py).as_any().to_owned(),
    })
}

// ─── Module initialisation ────────────────────────────────────────────────────

/// Create or update the `rython` module in `sys.modules`.
///
/// Safe to call multiple times — only registers the module once.
pub fn ensure_rython_module(py: Python<'_>, scene: Arc<Scene>) -> PyResult<()> {
    set_active_scene(scene);

    let sys = py.import("sys")?;
    let sys_modules = sys.getattr("modules")?;

    if sys_modules.get_item("rython").is_ok() {
        return Ok(());
    }

    let rython = PyModule::new(py, "rython")?;
    rython.add_class::<types::Vec3Py>()?;
    rython.add_class::<types::TransformPy>()?;
    rython.add_class::<entity::EntityPy>()?;
    rython.add_class::<resources::AssetHandlePy>()?;

    let scene_bridge = Py::new(py, scene::SceneBridge {})?;
    rython.add("scene", scene_bridge)?;

    let cam = Py::new(py, camera::CameraPy::new())?;
    rython.add("camera", cam)?;

    let sched = Py::new(py, scheduler::SchedulerBridge {})?;
    rython.add("scheduler", sched)?;

    let rend = Py::new(py, renderer::RendererBridge {})?;
    rython.add("renderer", rend)?;

    let t = Py::new(py, time::TimeBridge {})?;
    rython.add("time", t)?;

    let eng = Py::new(py, engine::EngineBridge {})?;
    rython.add("engine", eng)?;

    let inp = Py::new(py, input::InputBridge {})?;
    rython.add("input", inp)?;

    let aud = Py::new(py, audio::AudioBridge {})?;
    rython.add("audio", aud)?;

    let ui_bridge = Py::new(py, ui::UIBridge {})?;
    rython.add("ui", ui_bridge)?;

    for name in &["physics", "resources", "modules"] {
        let stub = Py::new(py, SubModulePy { name: name.to_string() })?;
        rython.add(*name, stub)?;
    }

    sys_modules.set_item("rython", &rython)?;
    Ok(())
}

/// Load scripts from a zip bundle into `sys.path` (release mode).
pub fn load_bundle(py: Python<'_>, bundle_path: &str) -> PyResult<()> {
    let sys = py.import("sys")?;
    let path = sys.getattr("path")?;
    path.call_method1("insert", (0i32, bundle_path))?;
    Ok(())
}

/// Import the entry point module and call its `init()` function if present.
pub fn call_entry_point(py: Python<'_>, module_name: &str) -> PyResult<()> {
    let module = py.import(PyString::new(py, module_name))?;
    if module.hasattr("init")? {
        module.call_method0("init")?;
    }
    Ok(())
}

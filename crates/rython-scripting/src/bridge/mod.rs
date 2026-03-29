//! PyO3 bridge — module root.  All global state lives here; per-class
//! implementations are split across the sibling sub-modules.

pub mod audio;
pub mod camera;
pub mod engine;
pub mod entity;
pub mod input;
pub mod job_handle;
pub mod physics;
pub mod renderer;
pub mod resources;
pub mod scene;
pub mod scheduler;
pub mod task;
pub mod time;
pub mod types;
pub mod ui;

// ── Re-exports so lib.rs stays unchanged ──────────────────────────────────────
pub use audio::set_active_audio;
pub use camera::CameraPy;
pub use entity::EntityPy;
pub use input::set_active_input;
pub use job_handle::JobHandlePy;
pub use physics::set_active_physics;
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

use crossbeam_channel::{Receiver, Sender};

use parking_lot::Mutex;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule, PyString};
use rython_ecs::Scene;
use rython_renderer::command::DrawCommand;
use rython_renderer::SceneSettings;

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

/// Look up a registered script class by name.
///
/// Public API: acquires GIL via `Python::attach` for callers that don't hold it.
pub fn get_script_class(name: &str) -> Option<Py<PyAny>> {
    Python::attach(|py| get_script_class_with_gil(py, name))
}

/// Internal variant for code that already holds the GIL — avoids the
/// redundant `Python::attach` call.
pub(crate) fn get_script_class_with_gil(py: Python<'_>, name: &str) -> Option<Py<PyAny>> {
    class_registry().lock().get(name).map(|p| p.clone_ref(py))
}

// ─── Draw command buffer ──────────────────────────────────────────────────────

static DRAW_COMMANDS: OnceLock<Arc<Mutex<Vec<DrawCommand>>>> = OnceLock::new();

pub(crate) fn draw_commands_store() -> &'static Arc<Mutex<Vec<DrawCommand>>> {
    DRAW_COMMANDS.get_or_init(|| Arc::new(Mutex::new(Vec::new())))
}

pub fn drain_draw_commands() -> Vec<DrawCommand> {
    std::mem::take(&mut draw_commands_store().lock())
}

// ─── Scene settings (clear color, light direction/color/intensity) ────────────

static SCENE_SETTINGS: OnceLock<Arc<Mutex<SceneSettings>>> = OnceLock::new();

pub(crate) fn scene_settings_store() -> &'static Arc<Mutex<SceneSettings>> {
    SCENE_SETTINGS.get_or_init(|| Arc::new(Mutex::new(SceneSettings::default())))
}

/// Snapshot the current scene settings for the renderer to consume.
pub fn get_scene_settings() -> SceneSettings {
    scene_settings_store().lock().clone()
}

// ─── Recurring callbacks ──────────────────────────────────────────────────────

static RECURRING_CALLBACKS: OnceLock<Arc<Mutex<Vec<Py<PyAny>>>>> = OnceLock::new();

pub(crate) fn recurring_callbacks_store() -> &'static Arc<Mutex<Vec<Py<PyAny>>>> {
    RECURRING_CALLBACKS.get_or_init(|| Arc::new(Mutex::new(Vec::new())))
}

pub fn flush_recurring_callbacks(py: Python<'_>) {
    // Iterate with the lock held — callbacks do not re-enter this lock,
    // so this is safe and avoids clone_ref on every callback every frame.
    let guard = recurring_callbacks_store().lock();
    for cb in guard.iter() {
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

// ─── Timer system ─────────────────────────────────────────────────────────────

pub(crate) struct PendingTimer {
    pub fire_at: f64,
    pub callback: Py<PyAny>,
}

static PENDING_TIMERS: OnceLock<Arc<Mutex<Vec<PendingTimer>>>> = OnceLock::new();

pub(crate) fn timer_store() -> &'static Arc<Mutex<Vec<PendingTimer>>> {
    PENDING_TIMERS.get_or_init(|| Arc::new(Mutex::new(Vec::new())))
}

/// Fire any timers whose deadline has passed. Call once per frame after
/// updating elapsed time.
pub fn flush_timers(py: Python<'_>) {
    let now = get_elapsed_secs();
    // Take ownership of the entire vec, partition into expired/remaining,
    // and put the remaining ones back — avoids clone_ref on expired callbacks.
    let all_timers: Vec<PendingTimer> = std::mem::take(&mut timer_store().lock());
    let mut remaining = Vec::new();
    let mut expired = Vec::new();
    for t in all_timers {
        if now >= t.fire_at {
            expired.push(t.callback);
        } else {
            remaining.push(t);
        }
    }
    if !remaining.is_empty() {
        *timer_store().lock() = remaining;
    }
    for cb in &expired {
        if let Err(e) = cb.bind(py).call0() {
            e.print_and_set_sys_last_vars(py);
        }
    }
}

// ─── Python job queues ────────────────────────────────────────────────────────
//
// Three static queues buffer Python-submitted tasks between the script phase
// (where tasks are submitted) and the engine tick phases (where they run).

use task::{PythonBgRequest, PythonParRequest};

static PYTHON_BG_QUEUE: OnceLock<Arc<Mutex<Vec<PythonBgRequest>>>> = OnceLock::new();
static PYTHON_PAR_QUEUE: OnceLock<Arc<Mutex<Vec<PythonParRequest>>>> = OnceLock::new();
static PYTHON_SEQ_QUEUE: OnceLock<Arc<Mutex<Vec<Py<PyAny>>>>> = OnceLock::new();

fn bg_queue() -> &'static Arc<Mutex<Vec<PythonBgRequest>>> {
    PYTHON_BG_QUEUE.get_or_init(|| Arc::new(Mutex::new(Vec::new())))
}

fn par_queue() -> &'static Arc<Mutex<Vec<PythonParRequest>>> {
    PYTHON_PAR_QUEUE.get_or_init(|| Arc::new(Mutex::new(Vec::new())))
}

fn seq_queue() -> &'static Arc<Mutex<Vec<Py<PyAny>>>> {
    PYTHON_SEQ_QUEUE.get_or_init(|| Arc::new(Mutex::new(Vec::new())))
}

// ─── Background completion channel ────────────────────────────────────────────

struct BgCompletionChannel {
    tx: Sender<(Arc<job_handle::JobState>, Result<(), String>)>,
    rx: Receiver<(Arc<job_handle::JobState>, Result<(), String>)>,
}

// SAFETY: Sender/Receiver<T> are Send + Sync when T: Send.
// Arc<JobState> is Send + Sync; String is Send.
unsafe impl Sync for BgCompletionChannel {}

static BG_CHANNEL: OnceLock<BgCompletionChannel> = OnceLock::new();

fn bg_channel() -> &'static BgCompletionChannel {
    BG_CHANNEL.get_or_init(|| {
        let (tx, rx) = crossbeam_channel::unbounded();
        BgCompletionChannel { tx, rx }
    })
}

pub(crate) fn push_bg_request(req: PythonBgRequest) {
    bg_queue().lock().push(req);
}

pub(crate) fn push_par_request(req: PythonParRequest) {
    par_queue().lock().push(req);
}

pub(crate) fn push_seq_task(callback: Py<PyAny>) {
    seq_queue().lock().push(callback);
}

/// Spawn all queued background tasks onto the global rayon thread pool.
/// Each task acquires the GIL independently when it runs.  Call once per tick,
/// before `flush_python_bg_completions`.
pub fn flush_python_bg_tasks() {
    let requests: Vec<PythonBgRequest> = std::mem::take(&mut bg_queue().lock());
    let tx = bg_channel().tx.clone();
    for req in requests {
        let state = Arc::clone(&req.state);
        let callback = req.callback;
        let tx = tx.clone();
        rayon::spawn(move || {
            let result = Python::attach(|py| {
                callback.bind(py).call0().map(|_| ()).map_err(|e| e.to_string())
            });
            let _ = tx.send((state, result));
        });
    }
}

/// Run all queued parallel tasks on the main thread (GIL held).
/// Tasks complete within the current tick and their `JobHandle` is immediately
/// marked done.
pub fn flush_python_par_tasks(py: Python<'_>) {
    let requests: Vec<PythonParRequest> = std::mem::take(&mut par_queue().lock());
    for req in requests {
        let result = req.callback.bind(py).call0();
        match result {
            Ok(_) => req.state.complete(py),
            Err(e) => req.state.fail(py, e.to_string()),
        }
    }
}

/// Run all sequential tasks queued during the previous tick.
pub fn flush_python_seq_tasks(py: Python<'_>) {
    let tasks: Vec<Py<PyAny>> = std::mem::take(&mut seq_queue().lock());
    for task in tasks {
        if let Err(e) = task.bind(py).call0() {
            e.print_and_set_sys_last_vars(py);
        }
    }
}

/// Process completed background tasks: update `JobHandle` state and fire
/// `on_complete` callbacks.  Call once per tick.
pub fn flush_python_bg_completions(py: Python<'_>) {
    let rx = &bg_channel().rx;
    while let Ok((state, result)) = rx.try_recv() {
        match result {
            Ok(()) => state.complete(py),
            Err(e) => state.fail(py, e),
        }
    }
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
    use pyo3::types::{PyBool, PyDict, PyList, PyNone};
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
        serde_json::Value::Array(arr) => {
            let elements: Vec<Bound<'py, PyAny>> = arr
                .iter()
                .map(|v| json_val_to_py(py, v))
                .collect::<PyResult<_>>()?;
            PyList::new(py, elements)?.into_any()
        }
        serde_json::Value::Object(obj) => {
            let dict = PyDict::new(py);
            for (k, v) in obj {
                dict.set_item(PyString::new(py, k), json_val_to_py(py, v)?)?;
            }
            dict.into_any()
        }
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
    rython.add_class::<job_handle::JobHandlePy>()?;

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

    let phys = Py::new(py, physics::PhysicsBridge {})?;
    rython.add("physics", phys)?;

    let res = Py::new(py, resources::ResourcesBridge {})?;
    rython.add("resources", res)?;

    let stub = Py::new(py, SubModulePy { name: "modules".to_string() })?;
    rython.add("modules", stub)?;

    sys_modules.set_item("rython", &rython)?;

    // Attach rython.throttle — a decorator that rate-limits a function to at
    // most N calls per second using the engine's own elapsed-time clock.
    //
    // Our PyModule has no __path__, so Python cannot resolve rython._decorators
    // as a subpackage after we replace sys.modules['rython'].  We inline the
    // implementation here so it works regardless of sys.path contents.
    // The logic mirrors rython/_decorators.py exactly; keep both in sync.
    py.run(
        c"\
import sys as _sys

def _throttle(hz):
    import functools as _ft
    if hz <= 0:
        raise ValueError(f'throttle hz must be > 0, got {hz!r}')
    _interval = 1.0 / hz
    def _decorator(fn):
        import rython as _r
        _last = [-_interval]
        @_ft.wraps(fn)
        def _wrapper(*args, **kwargs):
            _now = _r.time.elapsed
            if _now < _last[0]:
                _last[0] = _now - _interval
            if _now - _last[0] >= _interval:
                _last[0] = _now
                return fn(*args, **kwargs)
            return None
        return _wrapper
    return _decorator

_sys.modules['rython'].throttle = _throttle
del _sys, _throttle
",
        None,
        None,
    )?;

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

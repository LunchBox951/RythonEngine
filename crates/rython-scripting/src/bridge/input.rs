use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;
use pyo3::prelude::*;
use rython_input::{ActionValue, InputSnapshot};

use crate::bridge::input_map::{
    self as im, ActionValuePy, InputMap,
};

static INPUT_SNAPSHOT: OnceLock<Arc<Mutex<InputSnapshot>>> = OnceLock::new();

fn input_store() -> &'static Arc<Mutex<InputSnapshot>> {
    INPUT_SNAPSHOT.get_or_init(|| Arc::new(Mutex::new(InputSnapshot::new())))
}

/// Set the per-frame input snapshot (call from game loop after PlayerController::tick).
pub fn set_active_input(snapshot: InputSnapshot) {
    *input_store().lock() = snapshot;
}

// ─── Input bridge ─────────────────────────────────────────────────────────────

#[pyclass(name = "InputBridge")]
pub struct InputBridge {}

#[pymethods]
impl InputBridge {
    fn axis(&self, action: &str) -> f64 {
        input_store().lock().axis(action) as f64
    }

    fn axis2(&self, action: &str) -> (f32, f32) {
        let [x, y] = input_store().lock().axis2(action);
        (x, y)
    }

    fn axis3(&self, action: &str) -> (f32, f32, f32) {
        let [x, y, z] = input_store().lock().axis3(action);
        (x, y, z)
    }

    /// Returns the typed `ActionValue` for the action, or `None` if unbound.
    fn value(&self, py: Python<'_>, action: &str) -> PyResult<Option<Py<ActionValuePy>>> {
        let v: Option<ActionValue> = input_store().lock().value(action);
        match v {
            Some(val) => Ok(Some(Py::new(py, ActionValuePy::new(val))?)),
            None => Ok(None),
        }
    }

    fn pressed(&self, action: &str) -> bool {
        input_store().lock().pressed(action)
    }

    fn held(&self, action: &str) -> bool {
        input_store().lock().held(action)
    }

    fn released(&self, action: &str) -> bool {
        input_store().lock().released(action)
    }

    // ─── InputMap lifecycle ────────────────────────────────────────────────

    fn push_map(&self, py: Python<'_>, map: Py<InputMap>) -> PyResult<()> {
        im::push_map(py, map)
    }

    fn pop_map(&self, py: Python<'_>, id: &str) -> PyResult<()> {
        im::pop_map(py, id)
    }

    fn clear_maps(&self, py: Python<'_>) -> PyResult<()> {
        im::clear_maps(py)
    }

    fn active_maps(&self) -> PyResult<Vec<String>> {
        im::active_maps()
    }

    fn rebind(
        &self,
        map_id: &str,
        action_id: &str,
        binding_index: usize,
        new_key: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        im::rebind(map_id, action_id, binding_index, new_key)
    }

    fn __repr__(&self) -> String {
        "rython.input".to_string()
    }
}

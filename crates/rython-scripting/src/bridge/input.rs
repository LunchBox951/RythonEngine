use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;
use pyo3::prelude::*;
use rython_input::InputSnapshot;

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

    fn pressed(&self, action: &str) -> bool {
        input_store().lock().pressed(action)
    }

    fn held(&self, action: &str) -> bool {
        input_store().lock().held(action)
    }

    fn released(&self, action: &str) -> bool {
        input_store().lock().released(action)
    }

    fn __repr__(&self) -> String {
        "rython.input".to_string()
    }
}

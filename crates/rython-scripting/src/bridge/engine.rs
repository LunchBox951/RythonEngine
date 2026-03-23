use std::sync::atomic::Ordering;

use pyo3::prelude::*;

use super::QUIT_REQUESTED;

// ─── Engine bridge ────────────────────────────────────────────────────────────

/// Engine control bridge exposed as `rython.engine`.
#[pyclass(name = "EngineBridge")]
pub struct EngineBridge {}

#[pymethods]
impl EngineBridge {
    /// Signal the engine to exit cleanly after the current frame.
    fn request_quit(&self) {
        QUIT_REQUESTED.store(true, Ordering::Relaxed);
    }

    fn __repr__(&self) -> String {
        "rython.engine".to_string()
    }
}

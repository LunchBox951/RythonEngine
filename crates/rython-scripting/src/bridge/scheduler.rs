use pyo3::prelude::*;

use super::recurring_callbacks_store;

// ─── Scheduler bridge ─────────────────────────────────────────────────────────

/// Real scheduler bridge exposed as `rython.scheduler`.
#[pyclass(name = "SchedulerBridge")]
pub struct SchedulerBridge {}

#[pymethods]
impl SchedulerBridge {
    /// Register a callable to be invoked every frame.
    /// Wraps `TaskScheduler::register_recurring_sequential` at the Python level.
    fn register_recurring(&self, callback: Py<PyAny>) {
        recurring_callbacks_store().lock().push(callback);
    }

    fn __repr__(&self) -> String {
        "rython.scheduler".to_string()
    }
}

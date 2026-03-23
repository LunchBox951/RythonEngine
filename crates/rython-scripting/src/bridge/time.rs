use pyo3::prelude::*;

use super::get_elapsed_secs;

// ─── Time bridge ──────────────────────────────────────────────────────────────

/// Time utilities exposed as `rython.time`.
#[pyclass(name = "TimeBridge")]
pub struct TimeBridge {}

#[pymethods]
impl TimeBridge {
    /// Elapsed engine time in seconds since the engine started.
    #[getter]
    fn elapsed(&self) -> f64 {
        get_elapsed_secs()
    }

    fn __repr__(&self) -> String {
        "rython.time".to_string()
    }
}

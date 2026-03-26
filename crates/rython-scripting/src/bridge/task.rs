use std::sync::Arc;

use pyo3::prelude::*;

use super::job_handle::JobState;

/// A Python callable submitted for background (off-thread) execution.
pub(crate) struct PythonBgRequest {
    pub callback: Py<PyAny>,
    pub state: Arc<JobState>,
}

/// A Python callable submitted for parallel (same-frame, par phase) execution.
pub(crate) struct PythonParRequest {
    pub callback: Py<PyAny>,
    pub state: Arc<JobState>,
}

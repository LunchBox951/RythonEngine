use std::sync::Arc;

use parking_lot::Mutex;
use pyo3::prelude::*;

// ─── Job state ────────────────────────────────────────────────────────────────

pub(crate) struct JobStateInner {
    pub done: bool,
    pub failed: bool,
    pub error: Option<String>,
    pub on_complete: Vec<Py<PyAny>>,
}

impl JobStateInner {
    fn new() -> Self {
        Self {
            done: false,
            failed: false,
            error: None,
            on_complete: Vec::new(),
        }
    }
}

/// Shared state for a submitted job. Lives inside `Arc` so the Python handle
/// and the Rust worker both see the same state.
pub struct JobState(pub(crate) Mutex<JobStateInner>);

impl JobState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self(Mutex::new(JobStateInner::new())))
    }

    /// Mark done (success) and fire pending `on_complete` callbacks.
    pub fn complete(&self, py: Python<'_>) {
        let callbacks = {
            let mut inner = self.0.lock();
            inner.done = true;
            std::mem::take(&mut inner.on_complete)
        };
        for cb in callbacks {
            if let Err(e) = cb.bind(py).call0() {
                e.print(py);
            }
        }
    }

    /// Mark done (failure) and fire pending `on_complete` callbacks.
    pub fn fail(&self, py: Python<'_>, error: String) {
        let callbacks = {
            let mut inner = self.0.lock();
            inner.done = true;
            inner.failed = true;
            inner.error = Some(error);
            std::mem::take(&mut inner.on_complete)
        };
        for cb in callbacks {
            if let Err(e) = cb.bind(py).call0() {
                e.print(py);
            }
        }
    }
}

// ─── Python-visible JobHandle ─────────────────────────────────────────────────

/// Python object returned from `rython.scheduler.submit_background` and
/// `rython.scheduler.submit_parallel`.  Lets scripts poll or react to task
/// completion without blocking the main thread.
#[pyclass(name = "JobHandle")]
pub struct JobHandlePy {
    pub state: Arc<JobState>,
}

#[pymethods]
impl JobHandlePy {
    /// `True` once the task has finished (successfully or not).
    #[getter]
    fn is_done(&self) -> bool {
        self.state.0.lock().done
    }

    /// `True` while the task is still running or queued.
    #[getter]
    fn is_pending(&self) -> bool {
        !self.state.0.lock().done
    }

    /// `True` if the task finished with an error or uncaught exception.
    #[getter]
    fn is_failed(&self) -> bool {
        let inner = self.state.0.lock();
        inner.done && inner.failed
    }

    /// The error message if `is_failed`, otherwise `None`.
    #[getter]
    fn error(&self) -> Option<String> {
        self.state.0.lock().error.clone()
    }

    /// Register *callback* (zero-argument callable) to be called when the task
    /// completes.  If the task is already done the callback fires immediately.
    fn on_complete(&self, py: Python<'_>, callback: Py<PyAny>) -> PyResult<()> {
        let already_done = {
            let mut inner = self.state.0.lock();
            if inner.done {
                true
            } else {
                inner.on_complete.push(callback.clone_ref(py));
                false
            }
        };
        if already_done {
            callback.bind(py).call0()?;
        }
        Ok(())
    }

    fn __repr__(&self) -> String {
        let inner = self.state.0.lock();
        if inner.done {
            if inner.failed {
                format!("JobHandle(failed: {:?})", inner.error)
            } else {
                "JobHandle(done)".to_string()
            }
        } else {
            "JobHandle(pending)".to_string()
        }
    }
}

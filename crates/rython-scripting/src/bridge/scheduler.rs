use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

use super::{
    get_elapsed_secs, json_to_py_dict, recurring_callbacks_store, scene_store, timer_store,
    PendingTimer,
};

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

    /// Schedule `callback` to be called once after `delay_secs` seconds of
    /// elapsed engine time. The timer fires during the next `flush_timers`
    /// call whose elapsed time is >= (current_elapsed + delay_secs).
    fn on_timer(&self, delay_secs: f64, callback: Py<PyAny>) {
        let fire_at = get_elapsed_secs() + delay_secs;
        timer_store().lock().push(PendingTimer { fire_at, callback });
    }

    /// Subscribe `callback` to `event_name` exactly once.  After the first
    /// delivery the subscription becomes a no-op (deadlock-safe: we cannot
    /// unsubscribe from inside the event bus's read-lock, so we guard with an
    /// AtomicBool instead).
    fn on_event(&self, event_name: &str, callback: Py<PyAny>) -> PyResult<()> {
        let scene = {
            let guard = scene_store().lock();
            guard
                .as_ref()
                .cloned()
                .ok_or_else(|| PyErr::new::<PyRuntimeError, _>("No active scene"))?
        };

        let fired = Arc::new(AtomicBool::new(false));
        let fired_clone = Arc::clone(&fired);

        scene.subscribe(event_name, move |_name, payload| {
            // Guard against double-fire (AtomicBool swap: returns old value).
            if fired_clone.swap(true, Ordering::SeqCst) {
                return;
            }
            Python::attach(|py| {
                let kwargs = json_to_py_dict(py, payload).ok();
                let result = callback.bind(py).call((), kwargs.as_ref());
                if let Err(e) = result {
                    e.print(py);
                }
            });
        });

        Ok(())
    }

    fn __repr__(&self) -> String {
        "rython.scheduler".to_string()
    }
}

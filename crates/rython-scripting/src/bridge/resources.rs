use std::sync::Arc;

use parking_lot::RwLock;
use pyo3::prelude::*;
use rython_resources::{AssetHandle, ResourceManager, ResourceManagerConfig};

/// The active `ResourceManager` shared with the Python bridge.
///
/// This is a `RwLock<Option<_>>` (rather than a `OnceLock`) so the CLI can
/// unconditionally install its explicit `Arc<ResourceManager>` even if the
/// bridge was already touched (e.g. by a test importing `rython`, or by a
/// hot-reloaded script calling `load_mesh` before the CLI had a chance to
/// wire things up).  Silently dropping the CLI's Arc would orphan the
/// bridge onto a different manager than the one the CLI polls each frame,
/// leaving every Python handle stuck `Pending` forever.
static RESOURCE_MANAGER: RwLock<Option<Arc<ResourceManager>>> = RwLock::new(None);

/// Return a clone of the currently-active `Arc<ResourceManager>`.
///
/// If no manager has been installed yet, lazily initialize a default one so
/// early callers (tests, hot-reloaded scripts) still resolve against *some*
/// manager.  The CLI's subsequent `set_active_resources` call will replace
/// this implicit manager with the real one.
fn resource_store() -> Arc<ResourceManager> {
    // Fast path: read lock, clone the Arc, drop the guard before returning.
    if let Some(mgr) = RESOURCE_MANAGER.read().as_ref() {
        return Arc::clone(mgr);
    }
    // Slow path: upgrade to write and install a default manager if still None.
    let mut guard = RESOURCE_MANAGER.write();
    if let Some(mgr) = guard.as_ref() {
        return Arc::clone(mgr);
    }
    log::debug!(
        "resources bridge: no active ResourceManager installed; initializing default manager lazily"
    );
    let mgr = Arc::new(ResourceManager::new(ResourceManagerConfig::default()));
    *guard = Some(Arc::clone(&mgr));
    mgr
}

/// Share the engine `ResourceManager` with the Python bridge.
///
/// Overwrites any previously-installed manager unconditionally.  If a
/// manager was already installed (for example because the bridge was
/// touched before the CLI finished wiring), the swap is logged at info
/// level so the transition is observable.
///
/// Should be called before `ensure_rython_module()`, but calling it later
/// is safe — any `AssetHandle`s created against the previous manager will
/// continue to be serviced by that manager via their internal `Arc`, but
/// no *new* handles will be created against it.
pub fn set_active_resources(manager: Arc<ResourceManager>) {
    let mut guard = RESOURCE_MANAGER.write();
    if guard.is_some() {
        log::info!(
            "resources bridge: replacing previously-active ResourceManager with new instance"
        );
    }
    *guard = Some(manager);
}

// ─── AssetHandle bridge ───────────────────────────────────────────────────────

#[pyclass(name = "AssetHandle")]
pub struct AssetHandlePy {
    pub(crate) inner: AssetHandle,
}

impl AssetHandlePy {
    /// Clone the underlying `AssetHandle` for use in the pending-registration queue.
    pub fn clone_inner(&self) -> AssetHandle {
        self.inner.clone()
    }
}

#[pymethods]
impl AssetHandlePy {
    #[getter]
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }

    #[getter]
    fn is_pending(&self) -> bool {
        self.inner.is_pending()
    }

    #[getter]
    fn is_failed(&self) -> bool {
        self.inner.is_failed()
    }

    #[getter]
    fn error(&self) -> Option<String> {
        self.inner.error()
    }

    fn __repr__(&self) -> String {
        format!("AssetHandle(state={:?})", self.inner.state())
    }
}

// ─── Resources bridge ─────────────────────────────────────────────────────────

#[pyclass(name = "ResourcesBridge")]
pub struct ResourcesBridge {}

#[pymethods]
impl ResourcesBridge {
    fn load_image(&self, path: &str) -> AssetHandlePy {
        AssetHandlePy {
            inner: resource_store().load_image(path),
        }
    }

    fn load_mesh(&self, path: &str) -> AssetHandlePy {
        AssetHandlePy {
            inner: resource_store().load_mesh(path),
        }
    }

    fn load_sound(&self, path: &str) -> AssetHandlePy {
        AssetHandlePy {
            inner: resource_store().load_sound(path),
        }
    }

    #[pyo3(signature = (path, size = 16.0))]
    fn load_font(&self, path: &str, size: f32) -> AssetHandlePy {
        AssetHandlePy {
            inner: resource_store().load_font(path, size),
        }
    }

    #[pyo3(signature = (path, cols = 1, rows = 1))]
    fn load_spritesheet(&self, path: &str, cols: u32, rows: u32) -> AssetHandlePy {
        AssetHandlePy {
            inner: resource_store().load_spritesheet(path, cols, rows),
        }
    }

    fn memory_used_mb(&self) -> f64 {
        resource_store().memory_used_mb()
    }

    fn memory_budget_mb(&self) -> f64 {
        resource_store().memory_budget_mb()
    }

    fn __repr__(&self) -> String {
        "rython.resources".to_string()
    }
}

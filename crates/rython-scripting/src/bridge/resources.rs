use std::sync::{Arc, OnceLock};

use pyo3::prelude::*;
use rython_resources::{AssetHandle, ResourceManager, ResourceManagerConfig};

static RESOURCE_MANAGER: OnceLock<Arc<ResourceManager>> = OnceLock::new();

fn resource_store() -> &'static Arc<ResourceManager> {
    RESOURCE_MANAGER
        .get_or_init(|| Arc::new(ResourceManager::new(ResourceManagerConfig::default())))
}

/// Share the engine ResourceManager with the Python bridge.
/// Must be called before ensure_rython_module().
pub fn set_active_resources(manager: Arc<ResourceManager>) {
    let _ = RESOURCE_MANAGER.set(manager);
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

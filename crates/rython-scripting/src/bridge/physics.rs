use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use rython_physics::PhysicsWorld;

static PHYSICS_WORLD: OnceLock<Arc<Mutex<PhysicsWorld>>> = OnceLock::new();

pub(crate) fn physics_store() -> Option<&'static Arc<Mutex<PhysicsWorld>>> {
    PHYSICS_WORLD.get()
}

/// Share the engine PhysicsWorld with the Python bridge.
/// Must be called before ensure_rython_module().
pub fn set_active_physics(world: Arc<Mutex<PhysicsWorld>>) {
    let _ = PHYSICS_WORLD.set(world);
}

// ─── Physics bridge ───────────────────────────────────────────────────────────

#[pyclass(name = "PhysicsBridge")]
pub struct PhysicsBridge {}

#[pymethods]
impl PhysicsBridge {
    fn set_gravity(&self, x: f32, y: f32, z: f32) -> PyResult<()> {
        let world = physics_store().ok_or_else(|| {
            PyErr::new::<PyRuntimeError, _>("PhysicsWorld not initialized")
        })?;
        world.lock().set_gravity([x, y, z]);
        Ok(())
    }

    fn __repr__(&self) -> String {
        "rython.physics".to_string()
    }
}

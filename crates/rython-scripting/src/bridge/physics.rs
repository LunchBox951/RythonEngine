use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use rython_ecs::EntityId;
use rython_physics::PhysicsWorld;

use super::entity::EntityPy;
use super::types::Vec3Py;

static PHYSICS_WORLD: OnceLock<Arc<Mutex<PhysicsWorld>>> = OnceLock::new();

pub(crate) fn physics_store() -> Option<&'static Arc<Mutex<PhysicsWorld>>> {
    PHYSICS_WORLD.get()
}

/// Share the engine PhysicsWorld with the Python bridge.
/// Must be called before ensure_rython_module().
pub fn set_active_physics(world: Arc<Mutex<PhysicsWorld>>) {
    let _ = PHYSICS_WORLD.set(world);
}

// ─── RayHit Python wrapper ────────────────────────────────────────────────────

/// Result returned by `rython.physics.raycast` and `rython.physics.sphere_cast`.
///
/// `distance` is an alias for `toi` (time-of-impact equals distance along the
/// unit-normalised direction vector).
#[pyclass(name = "RayHit")]
pub struct RayHitPy {
    #[pyo3(get)]
    pub entity: Py<EntityPy>,
    #[pyo3(get)]
    pub point: Py<Vec3Py>,
    #[pyo3(get)]
    pub normal: Py<Vec3Py>,
    #[pyo3(get)]
    pub toi: f32,
}

#[pymethods]
impl RayHitPy {
    /// Alias for `toi`.  Distance along the (unit) ray to the hit surface.
    #[getter]
    fn distance(&self) -> f32 {
        self.toi
    }

    fn __repr__(&self) -> String {
        format!("RayHit(toi={})", self.toi)
    }
}

// ─── Physics bridge ───────────────────────────────────────────────────────────

#[pyclass(name = "PhysicsBridge")]
pub struct PhysicsBridge {}

#[pymethods]
impl PhysicsBridge {
    fn set_gravity(&self, x: f32, y: f32, z: f32) -> PyResult<()> {
        let world = physics_store()
            .ok_or_else(|| PyErr::new::<PyRuntimeError, _>("PhysicsWorld not initialized"))?;
        world.lock().set_gravity([x, y, z]);
        Ok(())
    }

    /// Cast a ray from `origin` in `direction` (both `(f32, f32, f32)` tuples)
    /// up to `max_dist` world units.  Returns a `RayHit` or `None`.
    fn raycast(
        &self,
        py: Python<'_>,
        origin: (f32, f32, f32),
        direction: (f32, f32, f32),
        max_dist: f32,
    ) -> PyResult<Option<Py<RayHitPy>>> {
        let world = physics_store()
            .ok_or_else(|| PyErr::new::<PyRuntimeError, _>("PhysicsWorld not initialized"))?;
        let hit = world.lock().raycast(
            [origin.0, origin.1, origin.2],
            [direction.0, direction.1, direction.2],
            max_dist,
        );
        match hit {
            None => Ok(None),
            Some(h) => {
                let entity = Py::new(py, EntityPy { id: h.entity.0, scene: None })?;
                let point = Py::new(
                    py,
                    Vec3Py::new(h.point[0], h.point[1], h.point[2]),
                )?;
                let normal = Py::new(
                    py,
                    Vec3Py::new(h.normal[0], h.normal[1], h.normal[2]),
                )?;
                Ok(Some(Py::new(py, RayHitPy { entity, point, normal, toi: h.toi })?))
            }
        }
    }

    /// Cast a sphere of `radius` from `origin` in `direction` up to `max_dist`
    /// world units.  `origin` and `direction` are `(f32, f32, f32)` tuples.
    /// Returns a `RayHit` or `None`.
    fn sphere_cast(
        &self,
        py: Python<'_>,
        origin: (f32, f32, f32),
        direction: (f32, f32, f32),
        radius: f32,
        max_dist: f32,
    ) -> PyResult<Option<Py<RayHitPy>>> {
        let world = physics_store()
            .ok_or_else(|| PyErr::new::<PyRuntimeError, _>("PhysicsWorld not initialized"))?;
        let hit = world.lock().sphere_cast(
            [origin.0, origin.1, origin.2],
            [direction.0, direction.1, direction.2],
            radius,
            max_dist,
        );
        match hit {
            None => Ok(None),
            Some(h) => {
                let entity = Py::new(py, EntityPy { id: h.entity.0, scene: None })?;
                let point = Py::new(
                    py,
                    Vec3Py::new(h.point[0], h.point[1], h.point[2]),
                )?;
                let normal = Py::new(
                    py,
                    Vec3Py::new(h.normal[0], h.normal[1], h.normal[2]),
                )?;
                Ok(Some(Py::new(py, RayHitPy { entity, point, normal, toi: h.toi })?))
            }
        }
    }

    /// Return the ground surface normal directly below `entity` (an `Entity`
    /// object) within `max_dist` world units.  Returns a `Vec3` or `None`.
    #[pyo3(signature = (entity, max_dist = 2.0))]
    fn ground_normal(
        &self,
        py: Python<'_>,
        entity: &EntityPy,
        max_dist: f32,
    ) -> PyResult<Option<Py<Vec3Py>>> {
        let world = physics_store()
            .ok_or_else(|| PyErr::new::<PyRuntimeError, _>("PhysicsWorld not initialized"))?;
        let normal = world
            .lock()
            .ground_normal(EntityId(entity.id), max_dist);
        match normal {
            None => Ok(None),
            Some(n) => Ok(Some(Py::new(py, Vec3Py::new(n[0], n[1], n[2]))?)),
        }
    }

    fn __repr__(&self) -> String {
        "rython.physics".to_string()
    }
}

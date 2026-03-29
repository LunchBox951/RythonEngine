use std::sync::Arc;

use pyo3::prelude::*;
use rython_ecs::component::{TagComponent, TransformComponent};
use rython_ecs::{EntityId, Scene};

use super::{physics::physics_store, scene_store, types::{TransformPy, Vec3Py}};

// ─── Entity wrapper ───────────────────────────────────────────────────────────

#[pyclass(name = "Entity")]
pub struct EntityPy {
    #[pyo3(get, set)]
    pub id: u64,
    /// Cached scene reference — avoids acquiring the global Mutex on every
    /// property access.  Populated at construction time; falls back to the
    /// global store when `None` (e.g. entities built from Python with the
    /// default constructor).
    pub scene: Option<Arc<Scene>>,
}

impl EntityPy {
    /// Resolve scene: use cached reference if available, otherwise fall back
    /// to the global store (single lock acquisition).
    #[inline]
    fn resolve_scene(&self) -> Option<Arc<Scene>> {
        if let Some(ref s) = self.scene {
            Some(Arc::clone(s))
        } else {
            scene_store().lock().as_ref().cloned()
        }
    }
}

#[pymethods]
impl EntityPy {
    #[new]
    #[pyo3(signature = (id = 0))]
    pub fn new(id: u64) -> Self {
        Self { id, scene: None }
    }

    #[getter]
    fn transform(&self) -> TransformPy {
        if let Some(scene) = self.resolve_scene() {
            let entity = EntityId(self.id);
            if let Some(t) = scene.components.get::<TransformComponent>(entity) {
                return TransformPy::from_component(&t, entity, Arc::clone(&scene));
            }
        }
        TransformPy::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, None, None, None)
    }

    fn has_tag(&self, tag: &str) -> bool {
        if let Some(scene) = self.resolve_scene() {
            let entity = EntityId(self.id);
            return scene
                .components
                .get_ref::<TagComponent, _, _>(entity, |t| t.tags.contains(&tag.to_string()))
                .unwrap_or(false);
        }
        false
    }

    fn add_tag(&self, tag: &str) {
        let tag_owned = tag.to_string();
        if let Some(scene) = self.resolve_scene() {
            let entity = EntityId(self.id);
            let tag_clone = tag_owned.clone();
            let existed = scene.components.get_mut(entity, |t: &mut TagComponent| {
                if !t.tags.contains(&tag_clone) {
                    t.tags.push(tag_clone.clone());
                }
            });
            if !existed {
                scene.components.insert(entity, TagComponent { tags: vec![tag_owned] });
            }
        }
    }

    fn despawn(&self) {
        if let Some(scene) = self.resolve_scene() {
            scene.queue_despawn(EntityId(self.id));
        }
    }

    // ─── Physics methods ──────────────────────────────────────────────────────

    fn apply_force(&self, x: f32, y: f32, z: f32) {
        if let Some(world) = physics_store() {
            world.lock().apply_force(EntityId(self.id), [x, y, z]);
        }
    }

    fn apply_impulse(&self, x: f32, y: f32, z: f32) {
        if let Some(world) = physics_store() {
            world.lock().apply_impulse(EntityId(self.id), [x, y, z]);
        }
    }

    fn set_velocity(&self, x: f32, y: f32, z: f32) {
        if let Some(world) = physics_store() {
            world.lock().set_linear_velocity(EntityId(self.id), [x, y, z]);
        }
    }

    #[getter]
    fn velocity(&self) -> Vec3Py {
        if let Some(world) = physics_store() {
            if let Some([vx, vy, vz]) = world.lock().get_linear_velocity(EntityId(self.id)) {
                return Vec3Py::new(vx, vy, vz);
            }
        }
        Vec3Py::new(0.0, 0.0, 0.0)
    }

    fn __repr__(&self) -> String {
        format!("Entity({})", self.id)
    }
}

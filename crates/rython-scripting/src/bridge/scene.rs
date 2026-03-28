use std::any::TypeId;
use std::collections::HashMap;

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use rython_ecs::component::{
    ColliderComponent, MeshComponent, RigidBodyComponent, TagComponent, TransformComponent,
};
use rython_ecs::EntityId;

use super::{
    entity::EntityPy, json_to_py_dict, py_value_to_json, register_script_class, scene_store,
    types::TransformPy,
};

// ─── Scene bridge ─────────────────────────────────────────────────────────────

#[pyclass(name = "SceneBridge")]
pub struct SceneBridge {}

#[pymethods]
impl SceneBridge {
    /// Spawn a new entity with optional components passed as keyword args.
    ///
    /// Supported kwargs:
    /// - `transform=rython.Transform(...)` → TransformComponent
    /// - `mesh="mesh_id"` or `mesh={"mesh_id": ..., "texture_id": ..., "visible": ...}` → MeshComponent
    /// - `tags=["tag1", "tag2"]` → TagComponent
    /// - `rigid_body={"body_type": "dynamic"|"static"|"kinematic", "mass": f32, "gravity_factor": f32}` → RigidBodyComponent
    /// - `collider={"shape": "box"|"sphere"|"capsule", "size": [f32; 3], "is_trigger": bool}` → ColliderComponent
    #[pyo3(signature = (**kwargs))]
    fn spawn(&self, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<EntityPy> {
        let mut components: Vec<(TypeId, Box<dyn rython_ecs::component::Component>)> = Vec::new();

        if let Some(kw) = kwargs {
            for (key, val) in kw.iter() {
                let key_str: String = key.extract()?;
                match key_str.as_str() {
                    "transform" => {
                        if let Ok(t) = val.extract::<PyRef<TransformPy>>() {
                            components.push((
                                TypeId::of::<TransformComponent>(),
                                Box::new(TransformComponent {
                                    x: t.x,
                                    y: t.y,
                                    z: t.z,
                                    rot_x: t.rot_x,
                                    rot_y: t.rot_y,
                                    rot_z: t.rot_z,
                                    scale_x: t.scale_x,
                                    scale_y: t.scale_y,
                                    scale_z: t.scale_z,
                                }),
                            ));
                        }
                    }
                    "mesh" => {
                        if let Ok(s) = val.extract::<String>() {
                            components.push((
                                TypeId::of::<MeshComponent>(),
                                Box::new(MeshComponent { mesh_id: s, ..Default::default() }),
                            ));
                        } else if let Ok(map) =
                            val.extract::<HashMap<String, Bound<'_, PyAny>>>()
                        {
                            let mesh_id = map
                                .get("mesh_id")
                                .and_then(|v| v.extract::<String>().ok())
                                .unwrap_or_default();
                            let texture_id = map
                                .get("texture_id")
                                .and_then(|v| v.extract::<String>().ok())
                                .unwrap_or_default();
                            let visible = map
                                .get("visible")
                                .and_then(|v| v.extract::<bool>().ok())
                                .unwrap_or(true);
                            let normal_map_id = map
                                .get("normal_map")
                                .and_then(|v| v.extract::<String>().ok())
                                .filter(|s| !s.is_empty());
                            let specular_map_id = map
                                .get("specular_map")
                                .and_then(|v| v.extract::<String>().ok())
                                .filter(|s| !s.is_empty());
                            let shininess = map
                                .get("shininess")
                                .and_then(|v| v.extract::<f32>().ok())
                                .unwrap_or(32.0);
                            let specular_color = map
                                .get("specular_color")
                                .and_then(|v| v.extract::<(f32, f32, f32)>().ok())
                                .map(|(r, g, b)| [r, g, b])
                                .unwrap_or([1.0, 1.0, 1.0]);
                            let metallic = map
                                .get("metallic")
                                .and_then(|v| v.extract::<f32>().ok())
                                .unwrap_or(0.0)
                                .clamp(0.0, 1.0);
                            let roughness = map
                                .get("roughness")
                                .and_then(|v| v.extract::<f32>().ok())
                                .unwrap_or(0.5)
                                .clamp(0.0, 1.0);
                            if map.get("metallic").and_then(|v| v.extract::<f32>().ok())
                                .is_some_and(|v| v < 0.0 || v > 1.0)
                            {
                                log::warn!("spawn mesh: metallic out of range — clamped to [0, 1]");
                            }
                            if map.get("roughness").and_then(|v| v.extract::<f32>().ok())
                                .is_some_and(|v| v < 0.0 || v > 1.0)
                            {
                                log::warn!("spawn mesh: roughness out of range — clamped to [0, 1]");
                            }
                            let emissive_map_id = map
                                .get("emissive_map")
                                .and_then(|v| v.extract::<String>().ok())
                                .filter(|s| !s.is_empty());
                            let emissive_color = map
                                .get("emissive_color")
                                .and_then(|v| v.extract::<(f32, f32, f32)>().ok())
                                .map(|(r, g, b)| [r, g, b, 0.0])
                                .unwrap_or([0.0, 0.0, 0.0, 0.0]);
                            let emissive_intensity = map
                                .get("emissive_intensity")
                                .and_then(|v| v.extract::<f32>().ok())
                                .unwrap_or(1.0)
                                .max(0.0);
                            if map.get("emissive_intensity").and_then(|v| v.extract::<f32>().ok())
                                .is_some_and(|v| v < 0.0)
                            {
                                log::warn!("spawn mesh: emissive_intensity < 0 — clamped to 0.0");
                            }
                            components.push((
                                TypeId::of::<MeshComponent>(),
                                Box::new(MeshComponent {
                                    mesh_id,
                                    texture_id,
                                    normal_map_id,
                                    specular_map_id,
                                    emissive_map_id,
                                    emissive_color,
                                    emissive_intensity,
                                    shininess,
                                    specular_color,
                                    visible,
                                    metallic,
                                    roughness,
                                    ..Default::default()
                                }),
                            ));
                        }
                    }
                    "tags" => {
                        if let Ok(tags) = val.extract::<Vec<String>>() {
                            components.push((
                                TypeId::of::<TagComponent>(),
                                Box::new(TagComponent { tags }),
                            ));
                        }
                    }
                    "rigid_body" => {
                        if let Ok(map) =
                            val.extract::<HashMap<String, Bound<'_, PyAny>>>()
                        {
                            let body_type = map
                                .get("body_type")
                                .and_then(|v| v.extract::<String>().ok())
                                .unwrap_or_else(|| "dynamic".to_string());
                            let mass = map
                                .get("mass")
                                .and_then(|v| v.extract::<f32>().ok())
                                .unwrap_or(1.0);
                            let gravity_factor = map
                                .get("gravity_factor")
                                .and_then(|v| v.extract::<f32>().ok())
                                .unwrap_or(1.0);
                            components.push((
                                TypeId::of::<RigidBodyComponent>(),
                                Box::new(RigidBodyComponent {
                                    body_type,
                                    mass,
                                    gravity_factor,
                                    collision_layer: 1,
                                    collision_mask: u32::MAX,
                                }),
                            ));
                        }
                    }
                    "collider" => {
                        if let Ok(map) =
                            val.extract::<HashMap<String, Bound<'_, PyAny>>>()
                        {
                            let shape = map
                                .get("shape")
                                .and_then(|v| v.extract::<String>().ok())
                                .unwrap_or_else(|| "box".to_string());
                            let size_vec = map
                                .get("size")
                                .and_then(|v| v.extract::<Vec<f32>>().ok())
                                .unwrap_or_else(|| vec![1.0, 1.0, 1.0]);
                            let size = [
                                size_vec.first().copied().unwrap_or(1.0),
                                size_vec.get(1).copied().unwrap_or(1.0),
                                size_vec.get(2).copied().unwrap_or(1.0),
                            ];
                            let is_trigger = map
                                .get("is_trigger")
                                .and_then(|v| v.extract::<bool>().ok())
                                .unwrap_or(false);
                            components.push((
                                TypeId::of::<ColliderComponent>(),
                                Box::new(ColliderComponent { shape, size, is_trigger }),
                            ));
                        }
                    }
                    _ => {}
                }
            }
        }

        let scene = {
            let guard = scene_store().lock();
            guard.as_ref().cloned().ok_or_else(|| PyErr::new::<PyRuntimeError, _>("No active scene"))?
        };

        let handle = scene.queue_spawn(components);
        scene.drain_commands();

        let eid = handle.get().ok_or_else(|| PyErr::new::<PyRuntimeError, _>("Spawn failed"))?;
        Ok(EntityPy { id: eid.0 })
    }

    /// Emit a custom named event with keyword payload.
    #[pyo3(signature = (event_name, **kwargs))]
    fn emit(&self, event_name: &str, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
        let mut payload = serde_json::json!({});
        if let Some(kw) = kwargs {
            for (key, val) in kw.iter() {
                let key_str: String = key.extract()?;
                let json_val = py_value_to_json(&val)?;
                payload[key_str] = json_val;
            }
        }

        let scene = {
            let guard = scene_store().lock();
            guard.as_ref().cloned()
        };
        if let Some(scene) = scene {
            scene.emit(event_name, payload);
        }
        Ok(())
    }

    /// Unsubscribe a previously registered handler by its ID.
    fn unsubscribe(&self, event_name: &str, handler_id: u64) -> PyResult<()> {
        let scene = {
            let guard = scene_store().lock();
            guard.as_ref().cloned().ok_or_else(|| PyErr::new::<PyRuntimeError, _>("No active scene"))?
        };
        scene.unsubscribe(event_name, handler_id);
        Ok(())
    }

    /// Subscribe a Python callable to a named event.
    fn subscribe(&self, event_name: &str, handler: Py<PyAny>) -> PyResult<u64> {
        let scene = {
            let guard = scene_store().lock();
            guard.as_ref().cloned().ok_or_else(|| PyErr::new::<PyRuntimeError, _>("No active scene"))?
        };

        let id = scene.subscribe(event_name, move |_name, payload| {
            Python::attach(|py| {
                let kwargs = json_to_py_dict(py, payload).ok();
                let result = handler.bind(py).call((), kwargs.as_ref());
                if let Err(e) = result {
                    e.print(py);
                }
            });
        });
        Ok(id)
    }

    /// Attach a Python script class to an entity.
    fn attach_script(
        &self,
        entity: &EntityPy,
        script_class: Py<PyAny>,
        py: Python<'_>,
    ) -> PyResult<()> {
        let class_name: String = script_class.bind(py).getattr("__name__")?.extract()?;
        register_script_class(&class_name, script_class);

        let scene = {
            let guard = scene_store().lock();
            guard.as_ref().cloned()
        };
        if let Some(scene) = scene {
            let entity_id = EntityId(entity.id);
            scene.components.insert(entity_id, crate::component::ScriptComponent {
                class_name: class_name.clone(),
            });
        }
        Ok(())
    }

    fn __repr__(&self) -> String {
        "rython.scene".to_string()
    }
}

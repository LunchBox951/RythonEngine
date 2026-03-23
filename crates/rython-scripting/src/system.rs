//! ScriptSystem: manages Python script instances and event dispatch.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use parking_lot::Mutex;
use pyo3::prelude::*;

use rython_ecs::{EntityId, Scene};

use crate::bridge::{EntityPy, Vec3Py, get_script_class, set_active_scene};
use crate::component::ScriptComponent;

// ─── GIL batch counter (for T-SCRIPT-19) ─────────────────────────────────────

static GIL_DISPATCH_COUNT: AtomicUsize = AtomicUsize::new(0);

pub fn gil_dispatch_count() -> usize {
    GIL_DISPATCH_COUNT.load(Ordering::SeqCst)
}

pub fn reset_gil_dispatch_count() {
    GIL_DISPATCH_COUNT.store(0, Ordering::SeqCst);
}

// ─── Script events ────────────────────────────────────────────────────────────

/// Events queued for batch Python dispatch.
#[derive(Clone)]
pub enum ScriptEvent {
    Collision {
        entity_a: EntityId,
        entity_b: EntityId,
        normal: [f32; 3],
    },
    TriggerEnter {
        entity: EntityId,
        other: EntityId,
    },
    TriggerExit {
        entity: EntityId,
        other: EntityId,
    },
    InputAction {
        entity: EntityId,
        action: String,
        value: f32,
    },
}

// ─── Per-entity script state ──────────────────────────────────────────────────

struct ScriptInstance {
    object: Py<PyAny>,
    class_name: String,
}

// ─── ScriptSystem ─────────────────────────────────────────────────────────────

pub struct ScriptSystem {
    scene: Arc<Scene>,
    instances: Mutex<HashMap<EntityId, ScriptInstance>>,
    pending_spawns: Mutex<Vec<EntityId>>,
    pending_despawns: Mutex<Vec<EntityId>>,
    event_queue: Mutex<Vec<ScriptEvent>>,
    error_log: Mutex<Vec<String>>,
}

impl ScriptSystem {
    pub fn new(scene: Arc<Scene>) -> Arc<Self> {
        let sys = Arc::new(Self {
            scene: Arc::clone(&scene),
            instances: Mutex::new(HashMap::new()),
            pending_spawns: Mutex::new(Vec::new()),
            pending_despawns: Mutex::new(Vec::new()),
            event_queue: Mutex::new(Vec::new()),
            error_log: Mutex::new(Vec::new()),
        });

        {
            let sys_weak = Arc::downgrade(&sys);
            scene.events.subscribe_entity_spawned(move |eid| {
                if let Some(s) = sys_weak.upgrade() {
                    s.pending_spawns.lock().push(EntityId(eid));
                }
            });
        }
        {
            let sys_weak = Arc::downgrade(&sys);
            scene.events.subscribe_entity_despawned(move |eid| {
                if let Some(s) = sys_weak.upgrade() {
                    s.pending_despawns.lock().push(EntityId(eid));
                }
            });
        }

        sys
    }

    pub fn queue_collision(&self, entity_a: EntityId, entity_b: EntityId, normal: [f32; 3]) {
        self.event_queue.lock().push(ScriptEvent::Collision { entity_a, entity_b, normal });
    }

    pub fn queue_trigger_enter(&self, entity: EntityId, other: EntityId) {
        self.event_queue.lock().push(ScriptEvent::TriggerEnter { entity, other });
    }

    pub fn queue_trigger_exit(&self, entity: EntityId, other: EntityId) {
        self.event_queue.lock().push(ScriptEvent::TriggerExit { entity, other });
    }

    pub fn queue_input_action(&self, entity: EntityId, action: impl Into<String>, value: f32) {
        self.event_queue.lock().push(ScriptEvent::InputAction {
            entity,
            action: action.into(),
            value,
        });
    }

    pub fn drain_errors(&self) -> Vec<String> {
        std::mem::take(&mut self.error_log.lock())
    }

    /// Process pending spawns/despawns and dispatch all queued events.
    /// Acquires the GIL exactly once. Call this at GAME_UPDATE and GAME_LATE
    /// batch boundaries to satisfy the "at most 2 GIL acquisitions per frame" requirement.
    pub fn flush(&self, py: Python<'_>) {
        GIL_DISPATCH_COUNT.fetch_add(1, Ordering::SeqCst);
        set_active_scene(Arc::clone(&self.scene));
        self.process_pending_despawns(py);
        self.process_pending_spawns(py);
        self.dispatch_events(py);
    }

    // ── Internal ─────────────────────────────────────────────────────────────

    fn process_pending_spawns(&self, py: Python<'_>) {
        let entities: Vec<EntityId> = std::mem::take(&mut self.pending_spawns.lock());
        for entity in entities {
            if let Some(sc) = self.scene.components.get::<ScriptComponent>(entity) {
                self.instantiate_script(py, entity, &sc.class_name);
            }
        }
    }

    fn process_pending_despawns(&self, py: Python<'_>) {
        let entities: Vec<EntityId> = std::mem::take(&mut self.pending_despawns.lock());
        for entity in entities {
            self.teardown_script(py, entity);
        }
    }

    fn instantiate_script(&self, py: Python<'_>, entity: EntityId, class_name: &str) {
        let class = match get_script_class(class_name) {
            Some(c) => c,
            None => {
                self.log_error(format!("Script class not registered: {class_name}"));
                return;
            }
        };

        let entity_wrapper = match Py::new(py, EntityPy { id: entity.0 }) {
            Ok(w) => w,
            Err(e) => {
                self.log_error(format!("Failed to create entity wrapper: {e}"));
                return;
            }
        };

        let instance = match class.bind(py).call1((entity_wrapper,)) {
            Ok(inst) => inst.unbind(),
            Err(e) => {
                self.log_python_error(py, &e, class_name, "__init__");
                return;
            }
        };

        // Call on_spawn if defined
        if instance.bind(py).hasattr("on_spawn").unwrap_or(false) {
            if let Err(e) = instance.bind(py).call_method0("on_spawn") {
                self.log_python_error(py, &e, class_name, "on_spawn");
            }
        }

        self.instances.lock().insert(entity, ScriptInstance {
            object: instance,
            class_name: class_name.to_string(),
        });
    }

    fn teardown_script(&self, py: Python<'_>, entity: EntityId) {
        let inst_opt = self.instances.lock().remove(&entity);
        if let Some(inst) = inst_opt {
            if inst.object.bind(py).hasattr("on_despawn").unwrap_or(false) {
                if let Err(e) = inst.object.bind(py).call_method0("on_despawn") {
                    self.log_python_error(py, &e, &inst.class_name, "on_despawn");
                }
            }
        }
    }

    fn dispatch_events(&self, py: Python<'_>) {
        let events: Vec<ScriptEvent> = std::mem::take(&mut self.event_queue.lock());
        for event in events {
            match event {
                ScriptEvent::Collision { entity_a, entity_b, normal } => {
                    let bw: Option<Py<PyAny>> =
                        Py::new(py, EntityPy { id: entity_b.0 }).ok().map(Into::into);
                    let aw: Option<Py<PyAny>> =
                        Py::new(py, EntityPy { id: entity_a.0 }).ok().map(Into::into);
                    let nv: Option<Py<PyAny>> =
                        Py::new(py, Vec3Py::new(normal[0], normal[1], normal[2]))
                            .ok()
                            .map(Into::into);
                    let nnv: Option<Py<PyAny>> =
                        Py::new(py, Vec3Py::new(-normal[0], -normal[1], -normal[2]))
                            .ok()
                            .map(Into::into);

                    if let (Some(bw), Some(nv)) = (bw, nv) {
                        self.call_handler_pair(py, entity_a, "on_collision", bw, nv);
                    }
                    if let (Some(aw), Some(nnv)) = (aw, nnv) {
                        self.call_handler_pair(py, entity_b, "on_collision", aw, nnv);
                    }
                }
                ScriptEvent::TriggerEnter { entity, other } => {
                    let ow: Option<Py<PyAny>> =
                        Py::new(py, EntityPy { id: other.0 }).ok().map(Into::into);
                    if let Some(ow) = ow {
                        self.call_handler_one(py, entity, "on_trigger_enter", ow);
                    }
                }
                ScriptEvent::TriggerExit { entity, other } => {
                    let ow: Option<Py<PyAny>> =
                        Py::new(py, EntityPy { id: other.0 }).ok().map(Into::into);
                    if let Some(ow) = ow {
                        self.call_handler_one(py, entity, "on_trigger_exit", ow);
                    }
                }
                ScriptEvent::InputAction { entity, action, value } => {
                    let ao: Option<Py<PyAny>> = action.into_pyobject(py).ok().map(|b| b.unbind().into());
                    let vo: Option<Py<PyAny>> = value.into_pyobject(py).ok().map(|b| b.unbind().into());
                    if let (Some(ao), Some(vo)) = (ao, vo) {
                        self.call_handler_pair(py, entity, "on_input_action", ao, vo);
                    }
                }
            }
        }
    }

    fn call_handler_one(&self, py: Python<'_>, entity: EntityId, method: &str, arg: Py<PyAny>) {
        let (obj, class_name) = {
            let guard = self.instances.lock();
            let inst = match guard.get(&entity) {
                Some(i) => i,
                None => return,
            };
            if !inst.object.bind(py).hasattr(method).unwrap_or(false) {
                return;
            }
            (inst.object.clone_ref(py), inst.class_name.clone())
        };
        if let Err(e) = obj.bind(py).call_method1(method, (arg,)) {
            self.log_python_error(py, &e, &class_name, method);
        }
    }

    fn call_handler_pair(
        &self,
        py: Python<'_>,
        entity: EntityId,
        method: &str,
        arg1: Py<PyAny>,
        arg2: Py<PyAny>,
    ) {
        let (obj, class_name) = {
            let guard = self.instances.lock();
            let inst = match guard.get(&entity) {
                Some(i) => i,
                None => return,
            };
            if !inst.object.bind(py).hasattr(method).unwrap_or(false) {
                return;
            }
            (inst.object.clone_ref(py), inst.class_name.clone())
        };
        if let Err(e) = obj.bind(py).call_method1(method, (arg1, arg2)) {
            self.log_python_error(py, &e, &class_name, method);
        }
    }

    fn log_python_error(&self, py: Python<'_>, err: &PyErr, script: &str, method: &str) {
        let tb = err
            .traceback(py)
            .and_then(|tb| tb.format().ok())
            .unwrap_or_default();
        let msg = format!("Script error in {script}.{method}: {err}\n{tb}");
        log::error!("{msg}");
        self.error_log.lock().push(msg);
    }

    fn log_error(&self, msg: String) {
        log::error!("{msg}");
        self.error_log.lock().push(msg);
    }

    /// Return the script instance object for an entity (for testing).
    pub fn get_instance(&self, entity: EntityId) -> Option<Py<PyAny>> {
        Python::attach(|py| self.instances.lock().get(&entity).map(|i| i.object.clone_ref(py)))
    }

    /// Directly instantiate a script for testing.
    pub fn instantiate_for_entity(&self, py: Python<'_>, entity: EntityId, class_name: &str) {
        self.instantiate_script(py, entity, class_name);
    }

    /// Hot-reload: replace script instance for an entity with a new class version.
    pub fn reload_entity_script(
        &self,
        py: Python<'_>,
        entity: EntityId,
        new_class: Py<PyAny>,
    ) -> PyResult<()> {
        let class_name: String = new_class.bind(py).getattr("__name__")?.extract()?;
        crate::bridge::register_script_class(&class_name, new_class);
        self.instances.lock().remove(&entity); // discard old without on_despawn
        self.instantiate_script(py, entity, &class_name);
        Ok(())
    }
}

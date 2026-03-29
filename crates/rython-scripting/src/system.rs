//! ScriptSystem: manages Python script instances and event dispatch.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use parking_lot::Mutex;
use pyo3::prelude::*;

use rython_ecs::{EntityId, Scene};

use crate::bridge::{EntityPy, Vec3Py, get_script_class_with_gil, set_active_scene};
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

/// Cached boolean flags for which event handlers are defined on a script class.
/// Inspected once at instantiation time so we never call `hasattr` per-frame.
struct HandlerFlags {
    on_spawn: bool,
    on_despawn: bool,
    on_collision: bool,
    on_trigger_enter: bool,
    on_trigger_exit: bool,
    on_input_action: bool,
}

impl HandlerFlags {
    fn inspect(_py: Python<'_>, obj: &Bound<'_, PyAny>) -> Self {
        Self {
            on_spawn: obj.hasattr("on_spawn").unwrap_or(false),
            on_despawn: obj.hasattr("on_despawn").unwrap_or(false),
            on_collision: obj.hasattr("on_collision").unwrap_or(false),
            on_trigger_enter: obj.hasattr("on_trigger_enter").unwrap_or(false),
            on_trigger_exit: obj.hasattr("on_trigger_exit").unwrap_or(false),
            on_input_action: obj.hasattr("on_input_action").unwrap_or(false),
        }
    }
}

struct ScriptInstance {
    object: Py<PyAny>,
    class_name: String,
    handlers: HandlerFlags,
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
        let class = match get_script_class_with_gil(py, class_name) {
            Some(c) => c,
            None => {
                self.log_error(format!("Script class not registered: {class_name}"));
                return;
            }
        };

        let entity_wrapper = match Py::new(py, EntityPy {
            id: entity.0,
            scene: Some(Arc::clone(&self.scene)),
        }) {
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

        // Cache which handlers are defined on this script class (once).
        let handlers = HandlerFlags::inspect(py, instance.bind(py));

        // Call on_spawn if defined
        if handlers.on_spawn {
            if let Err(e) = instance.bind(py).call_method0("on_spawn") {
                self.log_python_error(py, &e, class_name, "on_spawn");
            }
        }

        self.instances.lock().insert(entity, ScriptInstance {
            object: instance,
            class_name: class_name.to_string(),
            handlers,
        });
    }

    fn teardown_script(&self, py: Python<'_>, entity: EntityId) {
        let inst_opt = self.instances.lock().remove(&entity);
        if let Some(inst) = inst_opt {
            if inst.handlers.on_despawn {
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
                // Fix #3: Lazily construct EntityPy / Vec3Py only when a handler
                // exists — avoids 4 Py::new heap allocations per collision when
                // the script has no on_collision handler.
                ScriptEvent::Collision { entity_a, entity_b, normal } => {
                    let scene = &self.scene;
                    self.call_handler_pair_inline(
                        py, entity_a, "on_collision",
                        |py| Py::new(py, EntityPy { id: entity_b.0, scene: Some(Arc::clone(scene)) }).ok().map(Into::into),
                        |py| Py::new(py, Vec3Py::new(normal[0], normal[1], normal[2])).ok().map(Into::into),
                    );
                    self.call_handler_pair_inline(
                        py, entity_b, "on_collision",
                        |py| Py::new(py, EntityPy { id: entity_a.0, scene: Some(Arc::clone(scene)) }).ok().map(Into::into),
                        |py| Py::new(py, Vec3Py::new(-normal[0], -normal[1], -normal[2])).ok().map(Into::into),
                    );
                }
                ScriptEvent::TriggerEnter { entity, other } => {
                    let scene = &self.scene;
                    self.call_handler_one_lazy(py, entity, "on_trigger_enter",
                        |py| Py::new(py, EntityPy { id: other.0, scene: Some(Arc::clone(scene)) }).ok().map(Into::into));
                }
                ScriptEvent::TriggerExit { entity, other } => {
                    let scene = &self.scene;
                    self.call_handler_one_lazy(py, entity, "on_trigger_exit",
                        |py| Py::new(py, EntityPy { id: other.0, scene: Some(Arc::clone(scene)) }).ok().map(Into::into));
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

    /// Check the cached handler flag for the given method name.
    #[inline]
    fn has_handler(inst: &ScriptInstance, method: &str) -> bool {
        match method {
            "on_collision" => inst.handlers.on_collision,
            "on_trigger_enter" => inst.handlers.on_trigger_enter,
            "on_trigger_exit" => inst.handlers.on_trigger_exit,
            "on_input_action" => inst.handlers.on_input_action,
            "on_spawn" => inst.handlers.on_spawn,
            "on_despawn" => inst.handlers.on_despawn,
            _ => false,
        }
    }

    /// Lazily construct the argument only when the handler exists — avoids
    /// heap allocations for entities without the handler.
    fn call_handler_one_lazy<F>(
        &self,
        py: Python<'_>,
        entity: EntityId,
        method: &str,
        make_arg: F,
    )
    where
        F: FnOnce(Python<'_>) -> Option<Py<PyAny>>,
    {
        let (obj, class_name) = {
            let guard = self.instances.lock();
            let inst = match guard.get(&entity) {
                Some(i) => i,
                None => return,
            };
            if !Self::has_handler(inst, method) {
                return;
            }
            (inst.object.clone_ref(py), inst.class_name.clone())
        };
        let Some(arg) = make_arg(py) else { return };
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
            if !Self::has_handler(inst, method) {
                return;
            }
            (inst.object.clone_ref(py), inst.class_name.clone())
        };
        if let Err(e) = obj.bind(py).call_method1(method, (arg1, arg2)) {
            self.log_python_error(py, &e, &class_name, method);
        }
    }

    /// Like `call_handler_pair` but lazily constructs arguments only when the
    /// handler exists — avoids heap allocations when no handler is registered.
    fn call_handler_pair_inline<F1, F2>(
        &self,
        py: Python<'_>,
        entity: EntityId,
        method: &str,
        make_arg1: F1,
        make_arg2: F2,
    )
    where
        F1: FnOnce(Python<'_>) -> Option<Py<PyAny>>,
        F2: FnOnce(Python<'_>) -> Option<Py<PyAny>>,
    {
        let (obj, class_name) = {
            let guard = self.instances.lock();
            let inst = match guard.get(&entity) {
                Some(i) => i,
                None => return,
            };
            if !Self::has_handler(inst, method) {
                return;
            }
            (inst.object.clone_ref(py), inst.class_name.clone())
        };
        let (Some(a1), Some(a2)) = (make_arg1(py), make_arg2(py)) else { return };
        if let Err(e) = obj.bind(py).call_method1(method, (a1, a2)) {
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

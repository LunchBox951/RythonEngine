//! PyO3 bridge: defines the `rython` Python module and all wrapper types.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule, PyString};

use rython_ecs::component::{MeshComponent, TagComponent, TransformComponent};
use rython_ecs::{EntityId, Scene};
use rython_renderer::command::{Color, DrawCommand, DrawText};

// ─── Global state ─────────────────────────────────────────────────────────────

static ACTIVE_SCENE: OnceLock<Arc<Mutex<Option<Arc<Scene>>>>> = OnceLock::new();

fn scene_store() -> &'static Arc<Mutex<Option<Arc<Scene>>>> {
    ACTIVE_SCENE.get_or_init(|| Arc::new(Mutex::new(None)))
}

/// Set the scene that Python wrapper types will use for ECS access.
pub fn set_active_scene(scene: Arc<Scene>) {
    *scene_store().lock() = Some(scene);
}

// ─── Script class registry ────────────────────────────────────────────────────

static SCRIPT_CLASSES: OnceLock<Arc<Mutex<HashMap<String, Py<PyAny>>>>> = OnceLock::new();

pub fn class_registry() -> &'static Arc<Mutex<HashMap<String, Py<PyAny>>>> {
    SCRIPT_CLASSES.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
}

/// Register a Python class by name so `ScriptSystem` can instantiate it.
pub fn register_script_class(name: impl Into<String>, class: Py<PyAny>) {
    class_registry().lock().insert(name.into(), class);
}

/// Retrieve a script class by name (acquires GIL to clone the reference).
pub fn get_script_class(name: &str) -> Option<Py<PyAny>> {
    Python::attach(|py| class_registry().lock().get(name).map(|p| p.clone_ref(py)))
}

// ─── Draw command buffer ──────────────────────────────────────────────────────

static DRAW_COMMANDS: OnceLock<Arc<Mutex<Vec<DrawCommand>>>> = OnceLock::new();

fn draw_commands_store() -> &'static Arc<Mutex<Vec<DrawCommand>>> {
    DRAW_COMMANDS.get_or_init(|| Arc::new(Mutex::new(Vec::new())))
}

/// Drain all queued draw commands (call from the renderer each frame).
pub fn drain_draw_commands() -> Vec<DrawCommand> {
    std::mem::take(&mut draw_commands_store().lock())
}

// ─── Recurring Python callbacks ───────────────────────────────────────────────

static RECURRING_CALLBACKS: OnceLock<Arc<Mutex<Vec<Py<PyAny>>>>> = OnceLock::new();

fn recurring_callbacks_store() -> &'static Arc<Mutex<Vec<Py<PyAny>>>> {
    RECURRING_CALLBACKS.get_or_init(|| Arc::new(Mutex::new(Vec::new())))
}

/// Invoke all recurring Python callbacks registered via `rython.scheduler.register_recurring`.
/// Call once per frame while holding the GIL.
pub fn flush_recurring_callbacks(py: Python<'_>) {
    // clone_ref requires the GIL; collect first so we don't hold the lock while calling Python
    let callbacks: Vec<Py<PyAny>> = {
        let guard = recurring_callbacks_store().lock();
        guard.iter().map(|cb| cb.clone_ref(py)).collect()
    };
    for cb in &callbacks {
        if let Err(e) = cb.bind(py).call0() {
            e.print_and_set_sys_last_vars(py);
        }
    }
}

/// Remove all registered recurring callbacks (useful between tests).
pub fn clear_recurring_callbacks() {
    recurring_callbacks_store().lock().clear();
}

// ─── Engine time ──────────────────────────────────────────────────────────────

static ELAPSED_SECS_BITS: AtomicU64 = AtomicU64::new(0);

/// Update the elapsed engine time (call from the game loop each frame).
pub fn set_elapsed_secs(secs: f64) {
    ELAPSED_SECS_BITS.store(secs.to_bits(), Ordering::Relaxed);
}

fn get_elapsed_secs() -> f64 {
    f64::from_bits(ELAPSED_SECS_BITS.load(Ordering::Relaxed))
}

// ─── Quit flag ────────────────────────────────────────────────────────────────

static QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Returns true if Python scripts have called `rython.engine.request_quit()`.
pub fn was_quit_requested() -> bool {
    QUIT_REQUESTED.load(Ordering::Relaxed)
}

/// Reset the quit flag (call after the engine has handled the quit signal, or in tests).
pub fn reset_quit_requested() {
    QUIT_REQUESTED.store(false, Ordering::Relaxed);
}

// ─── Vec3 wrapper ─────────────────────────────────────────────────────────────

#[pyclass(name = "Vec3")]
pub struct Vec3Py {
    #[pyo3(get, set)]
    pub x: f32,
    #[pyo3(get, set)]
    pub y: f32,
    #[pyo3(get, set)]
    pub z: f32,
}

#[pymethods]
impl Vec3Py {
    #[new]
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn length(&self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    pub fn normalized(&self) -> Self {
        let len = self.length();
        if len < f32::EPSILON {
            Self { x: 0.0, y: 0.0, z: 0.0 }
        } else {
            Self { x: self.x / len, y: self.y / len, z: self.z / len }
        }
    }

    pub fn dot(&self, other: &Vec3Py) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    fn __add__(&self, other: &Vec3Py) -> Vec3Py {
        Vec3Py { x: self.x + other.x, y: self.y + other.y, z: self.z + other.z }
    }

    fn __sub__(&self, other: &Vec3Py) -> Vec3Py {
        Vec3Py { x: self.x - other.x, y: self.y - other.y, z: self.z - other.z }
    }

    fn __mul__(&self, scalar: f32) -> Vec3Py {
        Vec3Py { x: self.x * scalar, y: self.y * scalar, z: self.z * scalar }
    }

    fn __rmul__(&self, scalar: f32) -> Vec3Py {
        Vec3Py { x: self.x * scalar, y: self.y * scalar, z: self.z * scalar }
    }

    fn __neg__(&self) -> Vec3Py {
        Vec3Py { x: -self.x, y: -self.y, z: -self.z }
    }

    fn __repr__(&self) -> String {
        format!("Vec3({}, {}, {})", self.x, self.y, self.z)
    }
}

// ─── Transform wrapper ────────────────────────────────────────────────────────

#[pyclass(name = "Transform")]
pub struct TransformPy {
    /// Entity this transform is bound to (None = standalone value).
    pub entity_id: Option<u64>,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rot_x: f32,
    pub rot_y: f32,
    pub rot_z: f32,
    pub scale: f32,
}

impl TransformPy {
    pub fn from_component(comp: &TransformComponent, entity_id: EntityId) -> Self {
        Self {
            entity_id: Some(entity_id.0),
            x: comp.x,
            y: comp.y,
            z: comp.z,
            rot_x: comp.rot_x,
            rot_y: comp.rot_y,
            rot_z: comp.rot_z,
            scale: comp.scale,
        }
    }

    fn write_back(&self) {
        let Some(eid) = self.entity_id else { return };
        let guard = scene_store().lock();
        if let Some(scene) = guard.as_ref() {
            let entity = EntityId(eid);
            let (x, y, z, rx, ry, rz, s) =
                (self.x, self.y, self.z, self.rot_x, self.rot_y, self.rot_z, self.scale);
            scene.components.get_mut(entity, |t: &mut TransformComponent| {
                t.x = x;
                t.y = y;
                t.z = z;
                t.rot_x = rx;
                t.rot_y = ry;
                t.rot_z = rz;
                t.scale = s;
            });
        }
    }
}

#[pymethods]
impl TransformPy {
    #[new]
    #[pyo3(signature = (x=0.0, y=0.0, z=0.0, rot_x=0.0, rot_y=0.0, rot_z=0.0, scale=1.0))]
    pub fn new(
        x: f32,
        y: f32,
        z: f32,
        rot_x: f32,
        rot_y: f32,
        rot_z: f32,
        scale: f32,
    ) -> Self {
        Self { entity_id: None, x, y, z, rot_x, rot_y, rot_z, scale }
    }

    #[getter]
    fn x(&self) -> f32 {
        self.x
    }
    #[setter]
    fn set_x(&mut self, val: f32) {
        self.x = val;
        self.write_back();
    }

    #[getter]
    fn y(&self) -> f32 {
        self.y
    }
    #[setter]
    fn set_y(&mut self, val: f32) {
        self.y = val;
        self.write_back();
    }

    #[getter]
    fn z(&self) -> f32 {
        self.z
    }
    #[setter]
    fn set_z(&mut self, val: f32) {
        self.z = val;
        self.write_back();
    }

    #[getter]
    fn rot_x(&self) -> f32 {
        self.rot_x
    }
    #[setter]
    fn set_rot_x(&mut self, val: f32) {
        self.rot_x = val;
        self.write_back();
    }

    #[getter]
    fn rot_y(&self) -> f32 {
        self.rot_y
    }
    #[setter]
    fn set_rot_y(&mut self, val: f32) {
        self.rot_y = val;
        self.write_back();
    }

    #[getter]
    fn rot_z(&self) -> f32 {
        self.rot_z
    }
    #[setter]
    fn set_rot_z(&mut self, val: f32) {
        self.rot_z = val;
        self.write_back();
    }

    #[getter]
    fn scale(&self) -> f32 {
        self.scale
    }
    #[setter]
    fn set_scale(&mut self, val: f32) {
        self.scale = val;
        self.write_back();
    }

    fn __repr__(&self) -> String {
        format!(
            "Transform(x={}, y={}, z={}, rot_x={}, rot_y={}, rot_z={}, scale={})",
            self.x, self.y, self.z, self.rot_x, self.rot_y, self.rot_z, self.scale
        )
    }
}

// ─── Entity wrapper ───────────────────────────────────────────────────────────

#[pyclass(name = "Entity")]
pub struct EntityPy {
    #[pyo3(get, set)]
    pub id: u64,
}

#[pymethods]
impl EntityPy {
    #[new]
    #[pyo3(signature = (id = 0))]
    pub fn new(id: u64) -> Self {
        Self { id }
    }

    #[getter]
    fn transform(&self) -> TransformPy {
        let guard = scene_store().lock();
        if let Some(scene) = guard.as_ref() {
            let entity = EntityId(self.id);
            if let Some(t) = scene.components.get::<TransformComponent>(entity) {
                return TransformPy::from_component(&t, entity);
            }
        }
        TransformPy::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0)
    }

    fn has_tag(&self, tag: &str) -> bool {
        let guard = scene_store().lock();
        if let Some(scene) = guard.as_ref() {
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
        let guard = scene_store().lock();
        if let Some(scene) = guard.as_ref() {
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
        let guard = scene_store().lock();
        if let Some(scene) = guard.as_ref() {
            scene.queue_despawn(EntityId(self.id));
        }
    }

    fn __repr__(&self) -> String {
        format!("Entity({})", self.id)
    }
}

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
    #[pyo3(signature = (**kwargs))]
    fn spawn(&self, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<EntityPy> {
        use std::any::TypeId;

        let guard = scene_store().lock();
        let scene = guard
            .as_ref()
            .ok_or_else(|| PyErr::new::<PyRuntimeError, _>("No active scene"))?;

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
                                    scale: t.scale,
                                }),
                            ));
                        }
                    }
                    "mesh" => {
                        if let Ok(s) = val.extract::<String>() {
                            // mesh="mesh_id" shorthand
                            components.push((
                                TypeId::of::<MeshComponent>(),
                                Box::new(MeshComponent { mesh_id: s, ..Default::default() }),
                            ));
                        } else if let Ok(map) =
                            val.extract::<HashMap<String, Bound<'_, PyAny>>>()
                        {
                            // mesh={"mesh_id": ..., "texture_id": ..., "visible": ...}
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
                            components.push((
                                TypeId::of::<MeshComponent>(),
                                Box::new(MeshComponent {
                                    mesh_id,
                                    texture_id,
                                    visible,
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
                    _ => {}
                }
            }
        }

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

        let guard = scene_store().lock();
        if let Some(scene) = guard.as_ref() {
            scene.emit(event_name, payload);
        }
        Ok(())
    }

    /// Subscribe a Python callable to a named event.
    fn subscribe(&self, event_name: &str, handler: Py<PyAny>) -> PyResult<u64> {
        let guard = scene_store().lock();
        let scene =
            guard.as_ref().ok_or_else(|| PyErr::new::<PyRuntimeError, _>("No active scene"))?;

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

        let guard = scene_store().lock();
        if let Some(scene) = guard.as_ref() {
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

// ─── Camera bridge ────────────────────────────────────────────────────────────

/// Real camera object exposed as `rython.camera`.
#[pyclass(name = "Camera")]
pub struct CameraPy {
    pub pos_x: f32,
    pub pos_y: f32,
    pub pos_z: f32,
    pub rot_pitch: f32,
    pub rot_yaw: f32,
    pub rot_roll: f32,
}

#[pymethods]
impl CameraPy {
    #[new]
    pub fn new() -> Self {
        Self { pos_x: 0.0, pos_y: 0.0, pos_z: -10.0, rot_pitch: 0.0, rot_yaw: 0.0, rot_roll: 0.0 }
    }

    /// Set the camera world-space position.
    fn set_position(&mut self, x: f32, y: f32, z: f32) {
        self.pos_x = x;
        self.pos_y = y;
        self.pos_z = z;
    }

    /// Set the camera orientation as Euler angles (pitch, yaw, roll) in radians.
    fn set_rotation(&mut self, pitch: f32, yaw: f32, roll: f32) {
        self.rot_pitch = pitch;
        self.rot_yaw = yaw;
        self.rot_roll = roll;
    }

    /// Point the camera at a world-space target from its current position.
    fn set_look_at(&mut self, target_x: f32, target_y: f32, target_z: f32) {
        let dx = target_x - self.pos_x;
        let dy = target_y - self.pos_y;
        let dz = target_z - self.pos_z;
        let horiz = (dx * dx + dz * dz).sqrt();
        self.rot_yaw = dx.atan2(dz);
        self.rot_pitch = (-dy).atan2(horiz);
        self.rot_roll = 0.0;
    }

    #[getter]
    fn pos_x(&self) -> f32 {
        self.pos_x
    }
    #[getter]
    fn pos_y(&self) -> f32 {
        self.pos_y
    }
    #[getter]
    fn pos_z(&self) -> f32 {
        self.pos_z
    }
    #[getter]
    fn rot_pitch(&self) -> f32 {
        self.rot_pitch
    }
    #[getter]
    fn rot_yaw(&self) -> f32 {
        self.rot_yaw
    }
    #[getter]
    fn rot_roll(&self) -> f32 {
        self.rot_roll
    }

    fn __repr__(&self) -> String {
        format!(
            "Camera(pos=({}, {}, {}), pitch={:.3}, yaw={:.3}, roll={:.3})",
            self.pos_x, self.pos_y, self.pos_z, self.rot_pitch, self.rot_yaw, self.rot_roll
        )
    }
}

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

    fn __repr__(&self) -> String {
        "rython.scheduler".to_string()
    }
}

// ─── Renderer bridge ──────────────────────────────────────────────────────────

/// Real renderer bridge exposed as `rython.renderer`.
#[pyclass(name = "RendererBridge")]
pub struct RendererBridge {}

#[pymethods]
impl RendererBridge {
    /// Queue a text draw command for the current frame.
    #[pyo3(signature = (text, font_id = "default", x = 0.5, y = 0.1, size = 16, r = 255, g = 255, b = 255, z = 0.0))]
    fn draw_text(
        &self,
        text: &str,
        font_id: &str,
        x: f32,
        y: f32,
        size: u32,
        r: u8,
        g: u8,
        b: u8,
        z: f32,
    ) {
        draw_commands_store().lock().push(DrawCommand::Text(DrawText {
            text: text.to_string(),
            font_id: font_id.to_string(),
            x,
            y,
            color: Color::rgb(r, g, b),
            size,
            z,
        }));
    }

    fn __repr__(&self) -> String {
        "rython.renderer".to_string()
    }
}

// ─── Time bridge ─────────────────────────────────────────────────────────────

/// Time utilities exposed as `rython.time`.
#[pyclass(name = "TimeBridge")]
pub struct TimeBridge {}

#[pymethods]
impl TimeBridge {
    /// Elapsed engine time in seconds since the engine started.
    #[getter]
    fn elapsed(&self) -> f64 {
        get_elapsed_secs()
    }

    fn __repr__(&self) -> String {
        "rython.time".to_string()
    }
}

// ─── Engine bridge ────────────────────────────────────────────────────────────

/// Engine control bridge exposed as `rython.engine`.
#[pyclass(name = "EngineBridge")]
pub struct EngineBridge {}

#[pymethods]
impl EngineBridge {
    /// Signal the engine to exit cleanly after the current frame.
    fn request_quit(&self) {
        QUIT_REQUESTED.store(true, Ordering::Relaxed);
    }

    fn __repr__(&self) -> String {
        "rython.engine".to_string()
    }
}

// ─── Stub namespace ───────────────────────────────────────────────────────────

#[pyclass(name = "SubModule")]
pub struct SubModulePy {
    name: String,
}

#[pymethods]
impl SubModulePy {
    fn __repr__(&self) -> String {
        format!("rython.{}", self.name)
    }

    fn __getattr__(&self, _attr: &str) -> PyResult<Py<PyAny>> {
        Err(PyErr::new::<PyValueError, _>(format!("rython.{} is a stub", self.name)))
    }
}

// ─── JSON ↔ Python helpers ────────────────────────────────────────────────────

fn py_value_to_json(val: &Bound<'_, PyAny>) -> PyResult<serde_json::Value> {
    if val.is_none() {
        Ok(serde_json::Value::Null)
    } else if let Ok(b) = val.extract::<bool>() {
        Ok(serde_json::Value::Bool(b))
    } else if let Ok(i) = val.extract::<i64>() {
        Ok(serde_json::Value::Number(i.into()))
    } else if let Ok(f) = val.extract::<f64>() {
        Ok(serde_json::json!(f))
    } else if let Ok(s) = val.extract::<String>() {
        Ok(serde_json::Value::String(s))
    } else {
        Ok(serde_json::Value::Null)
    }
}

pub fn json_to_py_dict<'py>(
    py: Python<'py>,
    val: &serde_json::Value,
) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    if let serde_json::Value::Object(map) = val {
        for (k, v) in map {
            let py_val = json_val_to_py(py, v)?;
            dict.set_item(PyString::new(py, k), py_val)?;
        }
    }
    Ok(dict)
}

fn json_val_to_py<'py>(py: Python<'py>, val: &serde_json::Value) -> PyResult<Bound<'py, PyAny>> {
    use pyo3::types::{PyBool, PyNone};
    Ok(match val {
        serde_json::Value::Null => PyNone::get(py).as_any().to_owned(),
        serde_json::Value::Bool(b) => PyBool::new(py, *b).as_any().to_owned(),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.into_pyobject(py)?.into_any()
            } else {
                n.as_f64().unwrap_or(0.0).into_pyobject(py)?.into_any()
            }
        }
        serde_json::Value::String(s) => PyString::new(py, s).into_any(),
        _ => PyNone::get(py).as_any().to_owned(),
    })
}

// ─── Module initialisation ────────────────────────────────────────────────────

/// Create or update the `rython` module in `sys.modules`.
///
/// Safe to call multiple times — only registers the module once.
pub fn ensure_rython_module(py: Python<'_>, scene: Arc<Scene>) -> PyResult<()> {
    set_active_scene(scene);

    let sys = py.import("sys")?;
    let sys_modules = sys.getattr("modules")?;

    // Check if already registered
    if sys_modules.get_item("rython").is_ok() {
        return Ok(());
    }

    let rython = PyModule::new(py, "rython")?;
    rython.add_class::<Vec3Py>()?;
    rython.add_class::<TransformPy>()?;
    rython.add_class::<EntityPy>()?;

    // Scene bridge
    let scene_bridge = Py::new(py, SceneBridge {})?;
    rython.add("scene", scene_bridge)?;

    // Real sub-module implementations
    let camera = Py::new(py, CameraPy::new())?;
    rython.add("camera", camera)?;

    let scheduler = Py::new(py, SchedulerBridge {})?;
    rython.add("scheduler", scheduler)?;

    let renderer = Py::new(py, RendererBridge {})?;
    rython.add("renderer", renderer)?;

    let time = Py::new(py, TimeBridge {})?;
    rython.add("time", time)?;

    let engine = Py::new(py, EngineBridge {})?;
    rython.add("engine", engine)?;

    // Remaining sub-modules still as stubs
    for name in &["physics", "audio", "input", "ui", "resources", "modules"] {
        let stub = Py::new(py, SubModulePy { name: name.to_string() })?;
        rython.add(*name, stub)?;
    }

    sys_modules.set_item("rython", &rython)?;
    Ok(())
}

/// Load scripts from a zip bundle into `sys.path` (release mode).
pub fn load_bundle(py: Python<'_>, bundle_path: &str) -> PyResult<()> {
    let sys = py.import("sys")?;
    let path = sys.getattr("path")?;
    path.call_method1("insert", (0i32, bundle_path))?;
    Ok(())
}

/// Import the entry point module and call its `init()` function if present.
pub fn call_entry_point(py: Python<'_>, module_name: &str) -> PyResult<()> {
    let module = py.import(PyString::new(py, module_name))?;
    if module.hasattr("init")? {
        module.call_method0("init")?;
    }
    Ok(())
}

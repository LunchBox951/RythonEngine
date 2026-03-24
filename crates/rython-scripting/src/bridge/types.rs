use pyo3::prelude::*;
use rython_ecs::component::TransformComponent;
use rython_ecs::EntityId;

use super::scene_store;

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
    pub scale_x: f32,
    pub scale_y: f32,
    pub scale_z: f32,
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
            scale_x: comp.scale_x,
            scale_y: comp.scale_y,
            scale_z: comp.scale_z,
        }
    }

    fn write_back(&self) {
        let Some(eid) = self.entity_id else { return };
        let scene = { let guard = scene_store().lock(); guard.as_ref().cloned() };
        if let Some(scene) = scene {
            let entity = EntityId(eid);
            let (x, y, z, rx, ry, rz, sx, sy, sz) =
                (self.x, self.y, self.z, self.rot_x, self.rot_y, self.rot_z,
                 self.scale_x, self.scale_y, self.scale_z);
            scene.components.get_mut(entity, |t: &mut TransformComponent| {
                t.x = x;
                t.y = y;
                t.z = z;
                t.rot_x = rx;
                t.rot_y = ry;
                t.rot_z = rz;
                t.scale_x = sx;
                t.scale_y = sy;
                t.scale_z = sz;
            });
        }
    }
}

#[pymethods]
impl TransformPy {
    #[new]
    #[pyo3(signature = (x=0.0, y=0.0, z=0.0, rot_x=0.0, rot_y=0.0, rot_z=0.0, scale=1.0, scale_x=None, scale_y=None, scale_z=None))]
    pub fn new(
        x: f32,
        y: f32,
        z: f32,
        rot_x: f32,
        rot_y: f32,
        rot_z: f32,
        scale: f32,
        scale_x: Option<f32>,
        scale_y: Option<f32>,
        scale_z: Option<f32>,
    ) -> Self {
        Self {
            entity_id: None,
            x, y, z,
            rot_x, rot_y, rot_z,
            scale_x: scale_x.unwrap_or(scale),
            scale_y: scale_y.unwrap_or(scale),
            scale_z: scale_z.unwrap_or(scale),
        }
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

    /// Uniform scale getter — returns scale_x (use scale_x/y/z for per-axis).
    #[getter]
    fn scale(&self) -> f32 {
        self.scale_x
    }
    /// Uniform scale setter — sets all three axes to the same value.
    #[setter]
    fn set_scale(&mut self, val: f32) {
        self.scale_x = val;
        self.scale_y = val;
        self.scale_z = val;
        self.write_back();
    }

    #[getter]
    fn scale_x(&self) -> f32 {
        self.scale_x
    }
    #[setter]
    fn set_scale_x(&mut self, val: f32) {
        self.scale_x = val;
        self.write_back();
    }

    #[getter]
    fn scale_y(&self) -> f32 {
        self.scale_y
    }
    #[setter]
    fn set_scale_y(&mut self, val: f32) {
        self.scale_y = val;
        self.write_back();
    }

    #[getter]
    fn scale_z(&self) -> f32 {
        self.scale_z
    }
    #[setter]
    fn set_scale_z(&mut self, val: f32) {
        self.scale_z = val;
        self.write_back();
    }

    fn __repr__(&self) -> String {
        format!(
            "Transform(x={}, y={}, z={}, rot_x={}, rot_y={}, rot_z={}, scale_x={}, scale_y={}, scale_z={})",
            self.x, self.y, self.z, self.rot_x, self.rot_y, self.rot_z,
            self.scale_x, self.scale_y, self.scale_z
        )
    }
}

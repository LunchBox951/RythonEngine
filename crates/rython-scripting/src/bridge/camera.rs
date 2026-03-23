use pyo3::prelude::*;

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
    /// World-space look-at target (set by set_look_at or derived from set_rotation).
    pub target_x: f32,
    pub target_y: f32,
    pub target_z: f32,
}

#[pymethods]
impl CameraPy {
    #[new]
    pub fn new() -> Self {
        // Default position (0, 0, -10) looking at origin (0, 0, 0).
        Self {
            pos_x: 0.0,
            pos_y: 0.0,
            pos_z: -10.0,
            rot_pitch: 0.0,
            rot_yaw: 0.0,
            rot_roll: 0.0,
            target_x: 0.0,
            target_y: 0.0,
            target_z: 0.0,
        }
    }

    /// Set the camera world-space position.
    fn set_position(&mut self, x: f32, y: f32, z: f32) {
        self.pos_x = x;
        self.pos_y = y;
        self.pos_z = z;
    }

    /// Set the camera orientation as Euler angles (pitch, yaw, roll) in radians.
    /// Also updates the stored target as a unit-distance point in the look direction.
    fn set_rotation(&mut self, pitch: f32, yaw: f32, roll: f32) {
        self.rot_pitch = pitch;
        self.rot_yaw = yaw;
        self.rot_roll = roll;
        self.target_x = self.pos_x + yaw.sin() * pitch.cos();
        self.target_y = self.pos_y - pitch.sin();
        self.target_z = self.pos_z + yaw.cos() * pitch.cos();
    }

    /// Point the camera at a world-space target from its current position.
    /// Stores the exact target and derives pitch/yaw from it.
    fn set_look_at(&mut self, target_x: f32, target_y: f32, target_z: f32) {
        self.target_x = target_x;
        self.target_y = target_y;
        self.target_z = target_z;
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

    #[getter]
    fn target_x(&self) -> f32 {
        self.target_x
    }
    #[getter]
    fn target_y(&self) -> f32 {
        self.target_y
    }
    #[getter]
    fn target_z(&self) -> f32 {
        self.target_z
    }

    fn __repr__(&self) -> String {
        format!(
            "Camera(pos=({}, {}, {}), pitch={:.3}, yaw={:.3}, roll={:.3})",
            self.pos_x, self.pos_y, self.pos_z, self.rot_pitch, self.rot_yaw, self.rot_roll
        )
    }
}

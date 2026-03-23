use rython_core::math::{Mat4, Vec3};

/// 3D camera providing view and projection matrices for Phase 3 rendering.
///
/// Uses right-handed coordinate system with zero-to-one depth range (wgpu convention).
pub struct Camera {
    pub position: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov_degrees: f32,
    pub near: f32,
    pub far: f32,
    /// Viewport aspect ratio (width / height).
    pub aspect: f32,
}

impl Camera {
    pub fn new() -> Self {
        Self {
            position: Vec3::new(0.0, 0.0, -10.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fov_degrees: 90.0,
            near: 0.1,
            far: 1000.0,
            aspect: 16.0 / 9.0,
        }
    }

    pub fn set_position(&mut self, x: f32, y: f32, z: f32) {
        self.position = Vec3::new(x, y, z);
    }

    pub fn set_look_at(&mut self, x: f32, y: f32, z: f32) {
        self.target = Vec3::new(x, y, z);
    }

    pub fn set_fov(&mut self, degrees: f32) {
        self.fov_degrees = degrees;
    }

    /// Right-handed view matrix: transforms world space into camera space.
    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.position, self.target, self.up)
    }

    /// Right-handed perspective projection with zero-to-one depth (wgpu NDC range [0, 1]).
    ///
    /// Points at z = near map to NDC z = 0; points at z = far map to NDC z = 1.
    pub fn projection_matrix(&self) -> Mat4 {
        Mat4::perspective_rh(
            self.fov_degrees.to_radians(),
            self.aspect,
            self.near,
            self.far,
        )
    }

    /// Camera forward vector in world space (unit length).
    pub fn forward(&self) -> Vec3 {
        (self.target - self.position).normalize()
    }

    /// Combined view-projection matrix for use in mesh shaders.
    pub fn view_projection(&self) -> Mat4 {
        self.projection_matrix() * self.view_matrix()
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self::new()
    }
}

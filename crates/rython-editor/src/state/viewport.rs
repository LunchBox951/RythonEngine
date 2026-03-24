use rython_renderer::Camera;

use crate::viewport::camera_controller::CameraController;
use crate::viewport::gizmo::{GizmoDrag, GizmoMode, GizmoSpace};

/// All state needed to drive the 3D viewport.
pub struct ViewportState {
    pub camera: Camera,
    pub camera_controller: CameraController,
    pub gizmo_mode: GizmoMode,
    pub gizmo_space: GizmoSpace,
    pub show_grid: bool,
    pub show_wireframe: bool,
    /// Non-None while the user is actively dragging a gizmo handle.
    pub active_drag: Option<GizmoDrag>,
}

impl ViewportState {
    pub fn new() -> Self {
        let mut camera = Camera::new();
        camera.set_position(0.0, 5.0, -10.0);
        camera.set_look_at(0.0, 0.0, 0.0);
        camera.fov_degrees = 60.0;

        Self {
            camera,
            camera_controller: CameraController::new(),
            gizmo_mode: GizmoMode::Translate,
            gizmo_space: GizmoSpace::World,
            show_grid: true,
            show_wireframe: false,
            active_drag: None,
        }
    }
}

impl Default for ViewportState {
    fn default() -> Self {
        Self::new()
    }
}

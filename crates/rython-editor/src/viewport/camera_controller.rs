use egui::PointerButton;
use glam::Vec3;
use rython_renderer::Camera;

/// Orbit camera controller for the editor viewport.
///
/// Maintains spherical coordinates (yaw, pitch, distance) around a look-at
/// target. Mouse input drives orbit, pan, and zoom.
pub struct CameraController {
    /// World-space look-at point (orbit center).
    pub target: Vec3,
    /// Distance from target.
    pub distance: f32,
    /// Horizontal angle in radians.
    pub yaw: f32,
    /// Vertical angle in radians, clamped to avoid gimbal flip.
    pub pitch: f32,
}

impl CameraController {
    pub fn new() -> Self {
        Self {
            target: Vec3::ZERO,
            distance: 12.0,
            yaw: 0.0,
            pitch: 0.3, // slightly above horizon
        }
    }

    /// Update camera from egui response (drag, scroll) and write into `camera`.
    pub fn update(&mut self, response: &egui::Response, camera: &mut Camera) {
        // Scroll wheel → zoom
        if response.hovered() {
            let scroll = response.ctx.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.distance = (self.distance - scroll * 0.05).clamp(0.5, 500.0);
            }
        }

        // Middle mouse drag OR Alt + Left drag → orbit
        let is_orbit = response.dragged_by(PointerButton::Middle)
            || (response.dragged_by(PointerButton::Primary)
                && response.ctx.input(|i| i.modifiers.alt));

        if is_orbit {
            let delta = response.drag_delta();
            self.yaw -= delta.x * 0.005;
            self.pitch = (self.pitch - delta.y * 0.005)
                .clamp(-89.0_f32.to_radians(), 89.0_f32.to_radians());
        }

        // Right mouse drag OR Shift + Middle drag → pan
        let is_pan = response.dragged_by(PointerButton::Secondary)
            || (response.dragged_by(PointerButton::Middle)
                && response.ctx.input(|i| i.modifiers.shift));

        if is_pan {
            let delta = response.drag_delta();
            let pan_scale = self.distance * 0.001;
            // Camera right vector in XZ plane
            let right = Vec3::new(self.yaw.cos(), 0.0, -self.yaw.sin());
            self.target += right * (-delta.x * pan_scale) + Vec3::Y * (delta.y * pan_scale);
        }

        // Compute camera position from spherical coordinates
        let cos_pitch = self.pitch.cos();
        let sin_pitch = self.pitch.sin();
        let pos = self.target
            + Vec3::new(
                self.distance * cos_pitch * self.yaw.sin(),
                self.distance * sin_pitch,
                self.distance * cos_pitch * self.yaw.cos(),
            );

        camera.set_position(pos.x, pos.y, pos.z);
        camera.set_look_at(self.target.x, self.target.y, self.target.z);
    }
}

impl Default for CameraController {
    fn default() -> Self {
        Self::new()
    }
}

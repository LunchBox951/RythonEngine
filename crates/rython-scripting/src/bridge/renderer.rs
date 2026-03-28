use pyo3::prelude::*;
use rython_renderer::command::{Color, DrawCommand, DrawText};

use super::{draw_commands_store, scene_settings_store};

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

    /// Set the framebuffer clear color (linear RGBA, each component [0, 1]).
    ///
    /// Values outside [0, 1] are clamped with a warning.
    #[pyo3(signature = (r, g, b, a = 1.0))]
    fn set_clear_color(&self, r: f32, g: f32, b: f32, a: f32) {
        let clamp_warn = |v: f32, name: &str| {
            if v < 0.0 || v > 1.0 {
                log::warn!("set_clear_color: {} out of range ({}) — clamped to [0, 1]", name, v);
            }
            v.clamp(0.0, 1.0)
        };
        scene_settings_store().lock().clear_color = [
            clamp_warn(r, "r"),
            clamp_warn(g, "g"),
            clamp_warn(b, "b"),
            clamp_warn(a, "a"),
        ];
    }

    /// Set the directional light world-space direction. Normalized before storing.
    ///
    /// Zero vector falls back to (0, 1, 0) with a warning.
    fn set_light_direction(&self, x: f32, y: f32, z: f32) {
        let len = (x * x + y * y + z * z).sqrt();
        let dir = if len > 1e-6 {
            [x / len, y / len, z / len]
        } else {
            log::warn!("set_light_direction: zero vector provided — falling back to (0, 1, 0)");
            [0.0, 1.0, 0.0]
        };
        scene_settings_store().lock().light_direction = dir;
    }

    /// Set the directional light RGB color (linear [0, 1]).
    fn set_light_color(&self, r: f32, g: f32, b: f32) {
        scene_settings_store().lock().light_color = [r, g, b];
    }

    /// Set the directional light intensity multiplier.
    fn set_light_intensity(&self, intensity: f32) {
        scene_settings_store().lock().light_intensity = intensity;
    }

    fn __repr__(&self) -> String {
        "rython.renderer".to_string()
    }
}

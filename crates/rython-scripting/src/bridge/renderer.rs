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
    #[allow(clippy::too_many_arguments)]
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
        draw_commands_store()
            .lock()
            .push(DrawCommand::Text(DrawText {
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
            if !(0.0..=1.0).contains(&v) {
                log::warn!(
                    "set_clear_color: {} out of range ({}) — clamped to [0, 1]",
                    name,
                    v
                );
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

    /// Set the scene-wide ambient light color and intensity multiplier (linear RGB).
    ///
    /// The ambient contributes to all surfaces regardless of light direction.
    #[pyo3(signature = (r = 0.1, g = 0.1, b = 0.1, intensity = 1.0))]
    fn set_ambient_light(&self, r: f32, g: f32, b: f32, intensity: f32) {
        let mut s = scene_settings_store().lock();
        s.ambient_color = [r, g, b];
        s.ambient_intensity = intensity;
    }

    // ── §3 Shadow mapping API ─────────────────────────────────────────────────

    /// Enable or disable shadow casting from the primary directional light.
    fn set_shadow_enabled(&self, enabled: bool) {
        scene_settings_store().lock().shadow.enabled = enabled;
    }

    /// Set the shadow map resolution in pixels (square).
    ///
    /// Accepted values: 512, 1024, 2048, 4096.
    /// Invalid sizes are clamped to the nearest valid value with a warning.
    fn set_shadow_map_size(&self, size: u32) {
        let clamped = match size {
            0..=512 => 512,
            513..=1024 => 1024,
            1025..=2048 => 2048,
            _ => 4096,
        };
        if clamped != size {
            log::warn!(
                "set_shadow_map_size: {} is not a valid shadow map size — clamped to {}",
                size,
                clamped
            );
        }
        scene_settings_store().lock().shadow.map_size = clamped;
    }

    /// Set the shadow depth bias (prevents shadow acne). Default: 0.005.
    fn set_shadow_bias(&self, bias: f32) {
        scene_settings_store().lock().shadow.bias = bias;
    }

    /// Set the PCF sample count: 1 = no filtering, ≥4 = 3×3 kernel. Default: 4.
    fn set_shadow_pcf(&self, samples: u32) {
        scene_settings_store().lock().shadow.pcf_samples = samples;
    }

    fn __repr__(&self) -> String {
        "rython.renderer".to_string()
    }
}

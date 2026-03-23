use pyo3::prelude::*;
use rython_renderer::command::{Color, DrawCommand, DrawText};

use super::draw_commands_store;

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

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use rython_renderer::DrawCommand;
use rython_ui::{LayoutDir, UIManager};

static UI_MANAGER: OnceLock<Arc<Mutex<UIManager>>> = OnceLock::new();

fn ui_store() -> &'static Arc<Mutex<UIManager>> {
    UI_MANAGER.get_or_init(|| Arc::new(Mutex::new(UIManager::with_default_theme())))
}

/// Share an engine-created UIManager with the Python bridge.
/// Must be called before ensure_rython_module().
pub fn set_active_ui(manager: Arc<Mutex<UIManager>>) {
    let _ = UI_MANAGER.set(manager);
}

/// Build and return all draw commands for the current UI widget tree.
/// Called each frame from main.rs after Python callbacks run.
pub fn drain_ui_draw_commands() -> Vec<DrawCommand> {
    ui_store().lock().build_draw_commands()
}

// ─── UIBridge ─────────────────────────────────────────────────────────────────

#[pyclass(name = "UIBridge")]
pub struct UIBridge {}

#[pymethods]
impl UIBridge {
    /// Create a label widget. Returns the widget ID.
    fn create_label(&self, text: &str, x: f32, y: f32, w: f32, h: f32) -> u64 {
        ui_store().lock().create_label(text, x, y, w, h)
    }

    /// Create a button widget. Returns the widget ID.
    fn create_button(&self, text: &str, x: f32, y: f32, w: f32, h: f32) -> u64 {
        ui_store().lock().create_button(text, x, y, w, h)
    }

    /// Create a panel container widget. Returns the widget ID.
    fn create_panel(&self, x: f32, y: f32, w: f32, h: f32) -> u64 {
        ui_store().lock().create_panel(x, y, w, h)
    }

    /// Create a text input widget. Returns the widget ID.
    fn create_text_input(&self, placeholder: &str, x: f32, y: f32, w: f32, h: f32) -> u64 {
        ui_store().lock().create_text_input(placeholder, x, y, w, h)
    }

    /// Attach `child` as a child of `parent`.
    fn add_child(&self, parent: u64, child: u64) {
        ui_store().lock().add_child(parent, child);
    }

    /// Set the layout direction for a container widget.
    /// `direction` must be "none", "vertical", or "horizontal".
    fn set_layout(
        &self,
        id: u64,
        direction: &str,
        spacing: f32,
        padding: f32,
    ) -> PyResult<()> {
        let dir = match direction {
            "none" => LayoutDir::None,
            "vertical" => LayoutDir::Vertical,
            "horizontal" => LayoutDir::Horizontal,
            other => {
                return Err(PyErr::new::<PyRuntimeError, _>(format!(
                    "Unknown layout direction: {other}. Use 'none', 'vertical', or 'horizontal'."
                )));
            }
        };
        ui_store().lock().set_layout(id, dir, spacing, padding);
        Ok(())
    }

    /// Make the widget visible.
    fn show(&self, id: u64) {
        ui_store().lock().show(id);
    }

    /// Hide the widget.
    fn hide(&self, id: u64) {
        ui_store().lock().hide(id);
    }

    /// True if the widget and all its ancestors are visible.
    fn is_visible(&self, id: u64) -> bool {
        ui_store().lock().is_visible(id)
    }

    /// Set the display text of a widget (label text, button label, text input value).
    fn set_text(&self, id: u64, text: &str) {
        ui_store().lock().set_text(id, text);
    }

    /// Register a Python callable as the click handler for a button widget.
    /// The callback is called with no arguments when the button is clicked.
    fn on_click(&self, id: u64, callback: Py<PyAny>) {
        let cb = Arc::new(move || {
            Python::attach(|py| {
                if let Err(e) = callback.bind(py).call0() {
                    e.print_and_set_sys_last_vars(py);
                }
            });
        });
        ui_store().lock().set_on_click(id, cb);
    }

    /// Apply a partial theme override. Unset fields keep their current value.
    /// Colors are (r, g, b) tuples with values 0–255.
    #[pyo3(signature = (*, button_color=None, text_color=None, panel_color=None, border_color=None, font_size=None))]
    fn set_theme(
        &self,
        button_color: Option<(u8, u8, u8)>,
        text_color: Option<(u8, u8, u8)>,
        panel_color: Option<(u8, u8, u8)>,
        border_color: Option<(u8, u8, u8)>,
        font_size: Option<u32>,
    ) {
        let mut manager = ui_store().lock();
        let mut theme = manager.theme.clone();
        if let Some((r, g, b)) = button_color {
            theme.button_color = rython_renderer::Color::rgb(r, g, b);
        }
        if let Some((r, g, b)) = text_color {
            theme.text_color = rython_renderer::Color::rgb(r, g, b);
        }
        if let Some((r, g, b)) = panel_color {
            theme.panel_color = rython_renderer::Color::rgb(r, g, b);
        }
        if let Some((r, g, b)) = border_color {
            theme.border_color = rython_renderer::Color::rgb(r, g, b);
        }
        if let Some(sz) = font_size {
            theme.font_size = sz;
        }
        manager.set_theme(theme);
    }

    /// Load a UI layout from an editor JSON file (additive — does not clear existing widgets).
    /// Applies the file's theme, creates all widgets with fresh runtime IDs, sets all visual
    /// properties (colors, fonts, borders, layout, visibility), and wires parent-child.
    /// Returns a dict mapping widget name → runtime widget ID.
    fn load_layout(&self, path: &str) -> PyResult<HashMap<String, u64>> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            PyErr::new::<PyRuntimeError, _>(format!("load_layout: cannot read '{path}': {e}"))
        })?;
        let data: serde_json::Value = serde_json::from_str(&content).map_err(|e| {
            PyErr::new::<PyRuntimeError, _>(format!("load_layout: invalid JSON in '{path}': {e}"))
        })?;
        Ok(ui_store().lock().load_layout(&data))
    }

    fn __repr__(&self) -> String {
        "rython.ui".to_string()
    }
}

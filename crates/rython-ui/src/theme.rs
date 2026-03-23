use rython_renderer::Color;

/// Centralized styling for all widgets. Widgets without explicit colors/fonts
/// inherit from the active theme.
#[derive(Debug, Clone)]
pub struct Theme {
    pub font_id: String,
    pub font_size: u32,
    pub text_color: Color,
    pub button_color: Color,
    pub button_hover_color: Color,
    pub button_active_color: Color,
    pub panel_color: Color,
    pub border_color: Color,
    pub border_width: f32,
    /// Default inner padding for containers.
    pub padding: f32,
    /// Default gap between stacked children.
    pub spacing: f32,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            font_id: "default".to_string(),
            font_size: 18,
            text_color: Color::rgb(220, 220, 220),
            button_color: Color::rgb(50, 50, 70),
            button_hover_color: Color::rgb(70, 70, 100),
            button_active_color: Color::rgb(40, 40, 55),
            panel_color: Color::new(20, 20, 30, 200),
            border_color: Color::rgb(100, 100, 120),
            border_width: 1.0,
            padding: 0.01,
            spacing: 0.01,
        }
    }
}

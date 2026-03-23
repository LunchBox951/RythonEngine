use rython_renderer::Color;
use std::sync::Arc;

/// Unique identifier for a widget.
pub type WidgetId = u64;

/// The kind of widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetKind {
    Label,
    Button,
    TextInput,
    Panel,
    ScrollView,
}

/// Interaction state for a widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetState {
    Normal,
    Hover,
    Active,
    Disabled,
}

/// Layout direction for a container widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutDir {
    /// Children use their own x/y offsets relative to the parent.
    None,
    /// Children are stacked top-to-bottom.
    Vertical,
    /// Children are stacked left-to-right.
    Horizontal,
}

/// A single UI widget node.
#[derive(Clone)]
pub struct Widget {
    pub id: WidgetId,
    pub kind: WidgetKind,
    /// Display text (label text, button label, text input content).
    pub text: String,
    /// Placeholder text for TextInput.
    pub placeholder: String,
    /// Relative x position within parent (absolute for root widgets).
    pub x: f32,
    /// Relative y position within parent (absolute for root widgets).
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Computed absolute x after layout pass.
    pub abs_x: f32,
    /// Computed absolute y after layout pass.
    pub abs_y: f32,
    /// Background / fill color. `None` = inherit from theme.
    pub color: Option<Color>,
    /// Text color. `None` = inherit from theme.
    pub text_color: Option<Color>,
    /// Font asset ID. `None` = inherit from theme.
    pub font_id: Option<String>,
    /// Font size. `None` = inherit from theme.
    pub font_size: Option<u32>,
    pub border_color: Option<Color>,
    pub border_width: f32,
    /// Alpha multiplier [0.0, 1.0].
    pub alpha: f32,
    /// Base z-depth for draw ordering. UI default is 100.0 to render above game world.
    pub z: f32,
    pub visible: bool,
    pub state: WidgetState,
    pub focused: bool,
    pub parent: Option<WidgetId>,
    pub children: Vec<WidgetId>,
    /// Layout direction for children.
    pub layout: LayoutDir,
    /// Gap between stacked children (Vertical/Horizontal layout).
    pub spacing: f32,
    /// Inner padding for container layouts.
    pub padding: f32,
    /// Scroll offset (ScrollView only).
    pub scroll_y: f32,
    pub on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    #[allow(clippy::type_complexity)]
    pub on_submit: Option<Arc<dyn Fn(&str) + Send + Sync>>,
}

impl Widget {
    pub fn new(id: WidgetId, kind: WidgetKind, x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            id,
            kind,
            text: String::new(),
            placeholder: String::new(),
            x,
            y,
            w,
            h,
            abs_x: x,
            abs_y: y,
            color: None,
            text_color: None,
            font_id: None,
            font_size: None,
            border_color: None,
            border_width: 0.0,
            alpha: 1.0,
            z: 100.0,
            visible: true,
            state: WidgetState::Normal,
            focused: false,
            parent: None,
            children: Vec::new(),
            layout: LayoutDir::None,
            spacing: 0.0,
            padding: 0.0,
            scroll_y: 0.0,
            on_click: None,
            on_submit: None,
        }
    }

    /// Whether the point (px, py) is inside this widget's absolute bounds.
    pub fn contains_point(&self, px: f32, py: f32) -> bool {
        px >= self.abs_x
            && px <= self.abs_x + self.w
            && py >= self.abs_y
            && py <= self.abs_y + self.h
    }
}

use std::collections::HashMap;
use std::sync::Arc;

use rython_core::{EngineError, SchedulerHandle};
use rython_modules::Module;
use rython_renderer::{Color, CommandQueue, DrawCommand, DrawRect, DrawText};

use crate::animator::{EasingFn, TweenDef, UIAnimator};
use crate::commands::UICmd;
use crate::theme::Theme;
use crate::widget::{LayoutDir, Widget, WidgetId, WidgetKind, WidgetState};

/// Central UI system: widget tree, layout, theming, animation, input routing.
pub struct UIManager {
    widgets: HashMap<WidgetId, Widget>,
    /// Root widgets in insertion order (for rendering iteration).
    root_order: Vec<WidgetId>,
    next_id: WidgetId,
    pub theme: Theme,
    animator: UIAnimator,
    cmd_queue: Vec<UICmd>,
    tab_order: Vec<WidgetId>,
    focused: Option<WidgetId>,
    cursor_visible: bool,
}

impl UIManager {
    pub fn new(theme: Theme) -> Self {
        Self {
            widgets: HashMap::new(),
            root_order: Vec::new(),
            next_id: 1,
            theme,
            animator: UIAnimator::new(),
            cmd_queue: Vec::new(),
            tab_order: Vec::new(),
            focused: None,
            cursor_visible: false,
        }
    }

    pub fn with_default_theme() -> Self {
        Self::new(Theme::default())
    }

    // ─── Internal helpers ─────────────────────────────────────────────────────

    fn alloc_id(&mut self) -> WidgetId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn insert_root(&mut self, widget: Widget) -> WidgetId {
        let id = widget.id;
        self.root_order.push(id);
        self.widgets.insert(id, widget);
        id
    }

    fn insert_child(&mut self, parent_id: WidgetId, mut widget: Widget) -> WidgetId {
        let id = widget.id;
        widget.parent = Some(parent_id);
        // Children always render/hit-test above their parent container
        widget.z = self.widgets[&parent_id].z + 1.0;
        self.widgets.insert(id, widget);
        self.widgets.get_mut(&parent_id).unwrap().children.push(id);
        id
    }

    // ─── Widget creation ──────────────────────────────────────────────────────

    /// Create a Label at absolute position (x, y) with given size. No explicit text color.
    pub fn create_label(&mut self, text: &str, x: f32, y: f32, w: f32, h: f32) -> WidgetId {
        let id = self.alloc_id();
        let mut widget = Widget::new(id, WidgetKind::Label, x, y, w, h);
        widget.text = text.to_string();
        self.insert_root(widget)
    }

    /// Create a Label with an explicit text color (not theme-inherited).
    pub fn create_label_colored(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: Color,
    ) -> WidgetId {
        let id = self.alloc_id();
        let mut widget = Widget::new(id, WidgetKind::Label, x, y, w, h);
        widget.text = text.to_string();
        widget.text_color = Some(color);
        self.insert_root(widget)
    }

    /// Create a Panel at absolute position.
    pub fn create_panel(&mut self, x: f32, y: f32, w: f32, h: f32) -> WidgetId {
        let id = self.alloc_id();
        let widget = Widget::new(id, WidgetKind::Panel, x, y, w, h);
        self.insert_root(widget)
    }

    /// Create a Button at absolute position. No explicit background color.
    pub fn create_button(&mut self, text: &str, x: f32, y: f32, w: f32, h: f32) -> WidgetId {
        let id = self.alloc_id();
        let mut widget = Widget::new(id, WidgetKind::Button, x, y, w, h);
        widget.text = text.to_string();
        self.insert_root(widget)
    }

    /// Create a Button with an explicit background color (overrides theme).
    pub fn create_button_colored(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: Color,
    ) -> WidgetId {
        let id = self.alloc_id();
        let mut widget = Widget::new(id, WidgetKind::Button, x, y, w, h);
        widget.text = text.to_string();
        widget.color = Some(color);
        self.insert_root(widget)
    }

    /// Create a TextInput at absolute position.
    pub fn create_text_input(
        &mut self,
        placeholder: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    ) -> WidgetId {
        let id = self.alloc_id();
        let mut widget = Widget::new(id, WidgetKind::TextInput, x, y, w, h);
        widget.placeholder = placeholder.to_string();
        self.insert_root(widget)
    }

    /// Create a ScrollView at absolute position.
    pub fn create_scroll_view(&mut self, x: f32, y: f32, w: f32, h: f32) -> WidgetId {
        let id = self.alloc_id();
        let widget = Widget::new(id, WidgetKind::ScrollView, x, y, w, h);
        self.insert_root(widget)
    }

    // ─── Widget tree ──────────────────────────────────────────────────────────

    /// Attach `child` as a child of `parent`. The child is removed from root_order.
    pub fn add_child(&mut self, parent_id: WidgetId, child_id: WidgetId) {
        // Remove from root_order if it was there
        self.root_order.retain(|&id| id != child_id);
        let parent_z = self.widgets[&parent_id].z;
        // Children always render/hit-test above their parent container
        self.widgets.get_mut(&child_id).unwrap().z = parent_z + 1.0;
        self.widgets.get_mut(&child_id).unwrap().parent = Some(parent_id);
        self.widgets.get_mut(&parent_id).unwrap().children.push(child_id);
    }

    /// Create a Button as a direct child of `parent_id` with relative position.
    pub fn create_button_child(
        &mut self,
        text: &str,
        parent_id: WidgetId,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    ) -> WidgetId {
        let id = self.alloc_id();
        let mut widget = Widget::new(id, WidgetKind::Button, x, y, w, h);
        widget.text = text.to_string();
        self.insert_child(parent_id, widget)
    }

    // ─── Visibility ───────────────────────────────────────────────────────────

    /// Show a widget (sets its own visible flag; parent cascade applies on query).
    pub fn show(&mut self, id: WidgetId) {
        if let Some(w) = self.widgets.get_mut(&id) {
            w.visible = true;
        }
    }

    /// Hide a widget.
    pub fn hide(&mut self, id: WidgetId) {
        if let Some(w) = self.widgets.get_mut(&id) {
            w.visible = false;
        }
    }

    /// True if the widget and all its ancestors are visible.
    pub fn is_visible(&self, id: WidgetId) -> bool {
        let widget = match self.widgets.get(&id) {
            Some(w) => w,
            None => return false,
        };
        if !widget.visible {
            return false;
        }
        match widget.parent {
            Some(parent_id) => self.is_visible(parent_id),
            None => true,
        }
    }

    // ─── Layout ───────────────────────────────────────────────────────────────

    /// Configure layout direction, spacing, and padding for a container widget.
    pub fn set_layout(
        &mut self,
        id: WidgetId,
        dir: LayoutDir,
        spacing: f32,
        padding: f32,
    ) {
        let w = self.widgets.get_mut(&id).unwrap();
        w.layout = dir;
        w.spacing = spacing;
        w.padding = padding;
    }

    /// Recompute absolute positions for all widgets in the tree.
    pub fn compute_layout(&mut self) {
        let roots = self.root_order.clone();
        for id in roots {
            // Root widgets: abs position = their own x/y
            {
                let w = self.widgets.get_mut(&id).unwrap();
                w.abs_x = w.x;
                w.abs_y = w.y;
            }
            self.layout_children(id);
        }
    }

    fn layout_children(&mut self, id: WidgetId) {
        let (abs_x, abs_y, layout, spacing, padding, children) = {
            let w = &self.widgets[&id];
            (w.abs_x, w.abs_y, w.layout, w.spacing, w.padding, w.children.clone())
        };

        match layout {
            LayoutDir::None => {
                for child_id in children {
                    let (cx, cy) = {
                        let c = &self.widgets[&child_id];
                        (c.x, c.y)
                    };
                    {
                        let c = self.widgets.get_mut(&child_id).unwrap();
                        c.abs_x = abs_x + cx;
                        c.abs_y = abs_y + cy;
                    }
                    self.layout_children(child_id);
                }
            }
            LayoutDir::Vertical => {
                let mut cursor_y = abs_y + padding;
                for child_id in children {
                    let child_h = self.widgets[&child_id].h;
                    {
                        let c = self.widgets.get_mut(&child_id).unwrap();
                        c.abs_x = abs_x + padding;
                        c.abs_y = cursor_y;
                    }
                    self.layout_children(child_id);
                    cursor_y += child_h + spacing;
                }
            }
            LayoutDir::Horizontal => {
                let mut cursor_x = abs_x + padding;
                for child_id in children {
                    let child_w = self.widgets[&child_id].w;
                    {
                        let c = self.widgets.get_mut(&child_id).unwrap();
                        c.abs_x = cursor_x;
                        c.abs_y = abs_y + padding;
                    }
                    self.layout_children(child_id);
                    cursor_x += child_w + spacing;
                }
            }
        }
    }

    // ─── Theme ────────────────────────────────────────────────────────────────

    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

    /// Effective background/fill color for a widget (explicit or theme default).
    pub fn effective_color(&self, id: WidgetId) -> Color {
        let w = &self.widgets[&id];
        w.color.unwrap_or_else(|| match w.kind {
            WidgetKind::Button | WidgetKind::TextInput => self.theme.button_color,
            WidgetKind::Panel | WidgetKind::ScrollView => self.theme.panel_color,
            WidgetKind::Label => Color::rgb(0, 0, 0),
        })
    }

    /// Effective text color for a widget (explicit or theme default).
    pub fn effective_text_color(&self, id: WidgetId) -> Color {
        self.widgets[&id].text_color.unwrap_or(self.theme.text_color)
    }

    // ─── Animation ────────────────────────────────────────────────────────────

    /// Start a property tween on a widget.
    pub fn start_tween(
        &mut self,
        widget_id: WidgetId,
        property: &str,
        from: f32,
        to: f32,
        duration: f32,
        easing: EasingFn,
    ) {
        self.animator.start_tween(widget_id, property, from, to, duration, easing);
    }

    /// Start a sequential animation chain on a widget.
    pub fn animate_sequence(&mut self, widget_id: WidgetId, steps: Vec<TweenDef>) {
        self.animator.start_sequence(widget_id, steps);
    }

    /// Advance all animations by dt seconds and apply property updates to widgets.
    pub fn tick(&mut self, dt: f32) {
        let updates = self.animator.tick(dt);
        for (widget_id, property, value) in updates {
            if let Some(w) = self.widgets.get_mut(&widget_id) {
                match property.as_str() {
                    "alpha" => w.alpha = value.clamp(0.0, 1.0),
                    "position_x" => {
                        w.x = value;
                        w.abs_x = value;
                    }
                    "position_y" => {
                        w.y = value;
                        w.abs_y = value;
                    }
                    "w" => w.w = value,
                    "h" => w.h = value,
                    _ => {}
                }
            }
        }
    }

    /// True if the widget has any active animations.
    pub fn has_active_animation(&self, id: WidgetId) -> bool {
        self.animator.has_active_for(id)
    }

    // ─── Input routing ────────────────────────────────────────────────────────

    /// Register a click callback on a button.
    pub fn set_on_click(&mut self, id: WidgetId, callback: Arc<dyn Fn() + Send + Sync>) {
        if let Some(w) = self.widgets.get_mut(&id) {
            w.on_click = Some(callback);
        }
    }

    /// Process a mouse click at (x, y). Returns true if the event was consumed by a widget.
    pub fn on_mouse_click(&mut self, x: f32, y: f32) -> bool {
        // Collect candidates first to avoid borrow conflicts with callback invocation
        let mut candidates: Vec<(WidgetId, f32)> = self
            .widgets
            .values()
            .filter(|w| self.is_visible(w.id) && w.contains_point(x, y))
            .map(|w| (w.id, w.z))
            .collect();

        // Highest z = topmost widget receives the click
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        if let Some((widget_id, _)) = candidates.first().copied() {
            // Set focus
            if let Some(prev) = self.focused {
                if let Some(w) = self.widgets.get_mut(&prev) {
                    w.focused = false;
                }
            }
            self.focused = Some(widget_id);
            if let Some(w) = self.widgets.get_mut(&widget_id) {
                w.focused = true;
            }

            // Fire callback if present
            let callback = self.widgets[&widget_id].on_click.clone();
            if let Some(cb) = callback {
                cb();
            }
            true // consumed
        } else {
            false // not consumed
        }
    }

    /// Process mouse movement, updating hover states. Returns true if over any widget.
    pub fn on_mouse_move(&mut self, x: f32, y: f32) -> bool {
        let mut hit = false;
        let ids: Vec<WidgetId> = self.widgets.keys().copied().collect();
        for id in ids {
            let over = self.is_visible(id) && self.widgets[&id].contains_point(x, y);
            let w = self.widgets.get_mut(&id).unwrap();
            if over {
                if w.state == WidgetState::Normal {
                    w.state = WidgetState::Hover;
                }
                hit = true;
            } else if w.state == WidgetState::Hover {
                w.state = WidgetState::Normal;
            }
        }
        hit
    }

    /// Process a keyboard character event. Returns true if consumed by a focused widget.
    pub fn on_key_press(&mut self, ch: char) -> bool {
        if let Some(focused_id) = self.focused {
            if let Some(w) = self.widgets.get_mut(&focused_id) {
                if w.kind == WidgetKind::TextInput {
                    if ch == '\x08' {
                        // Backspace
                        w.text.pop();
                    } else if ch.is_control() {
                        return false;
                    } else {
                        w.text.push(ch);
                    }
                    return true;
                }
            }
        }
        false
    }

    /// Set keyboard focus to a specific widget.
    pub fn focus(&mut self, id: WidgetId) {
        if let Some(prev) = self.focused {
            if let Some(w) = self.widgets.get_mut(&prev) {
                w.focused = false;
            }
        }
        self.focused = Some(id);
        if let Some(w) = self.widgets.get_mut(&id) {
            w.focused = true;
        }
    }

    /// Set the tab navigation order.
    pub fn set_tab_order(&mut self, order: Vec<WidgetId>) {
        self.tab_order = order;
    }

    /// Move focus to the next widget in tab order. Returns the newly focused WidgetId.
    pub fn on_tab(&mut self) -> Option<WidgetId> {
        if self.tab_order.is_empty() {
            return None;
        }
        let current_pos = self
            .focused
            .and_then(|id| self.tab_order.iter().position(|&tid| tid == id))
            .unwrap_or(usize::MAX);

        let next_pos = if current_pos == usize::MAX {
            0
        } else {
            (current_pos + 1) % self.tab_order.len()
        };

        let next_id = self.tab_order[next_pos];
        self.focus(next_id);
        Some(next_id)
    }

    // ─── Command queue ────────────────────────────────────────────────────────

    pub fn queue_cmd(&mut self, cmd: UICmd) {
        self.cmd_queue.push(cmd);
    }

    /// Drain and apply all queued commands.
    pub fn drain_commands(&mut self) {
        let cmds: Vec<UICmd> = self.cmd_queue.drain(..).collect();
        for cmd in cmds {
            match cmd {
                UICmd::Show(id) => self.show(id),
                UICmd::Hide(id) => self.hide(id),
                UICmd::Focus(id) => self.focus(id),
                UICmd::SetCursorVisible(v) => self.cursor_visible = v,
            }
        }
    }

    // ─── Draw command emission ────────────────────────────────────────────────

    /// Build draw commands for all visible widgets. Z-values start at 100.0 to
    /// render above typical game-world draw commands.
    pub fn build_draw_commands(&self) -> Vec<DrawCommand> {
        let mut cmds = Vec::new();
        // Iterate roots in order; DFS to preserve hierarchy
        let roots = self.root_order.clone();
        for id in roots {
            self.collect_draw_commands(id, &mut cmds);
        }
        cmds
    }

    fn collect_draw_commands(&self, id: WidgetId, cmds: &mut Vec<DrawCommand>) {
        if !self.is_visible(id) {
            return;
        }
        let w = &self.widgets[&id];

        match w.kind {
            WidgetKind::Label => {
                let text_color = w.text_color.unwrap_or(self.theme.text_color);
                cmds.push(DrawCommand::Text(DrawText {
                    text: w.text.clone(),
                    font_id: w
                        .font_id
                        .clone()
                        .unwrap_or_else(|| self.theme.font_id.clone()),
                    x: w.abs_x,
                    y: w.abs_y,
                    color: text_color,
                    size: w.font_size.unwrap_or(self.theme.font_size),
                    z: w.z,
                }));
            }
            WidgetKind::Button => {
                let bg_color = match w.state {
                    WidgetState::Hover => w.color.unwrap_or(self.theme.button_hover_color),
                    WidgetState::Active => w.color.unwrap_or(self.theme.button_active_color),
                    _ => w.color.unwrap_or(self.theme.button_color),
                };
                cmds.push(DrawCommand::Rect(DrawRect {
                    x: w.abs_x,
                    y: w.abs_y,
                    w: w.w,
                    h: w.h,
                    color: bg_color,
                    border: w.border_color.or(Some(self.theme.border_color)),
                    border_width: if w.border_width > 0.0 {
                        w.border_width
                    } else {
                        self.theme.border_width
                    },
                    z: w.z,
                }));
                let text_color = w.text_color.unwrap_or(self.theme.text_color);
                cmds.push(DrawCommand::Text(DrawText {
                    text: w.text.clone(),
                    font_id: w
                        .font_id
                        .clone()
                        .unwrap_or_else(|| self.theme.font_id.clone()),
                    x: w.abs_x,
                    y: w.abs_y + w.h * 0.25,
                    color: text_color,
                    size: w.font_size.unwrap_or(self.theme.font_size),
                    z: w.z + 0.1,
                }));
            }
            WidgetKind::Panel | WidgetKind::ScrollView => {
                let bg_color = w.color.unwrap_or(self.theme.panel_color);
                cmds.push(DrawCommand::Rect(DrawRect {
                    x: w.abs_x,
                    y: w.abs_y,
                    w: w.w,
                    h: w.h,
                    color: bg_color,
                    border: w.border_color,
                    border_width: w.border_width,
                    z: w.z,
                }));
            }
            WidgetKind::TextInput => {
                let bg_color = w.color.unwrap_or(self.theme.button_color);
                cmds.push(DrawCommand::Rect(DrawRect {
                    x: w.abs_x,
                    y: w.abs_y,
                    w: w.w,
                    h: w.h,
                    color: bg_color,
                    border: Some(self.theme.border_color),
                    border_width: self.theme.border_width,
                    z: w.z,
                }));
                let display_text = if w.text.is_empty() {
                    w.placeholder.clone()
                } else {
                    w.text.clone()
                };
                let text_color = w.text_color.unwrap_or(self.theme.text_color);
                cmds.push(DrawCommand::Text(DrawText {
                    text: display_text,
                    font_id: w
                        .font_id
                        .clone()
                        .unwrap_or_else(|| self.theme.font_id.clone()),
                    x: w.abs_x,
                    y: w.abs_y + w.h * 0.25,
                    color: text_color,
                    size: w.font_size.unwrap_or(self.theme.font_size),
                    z: w.z + 0.1,
                }));
            }
        }

        // Recurse into children
        let children = w.children.clone();
        for child_id in children {
            self.collect_draw_commands(child_id, cmds);
        }
    }

    /// Push all visible widget draw commands into a renderer CommandQueue.
    pub fn flush_to_queue(&self, queue: &CommandQueue) {
        for cmd in self.build_draw_commands() {
            queue.push(cmd);
        }
    }

    // ─── Widget access ────────────────────────────────────────────────────────

    pub fn get_widget(&self, id: WidgetId) -> &Widget {
        &self.widgets[&id]
    }

    pub fn get_widget_mut(&mut self, id: WidgetId) -> &mut Widget {
        self.widgets.get_mut(&id).unwrap()
    }

    /// Set the display text of a widget. No-op if `id` does not exist.
    pub fn set_text(&mut self, id: WidgetId, text: impl Into<String>) {
        if let Some(w) = self.widgets.get_mut(&id) {
            w.text = text.into();
        }
    }
}

// ─── Module trait ─────────────────────────────────────────────────────────────

impl Module for UIManager {
    fn name(&self) -> &str {
        "ui"
    }

    fn dependencies(&self) -> Vec<String> {
        vec!["renderer".to_string(), "input".to_string()]
    }

    fn on_load(&mut self, _scheduler: &dyn SchedulerHandle) -> Result<(), EngineError> {
        Ok(())
    }

    fn on_unload(&mut self, _scheduler: &dyn SchedulerHandle) -> Result<(), EngineError> {
        self.widgets.clear();
        self.root_order.clear();
        self.cmd_queue.clear();
        self.focused = None;
        Ok(())
    }
}

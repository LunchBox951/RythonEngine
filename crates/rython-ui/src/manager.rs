use std::collections::HashMap;
use std::sync::Arc;

use rython_core::{EngineError, SchedulerHandle};
use rython_modules::Module;
use rython_renderer::{Color, CommandQueue, DrawCommand, DrawRect, DrawText};
use serde_json::{json, Value};

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
        let parent_z = self.widgets.get(&parent_id).map(|p| p.z).unwrap_or(0.0);
        // Children always render/hit-test above their parent container
        widget.z = parent_z + 1.0;
        self.widgets.insert(id, widget);
        if let Some(parent) = self.widgets.get_mut(&parent_id) {
            parent.children.push(id);
        }
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
    /// Returns an error if either id is unknown, or if the operation would create a
    /// parent/child cycle (including self-parenting).
    pub fn add_child(&mut self, parent_id: WidgetId, child_id: WidgetId) -> Result<(), String> {
        if parent_id == child_id {
            return Err(format!("add_child: widget {parent_id} cannot be its own child"));
        }
        if !self.widgets.contains_key(&parent_id) {
            return Err(format!("add_child: unknown parent widget id {parent_id}"));
        }
        if !self.widgets.contains_key(&child_id) {
            return Err(format!("add_child: unknown child widget id {child_id}"));
        }
        // Reject if parent is a descendant of child (would create a cycle).
        if self.is_ancestor(child_id, parent_id) {
            return Err(format!(
                "add_child: cycle detected — {parent_id} is already a descendant of {child_id}"
            ));
        }
        // Remove from previous parent's children list, if any.
        if let Some(prev_parent) = self.widgets.get(&child_id).and_then(|w| w.parent) {
            if let Some(p) = self.widgets.get_mut(&prev_parent) {
                p.children.retain(|&id| id != child_id);
            }
        }
        // Remove from root_order if it was there
        self.root_order.retain(|&id| id != child_id);
        let parent_z = self.widgets.get(&parent_id).map(|p| p.z).unwrap_or(0.0);
        if let Some(child) = self.widgets.get_mut(&child_id) {
            child.z = parent_z + 1.0;
            child.parent = Some(parent_id);
        }
        if let Some(parent) = self.widgets.get_mut(&parent_id) {
            parent.children.push(child_id);
        }
        Ok(())
    }

    /// True if `ancestor` is an ancestor of `node` (or equal to it).
    fn is_ancestor(&self, ancestor: WidgetId, mut node: WidgetId) -> bool {
        // Iterative walk with a depth cap to protect against corrupted trees.
        for _ in 0..1024 {
            if node == ancestor {
                return true;
            }
            match self.widgets.get(&node).and_then(|w| w.parent) {
                Some(parent) => node = parent,
                None => return false,
            }
        }
        false
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
    /// Iterative with a depth cap so a corrupted tree cannot stack-overflow.
    pub fn is_visible(&self, id: WidgetId) -> bool {
        let mut current = id;
        for _ in 0..Self::MAX_LAYOUT_DEPTH {
            let widget = match self.widgets.get(&current) {
                Some(w) => w,
                None => return false,
            };
            if !widget.visible {
                return false;
            }
            match widget.parent {
                Some(parent_id) => current = parent_id,
                None => return true,
            }
        }
        false
    }

    // ─── Layout ───────────────────────────────────────────────────────────────

    /// Configure layout direction, spacing, and padding for a container widget.
    /// No-op if `id` does not exist.
    pub fn set_layout(&mut self, id: WidgetId, dir: LayoutDir, spacing: f32, padding: f32) {
        if let Some(w) = self.widgets.get_mut(&id) {
            w.layout = dir;
            w.spacing = spacing;
            w.padding = padding;
        }
    }

    /// Recompute absolute positions for all widgets in the tree.
    pub fn compute_layout(&mut self) {
        // Iterate by index to avoid cloning root_order Vec.
        for i in 0..self.root_order.len() {
            let id = self.root_order[i];
            // Root widgets: abs position = their own x/y
            if let Some(w) = self.widgets.get_mut(&id) {
                w.abs_x = w.x;
                w.abs_y = w.y;
            } else {
                continue;
            }
            self.layout_children(id, 0);
        }
    }

    /// Maximum widget-tree depth before `layout_children` bails out.
    /// Protects against stack overflow from cyclic or pathologically deep trees.
    const MAX_LAYOUT_DEPTH: usize = 256;

    fn layout_children(&mut self, id: WidgetId, depth: usize) {
        if depth >= Self::MAX_LAYOUT_DEPTH {
            return;
        }
        let (abs_x, abs_y, layout, spacing, padding, children) = match self.widgets.get(&id) {
            Some(w) => (
                w.abs_x,
                w.abs_y,
                w.layout,
                w.spacing,
                w.padding,
                w.children.clone(),
            ),
            None => return,
        };

        match layout {
            LayoutDir::None => {
                for child_id in children {
                    let (cx, cy) = match self.widgets.get(&child_id) {
                        Some(c) => (c.x, c.y),
                        None => continue,
                    };
                    if let Some(c) = self.widgets.get_mut(&child_id) {
                        c.abs_x = abs_x + cx;
                        c.abs_y = abs_y + cy;
                    }
                    self.layout_children(child_id, depth + 1);
                }
            }
            LayoutDir::Vertical => {
                let mut cursor_y = abs_y + padding;
                for child_id in children {
                    let child_h = match self.widgets.get(&child_id) {
                        Some(c) => c.h,
                        None => continue,
                    };
                    if let Some(c) = self.widgets.get_mut(&child_id) {
                        c.abs_x = abs_x + padding;
                        c.abs_y = cursor_y;
                    }
                    self.layout_children(child_id, depth + 1);
                    cursor_y += child_h + spacing;
                }
            }
            LayoutDir::Horizontal => {
                let mut cursor_x = abs_x + padding;
                for child_id in children {
                    let child_w = match self.widgets.get(&child_id) {
                        Some(c) => c.w,
                        None => continue,
                    };
                    if let Some(c) = self.widgets.get_mut(&child_id) {
                        c.abs_x = cursor_x;
                        c.abs_y = abs_y + padding;
                    }
                    self.layout_children(child_id, depth + 1);
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
    /// Returns transparent black if `id` does not exist.
    pub fn effective_color(&self, id: WidgetId) -> Color {
        let Some(w) = self.widgets.get(&id) else {
            return Color::new(0, 0, 0, 0);
        };
        w.color.unwrap_or_else(|| match w.kind {
            WidgetKind::Button | WidgetKind::TextInput => self.theme.button_color,
            WidgetKind::Panel | WidgetKind::ScrollView => self.theme.panel_color,
            WidgetKind::Label => Color::rgb(0, 0, 0),
        })
    }

    /// Effective text color for a widget (explicit or theme default).
    /// Returns the theme default if `id` does not exist.
    pub fn effective_text_color(&self, id: WidgetId) -> Color {
        self.widgets
            .get(&id)
            .and_then(|w| w.text_color)
            .unwrap_or(self.theme.text_color)
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
        self.animator
            .start_tween(widget_id, property, from, to, duration, easing);
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

    /// Process a mouse click at (x, y). Returns the callback to invoke (if any) so the caller
    /// can fire it *after* releasing the UI lock, avoiding re-entrant deadlocks.
    /// Disabled widgets do not receive clicks.
    pub fn on_mouse_click(&mut self, x: f32, y: f32) -> Option<Arc<dyn Fn() + Send + Sync>> {
        // Collect candidates first to avoid borrow conflicts with callback invocation.
        // Filter out Disabled widgets so they never receive input events.
        let mut candidates: Vec<(WidgetId, f32)> = self
            .widgets
            .values()
            .filter(|w| {
                w.state != WidgetState::Disabled
                    && self.is_visible(w.id)
                    && w.contains_point(x, y)
            })
            .map(|w| (w.id, w.z))
            .collect();

        // Highest z = topmost widget receives the click.
        // Use `total_cmp` on the integer id as tie-break so coplanar widgets
        // have deterministic routing independent of HashMap iteration order.
        candidates.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
        });

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

            // Return callback — caller must invoke it after releasing the lock
            self.widgets
                .get(&widget_id)
                .and_then(|w| w.on_click.clone())
        } else {
            None
        }
    }

    /// Process mouse movement, updating hover states. Returns true if over any widget.
    pub fn on_mouse_move(&mut self, x: f32, y: f32) -> bool {
        let mut hit = false;
        // Two-pass approach: first gather (id, is_over) from immutable iteration,
        // then apply state changes. This avoids the intermediate Vec<WidgetId>
        // allocation that previously collected all keys before doing any work.
        let updates: Vec<(WidgetId, bool)> = self
            .widgets
            .iter()
            .map(|(&id, w)| {
                let over = self.is_visible(id) && w.contains_point(x, y);
                (id, over)
            })
            .collect();
        for (id, over) in updates {
            let Some(w) = self.widgets.get_mut(&id) else {
                continue;
            };
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
        // Iterate roots in order; DFS to preserve hierarchy.
        // Use index iteration to avoid cloning root_order Vec.
        for i in 0..self.root_order.len() {
            self.collect_draw_commands(self.root_order[i], 0, &mut cmds);
        }
        cmds
    }

    fn collect_draw_commands(&self, id: WidgetId, depth: usize, cmds: &mut Vec<DrawCommand>) {
        if depth >= Self::MAX_LAYOUT_DEPTH || !self.is_visible(id) {
            return;
        }
        let Some(w) = self.widgets.get(&id) else {
            return;
        };

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
            self.collect_draw_commands(child_id, depth + 1, cmds);
        }
    }

    /// Push all visible widget draw commands into a renderer CommandQueue.
    pub fn flush_to_queue(&self, queue: &CommandQueue) {
        for cmd in self.build_draw_commands() {
            queue.push(cmd);
        }
    }

    // ─── Widget access ────────────────────────────────────────────────────────

    /// Get a widget by id. Panics if unknown — callers crossing the Python boundary
    /// must use `try_get_widget` instead.
    pub fn get_widget(&self, id: WidgetId) -> &Widget {
        &self.widgets[&id]
    }

    /// Get a widget by id, returning `None` if unknown. Use this from bridge code.
    pub fn try_get_widget(&self, id: WidgetId) -> Option<&Widget> {
        self.widgets.get(&id)
    }

    /// Get a mutable widget by id. Panics if unknown — callers crossing the Python
    /// boundary must use `try_get_widget_mut` instead.
    pub fn get_widget_mut(&mut self, id: WidgetId) -> &mut Widget {
        self.widgets.get_mut(&id).unwrap()
    }

    /// Get a mutable widget by id, returning `None` if unknown.
    pub fn try_get_widget_mut(&mut self, id: WidgetId) -> Option<&mut Widget> {
        self.widgets.get_mut(&id)
    }

    /// Set the display text of a widget. No-op if `id` does not exist.
    pub fn set_text(&mut self, id: WidgetId, text: impl Into<String>) {
        if let Some(w) = self.widgets.get_mut(&id) {
            w.text = text.into();
        }
    }

    // ─── Editor accessors ─────────────────────────────────────────────────────

    /// Iterate over all widgets (unordered). Useful for editor panels.
    pub fn widgets(&self) -> impl Iterator<Item = &Widget> {
        self.widgets.values()
    }

    /// Root widget IDs in insertion order.
    pub fn root_order(&self) -> &[WidgetId] {
        &self.root_order
    }

    /// Number of widgets currently in the tree.
    pub fn widget_count(&self) -> usize {
        self.widgets.len()
    }

    /// Remove a widget and all its descendants. Updates parent's children list
    /// and prunes `focused` + `tab_order` if any of the removed ids were referenced.
    pub fn remove_widget(&mut self, id: WidgetId) {
        // Collect descendants first (DFS)
        let mut to_remove = Vec::new();
        self.collect_descendants(id, 0, &mut to_remove);
        to_remove.push(id);

        for rid in &to_remove {
            // Remove from parent's children list
            if let Some(w) = self.widgets.get(rid) {
                if let Some(parent_id) = w.parent {
                    if let Some(parent) = self.widgets.get_mut(&parent_id) {
                        parent.children.retain(|&c| c != *rid);
                    }
                }
            }
            self.widgets.remove(rid);
            self.root_order.retain(|&r| r != *rid);
            self.tab_order.retain(|&r| r != *rid);
            if self.focused == Some(*rid) {
                self.focused = None;
            }
        }
    }

    fn collect_descendants(&self, id: WidgetId, depth: usize, out: &mut Vec<WidgetId>) {
        if depth >= Self::MAX_LAYOUT_DEPTH {
            return;
        }
        if let Some(w) = self.widgets.get(&id) {
            for &child_id in &w.children {
                out.push(child_id);
                self.collect_descendants(child_id, depth + 1, out);
            }
        }
    }

    // ─── JSON serialization ───────────────────────────────────────────────────

    /// Serialize the widget tree and theme to a JSON value.
    pub fn save_json(&self) -> Value {
        let theme_val = json!({
            "font_id": self.theme.font_id,
            "font_size": self.theme.font_size,
            "text_color": color_to_json(self.theme.text_color),
            "button_color": color_to_json(self.theme.button_color),
            "button_hover_color": color_to_json(self.theme.button_hover_color),
            "button_active_color": color_to_json(self.theme.button_active_color),
            "panel_color": color_to_json(self.theme.panel_color),
            "border_color": color_to_json(self.theme.border_color),
            "border_width": self.theme.border_width,
            "padding": self.theme.padding,
            "spacing": self.theme.spacing,
        });

        // DFS from roots to preserve tree order in the array
        let mut widget_vals: Vec<Value> = Vec::with_capacity(self.widgets.len());
        let roots = self.root_order.clone();
        for root_id in roots {
            self.collect_widget_json(root_id, 0, &mut widget_vals);
        }

        json!({
            "theme": theme_val,
            "widgets": widget_vals,
        })
    }

    fn collect_widget_json(&self, id: WidgetId, depth: usize, out: &mut Vec<Value>) {
        if depth >= Self::MAX_LAYOUT_DEPTH {
            return;
        }
        if let Some(w) = self.widgets.get(&id) {
            out.push(widget_to_json(w));
            let children = w.children.clone();
            for child_id in children {
                self.collect_widget_json(child_id, depth + 1, out);
            }
        }
    }

    /// Clear the widget tree and reconstruct from a JSON value produced by `save_json`.
    pub fn load_json(&mut self, data: &Value) {
        self.widgets.clear();
        self.root_order.clear();
        self.tab_order.clear();
        self.cmd_queue.clear();
        self.focused = None;

        if let Some(t) = data.get("theme") {
            self.theme = theme_from_json(t);
        }

        let mut max_id: WidgetId = 0;
        if let Some(arr) = data.get("widgets").and_then(|v| v.as_array()) {
            for val in arr {
                let w = widget_from_json(val);
                if w.id > max_id {
                    max_id = w.id;
                }
                if w.parent.is_none() {
                    self.root_order.push(w.id);
                }
                self.widgets.insert(w.id, w);
            }
        }

        // Advance ID counter past all loaded IDs so new widgets never collide
        self.next_id = max_id + 1;
    }

    /// Additive layout loader: parses editor JSON (same format as `save_json`),
    /// creates new widgets with fresh runtime IDs remapped from JSON IDs, sets ALL
    /// visual properties (colors, fonts, borders, layout, visibility, etc.), wires
    /// parent-child relationships, and returns a `name → runtime_id` map.
    ///
    /// Unlike `load_json` this does NOT clear the widget tree, so multiple menus
    /// can coexist. Calling this also updates the active theme to match the file.
    pub fn load_layout(&mut self, data: &Value) -> HashMap<String, WidgetId> {
        if let Some(t) = data.get("theme") {
            self.theme = theme_from_json(t);
        }

        let mut id_map: HashMap<WidgetId, WidgetId> = HashMap::new();
        let mut name_to_id: HashMap<String, WidgetId> = HashMap::new();

        let widgets_arr = match data.get("widgets").and_then(|v| v.as_array()) {
            Some(a) => a.clone(),
            None => return name_to_id,
        };

        // First pass: allocate runtime IDs and set all widget properties.
        for val in &widgets_arr {
            let json_id = val["id"].as_u64().unwrap_or(0);
            let kind = kind_from_str(val["kind"].as_str().unwrap_or("Label"));
            let x = val["x"].as_f64().unwrap_or(0.0) as f32;
            let y = val["y"].as_f64().unwrap_or(0.0) as f32;
            let w = val["w"].as_f64().unwrap_or(0.0) as f32;
            let h = val["h"].as_f64().unwrap_or(0.0) as f32;

            let runtime_id = self.alloc_id();
            let mut widget = Widget::new(runtime_id, kind, x, y, w, h);

            widget.text = val["text"].as_str().unwrap_or("").to_string();
            widget.placeholder = val["placeholder"].as_str().unwrap_or("").to_string();
            widget.color = opt_color_from_json(&val["color"]);
            widget.text_color = opt_color_from_json(&val["text_color"]);
            widget.border_color = opt_color_from_json(&val["border_color"]);
            widget.border_width = val["border_width"].as_f64().unwrap_or(0.0) as f32;
            widget.font_id = val["font_id"].as_str().map(|s| s.to_string());
            widget.font_size = val["font_size"].as_u64().map(|n| n as u32);
            widget.alpha = val["alpha"].as_f64().unwrap_or(1.0) as f32;
            widget.visible = val["visible"].as_bool().unwrap_or(true);
            widget.layout = layout_from_str(val["layout"].as_str().unwrap_or("None"));
            widget.spacing = val["spacing"].as_f64().unwrap_or(0.0) as f32;
            widget.padding = val["padding"].as_f64().unwrap_or(0.0) as f32;
            widget.scroll_y = val["scroll_y"].as_f64().unwrap_or(0.0) as f32;

            // Add as root initially; add_child will remove non-roots in the second pass.
            self.root_order.push(runtime_id);
            self.widgets.insert(runtime_id, widget);

            id_map.insert(json_id, runtime_id);
            if let Some(name) = val["name"].as_str() {
                if !name.is_empty() {
                    name_to_id.insert(name.to_string(), runtime_id);
                }
            }
        }

        // Second pass: wire parent-child relationships using remapped IDs.
        for val in &widgets_arr {
            let json_id = val["id"].as_u64().unwrap_or(0);
            let runtime_id = match id_map.get(&json_id) {
                Some(&id) => id,
                None => continue,
            };
            if let Some(children) = val["children"].as_array() {
                for child_json_id in children.iter().filter_map(|x| x.as_u64()) {
                    if let Some(&child_runtime_id) = id_map.get(&child_json_id) {
                        let _ = self.add_child(runtime_id, child_runtime_id);
                    }
                }
            }
        }

        name_to_id
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

// ─── JSON helpers (free functions) ────────────────────────────────────────────

fn color_to_json(c: Color) -> Value {
    json!([c.r, c.g, c.b, c.a])
}

fn color_from_json(v: &Value) -> Color {
    let arr = match v.as_array() {
        Some(a) => a,
        None => return Color::rgb(0, 0, 0),
    };
    let get = |i: usize, default: u8| {
        arr.get(i)
            .and_then(|x| x.as_u64())
            .unwrap_or(default as u64) as u8
    };
    Color::new(get(0, 0), get(1, 0), get(2, 0), get(3, 255))
}

fn opt_color_to_json(c: Option<Color>) -> Value {
    c.map(color_to_json).unwrap_or(Value::Null)
}

fn opt_color_from_json(v: &Value) -> Option<Color> {
    if v.is_null() {
        None
    } else {
        Some(color_from_json(v))
    }
}

fn kind_str(k: WidgetKind) -> &'static str {
    match k {
        WidgetKind::Label => "Label",
        WidgetKind::Button => "Button",
        WidgetKind::TextInput => "TextInput",
        WidgetKind::Panel => "Panel",
        WidgetKind::ScrollView => "ScrollView",
    }
}

fn kind_from_str(s: &str) -> WidgetKind {
    match s {
        "Button" => WidgetKind::Button,
        "TextInput" => WidgetKind::TextInput,
        "Panel" => WidgetKind::Panel,
        "ScrollView" => WidgetKind::ScrollView,
        _ => WidgetKind::Label,
    }
}

fn layout_str(l: LayoutDir) -> &'static str {
    match l {
        LayoutDir::None => "None",
        LayoutDir::Vertical => "Vertical",
        LayoutDir::Horizontal => "Horizontal",
    }
}

fn layout_from_str(s: &str) -> LayoutDir {
    match s {
        "Vertical" => LayoutDir::Vertical,
        "Horizontal" => LayoutDir::Horizontal,
        _ => LayoutDir::None,
    }
}

fn widget_to_json(w: &Widget) -> Value {
    json!({
        "id": w.id,
        "kind": kind_str(w.kind),
        "x": w.x,
        "y": w.y,
        "w": w.w,
        "h": w.h,
        "color": opt_color_to_json(w.color),
        "text_color": opt_color_to_json(w.text_color),
        "border_color": opt_color_to_json(w.border_color),
        "border_width": w.border_width,
        "visible": w.visible,
        "z": w.z,
        "layout": layout_str(w.layout),
        "spacing": w.spacing,
        "padding": w.padding,
        "parent": w.parent,
        "children": w.children,
        "text": w.text,
        "font_id": w.font_id,
        "font_size": w.font_size,
        "placeholder": w.placeholder,
        "scroll_y": w.scroll_y,
        "alpha": w.alpha,
    })
}

fn widget_from_json(v: &Value) -> Widget {
    let id = v["id"].as_u64().unwrap_or(0);
    let kind = kind_from_str(v["kind"].as_str().unwrap_or("Label"));
    let x = v["x"].as_f64().unwrap_or(0.0) as f32;
    let y = v["y"].as_f64().unwrap_or(0.0) as f32;
    let w = v["w"].as_f64().unwrap_or(0.0) as f32;
    let h = v["h"].as_f64().unwrap_or(0.0) as f32;

    let mut widget = Widget::new(id, kind, x, y, w, h);
    widget.color = opt_color_from_json(&v["color"]);
    widget.text_color = opt_color_from_json(&v["text_color"]);
    widget.border_color = opt_color_from_json(&v["border_color"]);
    widget.border_width = v["border_width"].as_f64().unwrap_or(0.0) as f32;
    widget.visible = v["visible"].as_bool().unwrap_or(true);
    widget.z = v["z"].as_f64().unwrap_or(100.0) as f32;
    widget.layout = layout_from_str(v["layout"].as_str().unwrap_or("None"));
    widget.spacing = v["spacing"].as_f64().unwrap_or(0.0) as f32;
    widget.padding = v["padding"].as_f64().unwrap_or(0.0) as f32;
    widget.parent = v["parent"].as_u64();
    widget.children = v["children"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|x| x.as_u64()).collect())
        .unwrap_or_default();
    widget.text = v["text"].as_str().unwrap_or("").to_string();
    widget.font_id = v["font_id"].as_str().map(|s| s.to_string());
    widget.font_size = v["font_size"].as_u64().map(|n| n as u32);
    widget.placeholder = v["placeholder"].as_str().unwrap_or("").to_string();
    widget.scroll_y = v["scroll_y"].as_f64().unwrap_or(0.0) as f32;
    widget.alpha = v["alpha"].as_f64().unwrap_or(1.0) as f32;
    widget
}

fn theme_from_json(v: &Value) -> Theme {
    Theme {
        font_id: v["font_id"].as_str().unwrap_or("default").to_string(),
        font_size: v["font_size"].as_u64().unwrap_or(18) as u32,
        text_color: color_from_json(&v["text_color"]),
        button_color: color_from_json(&v["button_color"]),
        button_hover_color: color_from_json(&v["button_hover_color"]),
        button_active_color: color_from_json(&v["button_active_color"]),
        panel_color: color_from_json(&v["panel_color"]),
        border_color: color_from_json(&v["border_color"]),
        border_width: v["border_width"].as_f64().unwrap_or(1.0) as f32,
        padding: v["padding"].as_f64().unwrap_or(0.01) as f32,
        spacing: v["spacing"].as_f64().unwrap_or(0.01) as f32,
    }
}

use std::collections::HashMap;
use std::path::PathBuf;

use rython_renderer::Color;
use rython_ui::{LayoutDir, Theme, WidgetId, WidgetKind};
use serde_json::{json, Value};

use crate::state::selection::{Selection, SelectionState};

// ── Constants ─────────────────────────────────────────────────────────────────

const WIDGET_KINDS: &[(&str, WidgetKind)] = &[
    ("Label", WidgetKind::Label),
    ("Button", WidgetKind::Button),
    ("TextInput", WidgetKind::TextInput),
    ("Panel", WidgetKind::Panel),
    ("ScrollView", WidgetKind::ScrollView),
];

// ── Tree actions (collected during render, applied after) ─────────────────────

#[derive(Debug)]
enum TreeAction {
    Select(WidgetId),
    AddChild(WidgetId, WidgetKind),
    Delete(WidgetId),
    Duplicate(WidgetId),
    MoveUp(WidgetId),
    MoveDown(WidgetId),
}

// ── Editor widget data model ──────────────────────────────────────────────────

/// All serializable widget properties, owned by the editor (no Arc callbacks).
#[derive(Debug, Clone)]
struct EditorWidget {
    id: WidgetId,
    kind: WidgetKind,
    /// Display name shown in the tree panel.
    name: String,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    /// Computed absolute position after layout pass.
    abs_x: f32,
    abs_y: f32,
    /// Optional background color ([r,g,b,a] 0-255).
    color: Option<[u8; 4]>,
    /// Optional text color override.
    text_color: Option<[u8; 4]>,
    visible: bool,
    layout: LayoutDir,
    font_id: String,
    font_size: u32,
    /// Display text (Label/Button) or content (TextInput).
    text: String,
    /// Placeholder text for TextInput.
    placeholder: String,
    border_color: Option<[u8; 4]>,
    border_width: f32,
    scroll_y: f32,
    parent: Option<WidgetId>,
    children: Vec<WidgetId>,
}

impl EditorWidget {
    fn new(id: WidgetId, kind: WidgetKind) -> Self {
        let (w, h) = match kind {
            WidgetKind::Panel => (0.4, 0.6),
            WidgetKind::Button => (0.15, 0.05),
            WidgetKind::Label => (0.2, 0.04),
            WidgetKind::TextInput => (0.25, 0.04),
            WidgetKind::ScrollView => (0.3, 0.4),
        };
        Self {
            id,
            kind,
            name: format!("{} #{}", kind_name(kind), id),
            x: 0.1,
            y: 0.1,
            w,
            h,
            abs_x: 0.1,
            abs_y: 0.1,
            color: None,
            text_color: None,
            visible: true,
            layout: LayoutDir::None,
            font_id: "default".to_string(),
            font_size: 18,
            text: String::new(),
            placeholder: String::new(),
            border_color: None,
            border_width: 0.0,
            scroll_y: 0.0,
            parent: None,
            children: Vec::new(),
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "kind": kind_name(self.kind),
            "name": self.name,
            "x": self.x,
            "y": self.y,
            "w": self.w,
            "h": self.h,
            "color": self.color.map(|c| json!([c[0], c[1], c[2], c[3]])),
            "text_color": self.text_color.map(|c| json!([c[0], c[1], c[2], c[3]])),
            "visible": self.visible,
            "layout": layout_dir_name(self.layout),
            "font_id": self.font_id,
            "font_size": self.font_size,
            "text": self.text,
            "placeholder": self.placeholder,
            "border_color": self.border_color.map(|c| json!([c[0], c[1], c[2], c[3]])),
            "border_width": self.border_width,
            "scroll_y": self.scroll_y,
            "parent": self.parent,
            "children": self.children,
        })
    }

    fn from_json(v: &Value) -> Option<Self> {
        let id = v["id"].as_u64()?;
        let kind = parse_widget_kind(v["kind"].as_str()?)?;
        let layout = parse_layout_dir(v["layout"].as_str().unwrap_or("None"));

        let read_color = |key: &str| -> Option<[u8; 4]> {
            let arr = v[key].as_array()?;
            if arr.len() < 4 {
                return None;
            }
            Some([
                arr[0].as_u64()? as u8,
                arr[1].as_u64()? as u8,
                arr[2].as_u64()? as u8,
                arr[3].as_u64()? as u8,
            ])
        };

        let children: Vec<WidgetId> = v["children"]
            .as_array()
            .map(|a| a.iter().filter_map(|x| x.as_u64()).collect())
            .unwrap_or_default();

        let x = v["x"].as_f64().unwrap_or(0.1) as f32;
        let y = v["y"].as_f64().unwrap_or(0.1) as f32;

        Some(Self {
            id,
            kind,
            name: v["name"].as_str().unwrap_or("Widget").to_string(),
            x,
            y,
            w: v["w"].as_f64().unwrap_or(0.1) as f32,
            h: v["h"].as_f64().unwrap_or(0.05) as f32,
            abs_x: x,
            abs_y: y,
            color: read_color("color"),
            text_color: read_color("text_color"),
            visible: v["visible"].as_bool().unwrap_or(true),
            layout,
            font_id: v["font_id"].as_str().unwrap_or("default").to_string(),
            font_size: v["font_size"].as_u64().unwrap_or(18) as u32,
            text: v["text"].as_str().unwrap_or("").to_string(),
            placeholder: v["placeholder"].as_str().unwrap_or("").to_string(),
            border_color: read_color("border_color"),
            border_width: v["border_width"].as_f64().unwrap_or(0.0) as f32,
            scroll_y: v["scroll_y"].as_f64().unwrap_or(0.0) as f32,
            parent: v["parent"].as_u64(),
            children,
        })
    }
}

// ── Snapshot type for undo/redo ───────────────────────────────────────────────

type Snapshot = (
    HashMap<WidgetId, EditorWidget>,
    Vec<WidgetId>,
    WidgetId,
    Theme,
);

// ── UiEditorPanel ─────────────────────────────────────────────────────────────

pub struct UiEditorPanel {
    widgets: HashMap<WidgetId, EditorWidget>,
    root_order: Vec<WidgetId>,
    next_id: WidgetId,
    theme: Theme,
    /// Current file name (without extension) for save/load.
    file_name_buf: String,
    /// Selected kind index in the "Add" combo box.
    add_kind_idx: usize,
    undo_stack: Vec<Snapshot>,
    redo_stack: Vec<Snapshot>,
}

impl UiEditorPanel {
    pub fn new() -> Self {
        Self {
            widgets: HashMap::new(),
            root_order: Vec::new(),
            next_id: 1,
            theme: Theme::default(),
            file_name_buf: "ui".to_string(),
            add_kind_idx: 0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    // ── Undo/Redo ─────────────────────────────────────────────────────────────

    fn snapshot(&self) -> Snapshot {
        (
            self.widgets.clone(),
            self.root_order.clone(),
            self.next_id,
            self.theme.clone(),
        )
    }

    fn push_undo(&mut self) {
        let snap = self.snapshot();
        self.undo_stack.push(snap);
        self.redo_stack.clear();
        if self.undo_stack.len() > 50 {
            self.undo_stack.remove(0);
        }
    }

    fn undo(&mut self, sel: &mut SelectionState) {
        if let Some((widgets, root_order, next_id, theme)) = self.undo_stack.pop() {
            let current = self.snapshot();
            self.redo_stack.push(current);
            self.widgets = widgets;
            self.root_order = root_order;
            self.next_id = next_id;
            self.theme = theme;
            sel.current = Selection::None;
        }
    }

    fn redo(&mut self, sel: &mut SelectionState) {
        if let Some((widgets, root_order, next_id, theme)) = self.redo_stack.pop() {
            let current = self.snapshot();
            self.undo_stack.push(current);
            self.widgets = widgets;
            self.root_order = root_order;
            self.next_id = next_id;
            self.theme = theme;
            sel.current = Selection::None;
        }
    }

    // ── Widget management ─────────────────────────────────────────────────────

    fn alloc_id(&mut self) -> WidgetId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn add_root_widget(&mut self, kind: WidgetKind) -> WidgetId {
        self.push_undo();
        let id = self.alloc_id();
        let widget = EditorWidget::new(id, kind);
        self.root_order.push(id);
        self.widgets.insert(id, widget);
        id
    }

    fn add_child_widget(&mut self, parent_id: WidgetId, kind: WidgetKind) -> WidgetId {
        self.push_undo();
        let id = self.alloc_id();
        let mut widget = EditorWidget::new(id, kind);
        widget.parent = Some(parent_id);
        widget.x = 0.01;
        widget.y = 0.01;
        self.widgets.insert(id, widget);
        if let Some(parent) = self.widgets.get_mut(&parent_id) {
            parent.children.push(id);
        }
        id
    }

    fn delete_widget(&mut self, id: WidgetId, sel: &mut SelectionState) {
        self.push_undo();
        let parent = self.widgets.get(&id).and_then(|w| w.parent);
        if let Some(pid) = parent {
            if let Some(p) = self.widgets.get_mut(&pid) {
                p.children.retain(|&cid| cid != id);
            }
        } else {
            self.root_order.retain(|&rid| rid != id);
        }
        self.delete_subtree(id);
        if sel.current == Selection::Widget(id) {
            sel.current = Selection::None;
        }
    }

    fn delete_subtree(&mut self, id: WidgetId) {
        let children: Vec<WidgetId> = self
            .widgets
            .get(&id)
            .map(|w| w.children.clone())
            .unwrap_or_default();
        for child_id in children {
            self.delete_subtree(child_id);
        }
        self.widgets.remove(&id);
    }

    fn duplicate_widget(&mut self, id: WidgetId) -> WidgetId {
        self.push_undo();
        self.duplicate_recursive(id, None)
    }

    fn duplicate_recursive(&mut self, id: WidgetId, new_parent: Option<WidgetId>) -> WidgetId {
        let (old_name, old_children, mut new_w) = {
            let w = &self.widgets[&id];
            (w.name.clone(), w.children.clone(), w.clone())
        };
        let new_id = self.alloc_id();
        new_w.id = new_id;
        new_w.parent = new_parent;
        new_w.children = Vec::new();
        new_w.name = format!("{} (copy)", old_name);
        self.widgets.insert(new_id, new_w);

        if new_parent.is_none() {
            let pos = self
                .root_order
                .iter()
                .position(|&rid| rid == id)
                .unwrap_or(0);
            let insert_at = (pos + 1).min(self.root_order.len());
            self.root_order.insert(insert_at, new_id);
        } else if let Some(pid) = new_parent {
            if let Some(p) = self.widgets.get_mut(&pid) {
                p.children.push(new_id);
            }
        }

        for child_id in old_children {
            self.duplicate_recursive(child_id, Some(new_id));
        }

        new_id
    }

    fn move_up(&mut self, id: WidgetId) {
        self.push_undo();
        let parent = self.widgets.get(&id).and_then(|w| w.parent);
        if let Some(pid) = parent {
            if let Some(p) = self.widgets.get_mut(&pid) {
                if let Some(pos) = p.children.iter().position(|&cid| cid == id) {
                    if pos > 0 {
                        p.children.swap(pos, pos - 1);
                    }
                }
            }
        } else if let Some(pos) = self.root_order.iter().position(|&rid| rid == id) {
            if pos > 0 {
                self.root_order.swap(pos, pos - 1);
            }
        }
    }

    fn move_down(&mut self, id: WidgetId) {
        self.push_undo();
        let parent = self.widgets.get(&id).and_then(|w| w.parent);
        if let Some(pid) = parent {
            if let Some(p) = self.widgets.get_mut(&pid) {
                if let Some(pos) = p.children.iter().position(|&cid| cid == id) {
                    if pos + 1 < p.children.len() {
                        p.children.swap(pos, pos + 1);
                    }
                }
            }
        } else if let Some(pos) = self.root_order.iter().position(|&rid| rid == id) {
            if pos + 1 < self.root_order.len() {
                self.root_order.swap(pos, pos + 1);
            }
        }
    }

    fn apply_tree_action(&mut self, action: TreeAction, sel: &mut SelectionState) {
        match action {
            TreeAction::Select(id) => {
                sel.current = Selection::Widget(id);
            }
            TreeAction::AddChild(parent_id, kind) => {
                let new_id = self.add_child_widget(parent_id, kind);
                sel.current = Selection::Widget(new_id);
            }
            TreeAction::Delete(id) => {
                self.delete_widget(id, sel);
            }
            TreeAction::Duplicate(id) => {
                let new_id = self.duplicate_widget(id);
                sel.current = Selection::Widget(new_id);
            }
            TreeAction::MoveUp(id) => self.move_up(id),
            TreeAction::MoveDown(id) => self.move_down(id),
        }
    }

    // ── Layout ────────────────────────────────────────────────────────────────

    fn compute_layout(&mut self) {
        let roots = self.root_order.clone();
        for &id in &roots {
            {
                let w = self
                    .widgets
                    .get_mut(&id)
                    .expect("root_order ids must exist in widget map");
                w.abs_x = w.x;
                w.abs_y = w.y;
            }
            self.layout_children(id);
        }
    }

    fn layout_children(&mut self, id: WidgetId) {
        let (abs_x, abs_y, layout, children) = {
            let w = &self.widgets[&id];
            (w.abs_x, w.abs_y, w.layout, w.children.clone())
        };
        const PAD: f32 = 0.01;

        match layout {
            LayoutDir::None => {
                for child_id in children {
                    let (cx, cy) = {
                        let c = &self.widgets[&child_id];
                        (c.x, c.y)
                    };
                    {
                        let c = self
                            .widgets
                            .get_mut(&child_id)
                            .expect("child ids must exist in widget map");
                        c.abs_x = abs_x + cx;
                        c.abs_y = abs_y + cy;
                    }
                    self.layout_children(child_id);
                }
            }
            LayoutDir::Vertical => {
                let mut cursor_y = abs_y + PAD;
                for child_id in children {
                    let child_h = self.widgets[&child_id].h;
                    {
                        let c = self
                            .widgets
                            .get_mut(&child_id)
                            .expect("child ids must exist in widget map");
                        c.abs_x = abs_x + PAD;
                        c.abs_y = cursor_y;
                    }
                    cursor_y += child_h + PAD;
                    self.layout_children(child_id);
                }
            }
            LayoutDir::Horizontal => {
                let mut cursor_x = abs_x + PAD;
                for child_id in children {
                    let child_w = self.widgets[&child_id].w;
                    {
                        let c = self
                            .widgets
                            .get_mut(&child_id)
                            .expect("child ids must exist in widget map");
                        c.abs_x = cursor_x;
                        c.abs_y = abs_y + PAD;
                    }
                    cursor_x += child_w + PAD;
                    self.layout_children(child_id);
                }
            }
        }
    }

    // ── File I/O ──────────────────────────────────────────────────────────────

    fn save_to_file(&self, path: &std::path::Path) -> Result<(), String> {
        let theme_json = json!({
            "font_id": self.theme.font_id,
            "font_size": self.theme.font_size,
            "text_color": color_arr(self.theme.text_color),
            "button_color": color_arr(self.theme.button_color),
            "button_hover_color": color_arr(self.theme.button_hover_color),
            "button_active_color": color_arr(self.theme.button_active_color),
            "panel_color": color_arr(self.theme.panel_color),
            "border_color": color_arr(self.theme.border_color),
            "border_width": self.theme.border_width,
            "padding": self.theme.padding,
            "spacing": self.theme.spacing,
        });

        let mut widgets_json: Vec<Value> = Vec::new();
        for &root_id in &self.root_order {
            self.collect_widget_jsons(root_id, &mut widgets_json);
        }

        let doc = json!({
            "theme": theme_json,
            "root_order": self.root_order,
            "widgets": widgets_json,
        });

        let text = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| e.to_string())
    }

    fn collect_widget_jsons(&self, id: WidgetId, out: &mut Vec<Value>) {
        if let Some(w) = self.widgets.get(&id) {
            out.push(w.to_json());
            let children = w.children.clone();
            for child_id in children {
                self.collect_widget_jsons(child_id, out);
            }
        }
    }

    fn load_from_file(&mut self, path: &std::path::Path) -> Result<(), String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let doc: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;

        self.push_undo();
        self.widgets.clear();
        self.root_order.clear();

        if let Some(theme_v) = doc.get("theme") {
            self.theme = load_theme(theme_v);
        }

        if let Some(arr) = doc["widgets"].as_array() {
            for wv in arr {
                if let Some(w) = EditorWidget::from_json(wv) {
                    let id = w.id;
                    self.widgets.insert(id, w);
                }
            }
        }

        if let Some(arr) = doc["root_order"].as_array() {
            self.root_order = arr.iter().filter_map(|v| v.as_u64()).collect();
        }

        let max_id = self.widgets.keys().copied().max().unwrap_or(0);
        self.next_id = max_id + 1;

        Ok(())
    }

    // ── Public show entry point ───────────────────────────────────────────────

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        project_root: Option<&PathBuf>,
        sel: &mut SelectionState,
    ) {
        // Keyboard shortcuts
        let undo_pressed = ui
            .ctx()
            .input(|i| i.key_pressed(egui::Key::Z) && i.modifiers.ctrl && !i.modifiers.shift);
        let redo_pressed = ui
            .ctx()
            .input(|i| i.key_pressed(egui::Key::Z) && i.modifiers.ctrl && i.modifiers.shift);
        if undo_pressed {
            self.undo(sel);
        }
        if redo_pressed {
            self.redo(sel);
        }

        // ── Toolbar ───────────────────────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.heading("UI Editor");
            ui.separator();
            ui.label("File:");
            ui.add(egui::TextEdit::singleline(&mut self.file_name_buf).desired_width(100.0));

            if ui.button("New").clicked() {
                self.push_undo();
                self.widgets.clear();
                self.root_order.clear();
                self.next_id = 1;
                sel.current = Selection::None;
            }
            if ui.button("Save").clicked() {
                if let Some(root) = project_root {
                    let ui_dir = root.join("ui");
                    let _ = std::fs::create_dir_all(&ui_dir);
                    let path = ui_dir.join(format!("{}.json", self.file_name_buf));
                    let _ = self.save_to_file(&path);
                }
            }
            if ui.button("Load").clicked() {
                if let Some(root) = project_root {
                    let auto_path = root.join("ui").join(format!("{}.json", self.file_name_buf));
                    if auto_path.exists() {
                        let _ = self.load_from_file(&auto_path);
                        sel.current = Selection::None;
                    } else if let Some(picked) = rfd::FileDialog::new()
                        .add_filter("UI Layout", &["json"])
                        .set_directory(root.join("ui"))
                        .pick_file()
                    {
                        let _ = self.load_from_file(&picked);
                        sel.current = Selection::None;
                    }
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(!self.redo_stack.is_empty(), egui::Button::new("↪ Redo"))
                    .clicked()
                {
                    self.redo(sel);
                }
                if ui
                    .add_enabled(!self.undo_stack.is_empty(), egui::Button::new("↩ Undo"))
                    .clicked()
                {
                    self.undo(sel);
                }
            });
        });
        ui.separator();

        // ── Three-column layout ───────────────────────────────────────────────
        let avail = ui.available_size();
        let tree_w = (avail.x * 0.25).max(150.0);
        let preview_w = (avail.x * 0.47).max(180.0);

        ui.horizontal_top(|ui| {
            // ── Left: Widget Tree ─────────────────────────────────────────
            ui.vertical(|ui| {
                ui.set_width(tree_w);
                self.show_tree(ui, sel);
            });

            ui.separator();

            // ── Center: Preview ───────────────────────────────────────────
            ui.vertical(|ui| {
                ui.set_width(preview_w);
                self.show_preview(ui, sel);
            });

            ui.separator();

            // ── Right: Properties / Theme ─────────────────────────────────
            ui.vertical(|ui| {
                self.show_properties(ui, sel);
            });
        });
    }

    // ── Widget tree column ────────────────────────────────────────────────────

    fn show_tree(&mut self, ui: &mut egui::Ui, sel: &mut SelectionState) {
        ui.label(egui::RichText::new("Widget Tree").strong());
        ui.separator();

        // "Add" toolbar
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("ui_add_kind")
                .width(90.0)
                .selected_text(WIDGET_KINDS[self.add_kind_idx].0)
                .show_ui(ui, |ui| {
                    for (i, (label, _)) in WIDGET_KINDS.iter().enumerate() {
                        if ui
                            .selectable_label(self.add_kind_idx == i, *label)
                            .clicked()
                        {
                            self.add_kind_idx = i;
                        }
                    }
                });
            if ui.button("+ Add").clicked() {
                let kind = WIDGET_KINDS[self.add_kind_idx].1;
                let sel_id = widget_sel_id(sel);
                let new_id = if let Some(parent_id) = sel_id {
                    if self.widgets.contains_key(&parent_id) {
                        self.add_child_widget(parent_id, kind)
                    } else {
                        self.add_root_widget(kind)
                    }
                } else {
                    self.add_root_widget(kind)
                };
                sel.current = Selection::Widget(new_id);
            }
        });

        ui.separator();

        egui::ScrollArea::vertical()
            .id_salt("ui_tree_scroll")
            .show(ui, |ui| {
                let roots = self.root_order.clone();
                let mut actions: Vec<TreeAction> = Vec::new();
                let sel_id = widget_sel_id(sel);
                for root_id in roots {
                    show_tree_node(ui, root_id, &self.widgets, sel_id, &mut actions);
                }
                for action in actions {
                    self.apply_tree_action(action, sel);
                }
            });
    }

    // ── Preview column ────────────────────────────────────────────────────────

    fn show_preview(&mut self, ui: &mut egui::Ui, sel: &SelectionState) {
        ui.label(egui::RichText::new("Preview").strong());
        ui.separator();

        self.compute_layout();

        let avail = ui.available_rect_before_wrap();
        let pw = (avail.width() - 8.0).max(50.0);
        let ph = pw * 0.75;
        let preview_rect = egui::Rect::from_min_size(
            avail.min + egui::Vec2::new(4.0, 4.0),
            egui::Vec2::new(pw, ph),
        );

        // Background + frame
        ui.painter()
            .rect_filled(preview_rect, 3.0, egui::Color32::from_rgb(12, 12, 18));
        ui.painter().rect_stroke(
            preview_rect,
            3.0,
            egui::Stroke::new(1.0, egui::Color32::from_rgb(50, 50, 60)),
            egui::StrokeKind::Middle,
        );

        let sel_id = widget_sel_id(sel);
        let roots = self.root_order.clone();
        for root_id in &roots {
            draw_preview_node(
                ui.painter(),
                *root_id,
                &self.widgets,
                &self.theme,
                preview_rect,
                sel_id,
            );
        }

        ui.allocate_rect(preview_rect, egui::Sense::hover());
    }

    // ── Properties column ─────────────────────────────────────────────────────

    fn show_properties(&mut self, ui: &mut egui::Ui, sel: &SelectionState) {
        let sel_id = widget_sel_id(sel);

        if let Some(id) = sel_id {
            if self.widgets.contains_key(&id) {
                ui.label(egui::RichText::new("Properties").strong());
                ui.separator();

                let mut w = self.widgets[&id].clone();
                let mut changed = false;

                egui::ScrollArea::vertical()
                    .id_salt("ui_props_scroll")
                    .show(ui, |ui| {
                        show_widget_props(ui, &mut w, &mut changed);
                    });

                if changed {
                    self.push_undo();
                    self.widgets.insert(id, w);
                }
                return;
            }
        }

        // No widget selected — show theme editor
        ui.label(egui::RichText::new("Theme").strong());
        ui.separator();
        let mut theme = self.theme.clone();
        let mut theme_changed = false;
        egui::ScrollArea::vertical()
            .id_salt("ui_theme_scroll")
            .show(ui, |ui| {
                show_theme_editor(ui, &mut theme, &mut theme_changed);
            });
        if theme_changed {
            self.push_undo();
            self.theme = theme;
        }
    }
}

impl Default for UiEditorPanel {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tree rendering (free function — read-only widget data) ────────────────────

fn show_tree_node(
    ui: &mut egui::Ui,
    id: WidgetId,
    widgets: &HashMap<WidgetId, EditorWidget>,
    sel_id: Option<WidgetId>,
    actions: &mut Vec<TreeAction>,
) {
    let Some(w) = widgets.get(&id) else { return };
    let display = format!("{} ({})", w.name, kind_name(w.kind));
    let is_selected = sel_id == Some(id);
    let children = w.children.clone();
    let has_children = !children.is_empty();

    let resp = ui.selectable_label(is_selected, &display);
    if resp.clicked() {
        actions.push(TreeAction::Select(id));
    }

    resp.context_menu(|ui: &mut egui::Ui| {
        ui.label("Add child:");
        for &(label, kind) in WIDGET_KINDS {
            if ui.button(format!("  {label}")).clicked() {
                actions.push(TreeAction::AddChild(id, kind));
                ui.close_menu();
            }
        }
        ui.separator();
        if ui.button("Delete").clicked() {
            actions.push(TreeAction::Delete(id));
            ui.close_menu();
        }
        if ui.button("Duplicate").clicked() {
            actions.push(TreeAction::Duplicate(id));
            ui.close_menu();
        }
        ui.separator();
        if ui.button("Move Up ↑").clicked() {
            actions.push(TreeAction::MoveUp(id));
            ui.close_menu();
        }
        if ui.button("Move Down ↓").clicked() {
            actions.push(TreeAction::MoveDown(id));
            ui.close_menu();
        }
    });

    if has_children {
        ui.indent(id, |ui| {
            for child_id in &children {
                show_tree_node(ui, *child_id, widgets, sel_id, actions);
            }
        });
    }
}

// ── Preview rendering (free function — read-only) ─────────────────────────────

fn draw_preview_node(
    painter: &egui::Painter,
    id: WidgetId,
    widgets: &HashMap<WidgetId, EditorWidget>,
    theme: &Theme,
    rect: egui::Rect,
    sel_id: Option<WidgetId>,
) {
    let Some(w) = widgets.get(&id) else { return };
    if !w.visible {
        return;
    }

    // Map normalized [0,1] coords to preview pixel rect
    let px = rect.min.x + w.abs_x * rect.width();
    let py = rect.min.y + w.abs_y * rect.height();
    let pw = (w.w * rect.width()).max(2.0);
    let ph = (w.h * rect.height()).max(2.0);
    let widget_rect = egui::Rect::from_min_size(egui::Pos2::new(px, py), egui::Vec2::new(pw, ph));

    // Background
    let bg = w.color.map(arr_to_egui).unwrap_or_else(|| match w.kind {
        WidgetKind::Label => egui::Color32::TRANSPARENT,
        WidgetKind::Button | WidgetKind::TextInput => color_to_egui(theme.button_color),
        WidgetKind::Panel | WidgetKind::ScrollView => color_to_egui(theme.panel_color),
    });
    if bg != egui::Color32::TRANSPARENT {
        painter.rect_filled(widget_rect, 2.0, bg);
    }

    // Border
    if let Some(bc) = w.border_color {
        if w.border_width > 0.0 {
            painter.rect_stroke(
                widget_rect,
                2.0,
                egui::Stroke::new(w.border_width.min(3.0), arr_to_egui(bc)),
                egui::StrokeKind::Middle,
            );
        }
    }

    // Selection highlight
    if sel_id == Some(id) {
        painter.rect_stroke(
            widget_rect,
            2.0,
            egui::Stroke::new(1.5, egui::Color32::from_rgb(80, 160, 240)),
            egui::StrokeKind::Middle,
        );
    }

    // Text
    let show_text = matches!(
        w.kind,
        WidgetKind::Label | WidgetKind::Button | WidgetKind::TextInput
    );
    if show_text {
        let text = if w.text.is_empty() && w.kind == WidgetKind::TextInput {
            w.placeholder.as_str()
        } else {
            w.text.as_str()
        };
        if !text.is_empty() {
            let text_col = w
                .text_color
                .map(arr_to_egui)
                .unwrap_or_else(|| color_to_egui(theme.text_color));
            let font_size = (ph * 0.45).clamp(8.0, 14.0);
            painter.text(
                widget_rect.center(),
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::proportional(font_size),
                text_col,
            );
        }
    }

    // Recurse into children
    let children = w.children.clone();
    for child_id in children {
        draw_preview_node(painter, child_id, widgets, theme, rect, sel_id);
    }
}

// ── Widget property editor ────────────────────────────────────────────────────

fn show_widget_props(ui: &mut egui::Ui, w: &mut EditorWidget, changed: &mut bool) {
    // Name
    ui.label("Name");
    *changed |= ui.text_edit_singleline(&mut w.name).changed();
    ui.separator();

    // Geometry
    ui.label(egui::RichText::new("Geometry").italics());
    egui::Grid::new("wg_geom").num_columns(2).show(ui, |ui| {
        ui.label("X");
        *changed |= ui
            .add(egui::DragValue::new(&mut w.x).speed(0.002))
            .changed();
        ui.end_row();
        ui.label("Y");
        *changed |= ui
            .add(egui::DragValue::new(&mut w.y).speed(0.002))
            .changed();
        ui.end_row();
        ui.label("W");
        *changed |= ui
            .add(egui::DragValue::new(&mut w.w).speed(0.002))
            .changed();
        ui.end_row();
        ui.label("H");
        *changed |= ui
            .add(egui::DragValue::new(&mut w.h).speed(0.002))
            .changed();
        ui.end_row();
    });

    // Visibility + layout direction
    ui.horizontal(|ui| {
        ui.label("Visible");
        *changed |= ui.checkbox(&mut w.visible, "").changed();
    });

    ui.label("Layout Dir");
    egui::ComboBox::from_id_salt("wg_layout")
        .selected_text(layout_dir_name(w.layout))
        .show_ui(ui, |ui: &mut egui::Ui| {
            for (label, dir) in [
                ("None", LayoutDir::None),
                ("Vertical", LayoutDir::Vertical),
                ("Horizontal", LayoutDir::Horizontal),
            ] {
                if ui.selectable_value(&mut w.layout, dir, label).changed() {
                    *changed = true;
                }
            }
        });

    // Background color
    ui.separator();
    ui.label(egui::RichText::new("Appearance").italics());
    ui.horizontal(|ui| {
        let mut has_color = w.color.is_some();
        if ui.checkbox(&mut has_color, "Custom Color").changed() {
            w.color = if has_color {
                Some([100, 100, 120, 255])
            } else {
                None
            };
            *changed = true;
        }
    });
    if let Some(ref mut c) = w.color {
        let mut c32 = arr_to_egui(*c);
        if ui.color_edit_button_srgba(&mut c32).changed() {
            *c = egui_to_arr(c32);
            *changed = true;
        }
    }

    // Kind-specific fields
    ui.separator();
    ui.label(egui::RichText::new(kind_name(w.kind)).strong());

    match w.kind {
        WidgetKind::Label | WidgetKind::Button => {
            egui::Grid::new("wg_text_props")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("Text");
                    *changed |= ui.text_edit_singleline(&mut w.text).changed();
                    ui.end_row();
                    ui.label("Font ID");
                    *changed |= ui.text_edit_singleline(&mut w.font_id).changed();
                    ui.end_row();
                    ui.label("Font Size");
                    *changed |= ui
                        .add(egui::DragValue::new(&mut w.font_size).speed(1.0))
                        .changed();
                    ui.end_row();
                });
            ui.horizontal(|ui| {
                let mut has_tc = w.text_color.is_some();
                if ui.checkbox(&mut has_tc, "Custom Text Color").changed() {
                    w.text_color = if has_tc {
                        Some([220, 220, 220, 255])
                    } else {
                        None
                    };
                    *changed = true;
                }
            });
            if let Some(ref mut tc) = w.text_color {
                let mut c32 = arr_to_egui(*tc);
                if ui.color_edit_button_srgba(&mut c32).changed() {
                    *tc = egui_to_arr(c32);
                    *changed = true;
                }
            }
        }
        WidgetKind::TextInput => {
            egui::Grid::new("wg_ti_props")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("Placeholder");
                    *changed |= ui.text_edit_singleline(&mut w.placeholder).changed();
                    ui.end_row();
                    ui.label("Font ID");
                    *changed |= ui.text_edit_singleline(&mut w.font_id).changed();
                    ui.end_row();
                    ui.label("Font Size");
                    *changed |= ui
                        .add(egui::DragValue::new(&mut w.font_size).speed(1.0))
                        .changed();
                    ui.end_row();
                });
        }
        WidgetKind::Panel => {
            egui::Grid::new("wg_panel_props")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("Border Width");
                    *changed |= ui
                        .add(egui::DragValue::new(&mut w.border_width).speed(0.1))
                        .changed();
                    ui.end_row();
                });
            ui.horizontal(|ui| {
                let mut has_bc = w.border_color.is_some();
                if ui.checkbox(&mut has_bc, "Border Color").changed() {
                    w.border_color = if has_bc {
                        Some([100, 100, 120, 255])
                    } else {
                        None
                    };
                    *changed = true;
                }
            });
            if let Some(ref mut bc) = w.border_color {
                let mut c32 = arr_to_egui(*bc);
                if ui.color_edit_button_srgba(&mut c32).changed() {
                    *bc = egui_to_arr(c32);
                    *changed = true;
                }
            }
        }
        WidgetKind::ScrollView => {
            egui::Grid::new("wg_sv_props")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("Scroll Y");
                    *changed |= ui
                        .add(egui::DragValue::new(&mut w.scroll_y).speed(0.01))
                        .changed();
                    ui.end_row();
                });
        }
    }
}

// ── Theme editor ──────────────────────────────────────────────────────────────

fn show_theme_editor(ui: &mut egui::Ui, theme: &mut Theme, changed: &mut bool) {
    egui::Grid::new("theme_scalars")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("Font ID");
            *changed |= ui.text_edit_singleline(&mut theme.font_id).changed();
            ui.end_row();
            ui.label("Font Size");
            *changed |= ui
                .add(egui::DragValue::new(&mut theme.font_size).speed(1.0))
                .changed();
            ui.end_row();
            ui.label("Border Width");
            *changed |= ui
                .add(egui::DragValue::new(&mut theme.border_width).speed(0.1))
                .changed();
            ui.end_row();
            ui.label("Padding");
            *changed |= ui
                .add(egui::DragValue::new(&mut theme.padding).speed(0.001))
                .changed();
            ui.end_row();
            ui.label("Spacing");
            *changed |= ui
                .add(egui::DragValue::new(&mut theme.spacing).speed(0.001))
                .changed();
            ui.end_row();
        });

    ui.separator();
    ui.label(egui::RichText::new("Colors").italics());
    egui::Grid::new("theme_colors")
        .num_columns(2)
        .show(ui, |ui| {
            let fields: &mut [(&str, &mut Color)] = &mut [
                ("Text", &mut theme.text_color),
                ("Button", &mut theme.button_color),
                ("Button Hover", &mut theme.button_hover_color),
                ("Button Active", &mut theme.button_active_color),
                ("Panel", &mut theme.panel_color),
                ("Border", &mut theme.border_color),
            ];
            for (label, color) in fields.iter_mut() {
                ui.label(*label);
                let mut c32 = color_to_egui(**color);
                if ui.color_edit_button_srgba(&mut c32).changed() {
                    **color = egui_to_color(c32);
                    *changed = true;
                }
                ui.end_row();
            }
        });
}

// ── Helper functions ──────────────────────────────────────────────────────────

fn kind_name(kind: WidgetKind) -> &'static str {
    match kind {
        WidgetKind::Label => "Label",
        WidgetKind::Button => "Button",
        WidgetKind::TextInput => "TextInput",
        WidgetKind::Panel => "Panel",
        WidgetKind::ScrollView => "ScrollView",
    }
}

fn layout_dir_name(dir: LayoutDir) -> &'static str {
    match dir {
        LayoutDir::None => "None",
        LayoutDir::Vertical => "Vertical",
        LayoutDir::Horizontal => "Horizontal",
    }
}

fn parse_layout_dir(s: &str) -> LayoutDir {
    match s {
        "Vertical" => LayoutDir::Vertical,
        "Horizontal" => LayoutDir::Horizontal,
        _ => LayoutDir::None,
    }
}

fn parse_widget_kind(s: &str) -> Option<WidgetKind> {
    match s {
        "Label" => Some(WidgetKind::Label),
        "Button" => Some(WidgetKind::Button),
        "TextInput" => Some(WidgetKind::TextInput),
        "Panel" => Some(WidgetKind::Panel),
        "ScrollView" => Some(WidgetKind::ScrollView),
        _ => None,
    }
}

fn color_to_egui(c: Color) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

fn egui_to_color(c: egui::Color32) -> Color {
    Color::new(c.r(), c.g(), c.b(), c.a())
}

fn arr_to_egui(arr: [u8; 4]) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(arr[0], arr[1], arr[2], arr[3])
}

fn egui_to_arr(c: egui::Color32) -> [u8; 4] {
    [c.r(), c.g(), c.b(), c.a()]
}

fn color_arr(c: Color) -> [u8; 4] {
    [c.r, c.g, c.b, c.a]
}

fn widget_sel_id(sel: &SelectionState) -> Option<WidgetId> {
    if let Selection::Widget(id) = sel.current {
        Some(id)
    } else {
        None
    }
}

fn load_theme(v: &Value) -> Theme {
    let def = Theme::default();
    let read_color = |key: &str, fallback: Color| -> Color {
        let Some(arr) = v[key].as_array() else {
            return fallback;
        };
        if arr.len() < 4 {
            return fallback;
        }
        Color::new(
            arr[0].as_u64().unwrap_or(0) as u8,
            arr[1].as_u64().unwrap_or(0) as u8,
            arr[2].as_u64().unwrap_or(0) as u8,
            arr[3].as_u64().unwrap_or(255) as u8,
        )
    };
    Theme {
        font_id: v["font_id"].as_str().unwrap_or(&def.font_id).to_string(),
        font_size: v["font_size"].as_u64().unwrap_or(def.font_size as u64) as u32,
        text_color: read_color("text_color", def.text_color),
        button_color: read_color("button_color", def.button_color),
        button_hover_color: read_color("button_hover_color", def.button_hover_color),
        button_active_color: read_color("button_active_color", def.button_active_color),
        panel_color: read_color("panel_color", def.panel_color),
        border_color: read_color("border_color", def.border_color),
        border_width: v["border_width"]
            .as_f64()
            .unwrap_or(def.border_width as f64) as f32,
        padding: v["padding"].as_f64().unwrap_or(def.padding as f64) as f32,
        spacing: v["spacing"].as_f64().unwrap_or(def.spacing as f64) as f32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::selection::SelectionState;

    // ── Construction ──────────────────────────────────────────────────────────

    #[test]
    fn new_panel_is_empty() {
        let panel = UiEditorPanel::new();
        assert!(panel.widgets.is_empty());
        assert!(panel.root_order.is_empty());
        assert_eq!(panel.next_id, 1);
        assert!(panel.undo_stack.is_empty());
        assert!(panel.redo_stack.is_empty());
    }

    // ── add_root_widget ───────────────────────────────────────────────────────

    #[test]
    fn add_root_widget_creates_widget_entry() {
        let mut panel = UiEditorPanel::new();
        let id = panel.add_root_widget(WidgetKind::Label);
        assert!(panel.widgets.contains_key(&id));
        assert_eq!(panel.widgets[&id].kind, WidgetKind::Label);
    }

    #[test]
    fn add_root_widget_appends_to_root_order() {
        let mut panel = UiEditorPanel::new();
        let id = panel.add_root_widget(WidgetKind::Label);
        assert!(panel.root_order.contains(&id));
    }

    #[test]
    fn add_root_widget_increments_next_id() {
        let mut panel = UiEditorPanel::new();
        let id1 = panel.add_root_widget(WidgetKind::Label);
        let id2 = panel.add_root_widget(WidgetKind::Button);
        assert_ne!(id1, id2);
        assert_eq!(id2, id1 + 1);
    }

    // ── add_child_widget ──────────────────────────────────────────────────────

    #[test]
    fn add_child_widget_links_to_parent() {
        let mut panel = UiEditorPanel::new();
        let parent_id = panel.add_root_widget(WidgetKind::Panel);
        let child_id = panel.add_child_widget(parent_id, WidgetKind::Button);
        assert_eq!(panel.widgets[&child_id].parent, Some(parent_id));
        assert!(panel.widgets[&parent_id].children.contains(&child_id));
    }

    #[test]
    fn add_child_widget_not_in_root_order() {
        let mut panel = UiEditorPanel::new();
        let parent_id = panel.add_root_widget(WidgetKind::Panel);
        let child_id = panel.add_child_widget(parent_id, WidgetKind::Button);
        assert!(!panel.root_order.contains(&child_id));
    }

    // ── delete_widget ─────────────────────────────────────────────────────────

    #[test]
    fn delete_root_widget_removes_from_map_and_root_order() {
        let mut panel = UiEditorPanel::new();
        let mut sel = SelectionState::default();
        let id = panel.add_root_widget(WidgetKind::Label);
        panel.delete_widget(id, &mut sel);
        assert!(!panel.widgets.contains_key(&id));
        assert!(!panel.root_order.contains(&id));
    }

    #[test]
    fn delete_widget_clears_selection_if_selected() {
        let mut panel = UiEditorPanel::new();
        let mut sel = SelectionState::default();
        let id = panel.add_root_widget(WidgetKind::Button);
        sel.current = Selection::Widget(id);
        panel.delete_widget(id, &mut sel);
        assert_eq!(sel.current, Selection::None);
    }

    #[test]
    fn delete_widget_removes_child_from_parent_children_list() {
        let mut panel = UiEditorPanel::new();
        let mut sel = SelectionState::default();
        let parent_id = panel.add_root_widget(WidgetKind::Panel);
        let child_id = panel.add_child_widget(parent_id, WidgetKind::Label);
        panel.delete_widget(child_id, &mut sel);
        assert!(!panel.widgets.contains_key(&child_id));
        assert!(!panel.widgets[&parent_id].children.contains(&child_id));
    }

    #[test]
    fn delete_widget_removes_entire_subtree() {
        let mut panel = UiEditorPanel::new();
        let mut sel = SelectionState::default();
        let root = panel.add_root_widget(WidgetKind::Panel);
        let child = panel.add_child_widget(root, WidgetKind::Panel);
        let grandchild = panel.add_child_widget(child, WidgetKind::Label);
        panel.delete_widget(root, &mut sel);
        assert!(!panel.widgets.contains_key(&root));
        assert!(!panel.widgets.contains_key(&child));
        assert!(!panel.widgets.contains_key(&grandchild));
    }

    // ── duplicate_widget ──────────────────────────────────────────────────────

    #[test]
    fn duplicate_widget_creates_new_entry_with_copy_suffix() {
        let mut panel = UiEditorPanel::new();
        let id = panel.add_root_widget(WidgetKind::Label);
        panel.widgets.get_mut(&id).unwrap().name = "MyLabel".to_string();
        let dup_id = panel.duplicate_widget(id);
        assert_ne!(dup_id, id);
        assert!(panel.widgets[&dup_id].name.contains("copy"));
    }

    #[test]
    fn duplicate_widget_inserts_after_original_in_root_order() {
        let mut panel = UiEditorPanel::new();
        let a = panel.add_root_widget(WidgetKind::Label);
        let _b = panel.add_root_widget(WidgetKind::Button);
        let a_dup = panel.duplicate_widget(a);
        let a_pos = panel.root_order.iter().position(|&x| x == a).unwrap();
        let dup_pos = panel.root_order.iter().position(|&x| x == a_dup).unwrap();
        assert_eq!(dup_pos, a_pos + 1);
    }

    #[test]
    fn duplicate_widget_same_kind() {
        let mut panel = UiEditorPanel::new();
        let id = panel.add_root_widget(WidgetKind::Button);
        let dup_id = panel.duplicate_widget(id);
        assert_eq!(panel.widgets[&dup_id].kind, WidgetKind::Button);
    }

    // ── move_up / move_down ───────────────────────────────────────────────────

    #[test]
    fn move_up_swaps_with_predecessor_in_root_order() {
        let mut panel = UiEditorPanel::new();
        let a = panel.add_root_widget(WidgetKind::Label);
        let b = panel.add_root_widget(WidgetKind::Button);
        panel.move_up(b);
        assert_eq!(panel.root_order[0], b);
        assert_eq!(panel.root_order[1], a);
    }

    #[test]
    fn move_up_noop_when_already_first() {
        let mut panel = UiEditorPanel::new();
        let a = panel.add_root_widget(WidgetKind::Label);
        let b = panel.add_root_widget(WidgetKind::Button);
        panel.move_up(a);
        assert_eq!(panel.root_order[0], a);
        assert_eq!(panel.root_order[1], b);
    }

    #[test]
    fn move_down_swaps_with_successor_in_root_order() {
        let mut panel = UiEditorPanel::new();
        let a = panel.add_root_widget(WidgetKind::Label);
        let b = panel.add_root_widget(WidgetKind::Button);
        panel.move_down(a);
        assert_eq!(panel.root_order[0], b);
        assert_eq!(panel.root_order[1], a);
    }

    #[test]
    fn move_down_noop_when_already_last() {
        let mut panel = UiEditorPanel::new();
        let a = panel.add_root_widget(WidgetKind::Label);
        let b = panel.add_root_widget(WidgetKind::Button);
        panel.move_down(b);
        assert_eq!(panel.root_order[0], a);
        assert_eq!(panel.root_order[1], b);
    }

    // ── undo / redo ───────────────────────────────────────────────────────────

    #[test]
    fn undo_restores_state_before_last_change() {
        let mut panel = UiEditorPanel::new();
        let mut sel = SelectionState::default();
        // push_undo is called inside add_root_widget before the change
        let _ = panel.add_root_widget(WidgetKind::Label);
        let count_one = panel.widgets.len();
        let _ = panel.add_root_widget(WidgetKind::Button);
        assert_eq!(panel.widgets.len(), count_one + 1);
        panel.undo(&mut sel);
        assert_eq!(panel.widgets.len(), count_one);
    }

    #[test]
    fn redo_reapplies_undone_change() {
        let mut panel = UiEditorPanel::new();
        let mut sel = SelectionState::default();
        let _ = panel.add_root_widget(WidgetKind::Label);
        let _ = panel.add_root_widget(WidgetKind::Button);
        let count_two = panel.widgets.len();
        panel.undo(&mut sel);
        panel.redo(&mut sel);
        assert_eq!(panel.widgets.len(), count_two);
    }

    #[test]
    fn undo_clears_selection() {
        let mut panel = UiEditorPanel::new();
        let mut sel = SelectionState::default();
        let id = panel.add_root_widget(WidgetKind::Label);
        sel.current = Selection::Widget(id);
        let _ = panel.add_root_widget(WidgetKind::Button);
        panel.undo(&mut sel);
        assert_eq!(sel.current, Selection::None);
    }

    #[test]
    fn undo_stack_caps_at_50_entries() {
        let mut panel = UiEditorPanel::new();
        for _ in 0..60 {
            panel.add_root_widget(WidgetKind::Label);
        }
        assert!(panel.undo_stack.len() <= 50);
    }

    #[test]
    fn undo_noop_on_empty_stack() {
        let mut panel = UiEditorPanel::new();
        let mut sel = SelectionState::default();
        // Should not panic
        panel.undo(&mut sel);
        panel.undo(&mut sel);
    }

    #[test]
    fn redo_noop_on_empty_redo_stack() {
        let mut panel = UiEditorPanel::new();
        let mut sel = SelectionState::default();
        // Should not panic
        panel.redo(&mut sel);
    }

    // ── compute_layout ────────────────────────────────────────────────────────

    #[test]
    fn compute_layout_sets_abs_position_for_root_widget() {
        let mut panel = UiEditorPanel::new();
        let id = panel.add_root_widget(WidgetKind::Label);
        panel.widgets.get_mut(&id).unwrap().x = 0.2;
        panel.widgets.get_mut(&id).unwrap().y = 0.3;
        panel.compute_layout();
        assert!((panel.widgets[&id].abs_x - 0.2).abs() < 1e-5);
        assert!((panel.widgets[&id].abs_y - 0.3).abs() < 1e-5);
    }

    #[test]
    fn compute_layout_none_propagates_parent_abs_to_child() {
        let mut panel = UiEditorPanel::new();
        let parent = panel.add_root_widget(WidgetKind::Panel);
        let child = panel.add_child_widget(parent, WidgetKind::Label);
        panel.widgets.get_mut(&parent).unwrap().x = 0.1;
        panel.widgets.get_mut(&parent).unwrap().y = 0.2;
        panel.widgets.get_mut(&child).unwrap().x = 0.05;
        panel.widgets.get_mut(&child).unwrap().y = 0.05;
        panel.compute_layout();
        assert!((panel.widgets[&child].abs_x - 0.15).abs() < 1e-5);
        assert!((panel.widgets[&child].abs_y - 0.25).abs() < 1e-5);
    }
}

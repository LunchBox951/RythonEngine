use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::Instant;

use glam::Vec3;
use rython_ecs::component::TransformComponent;
use rython_ecs::{EntityId, Scene};
use rython_renderer::{GpuContext, RendererConfig, RendererState};

use crate::panels::asset_browser::AssetBrowserPanel;
use crate::panels::component_inspector::ComponentInspectorPanel;
use crate::panels::scene_hierarchy::SceneHierarchyPanel;
use crate::panels::script_panel::ScriptPanel;
use crate::panels::ui_editor::UiEditorPanel;
use crate::panels::viewport_panel;
use crate::project::io::{create_project, list_scenes, load_scene, open_project, save_scene};
use crate::state::undo::{DespawnEntity, EntitySnapshot, ModifyComponent, SpawnEntity};
use crate::state::{ProjectState, SelectionState, UndoStack, ViewportState};
use crate::viewport::gizmo::{
    apply_rotate_drag, apply_scale_drag, apply_translate_drag, draw_gizmo, hit_test_gizmo_with_vp,
    GizmoDrag, GizmoMode,
};
use crate::viewport::ViewportTexture;

// ─────────────────────────────────────────────────────────────────────────────
// Phase 5: Log types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

#[derive(Clone)]
pub struct ConsoleEntry {
    pub level: LogLevel,
    pub message: String,
}

impl ConsoleEntry {
    fn new(level: LogLevel, message: impl Into<String>) -> Self {
        Self {
            level,
            message: message.into(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Phase 5: Preferences
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum EditorTheme {
    Dark,
    Light,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Preferences {
    pub theme: EditorTheme,
    pub font_size: f32,
    pub viewport_bg: [u8; 3],
    pub grid_spacing: f32,
    /// 0 = disabled; positive value = seconds between auto-saves.
    pub auto_save_secs: u64,
    /// 0 = Translate, 1 = Rotate, 2 = Scale.
    pub default_gizmo_mode: usize,
    pub external_editor: String,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            theme: EditorTheme::Dark,
            font_size: 14.0,
            viewport_bg: [30, 30, 30],
            grid_spacing: 1.0,
            auto_save_secs: 0,
            default_gizmo_mode: 0,
            external_editor: String::new(),
        }
    }
}

impl Preferences {
    fn config_path() -> Option<PathBuf> {
        std::env::var("HOME").ok().map(|h| {
            PathBuf::from(h)
                .join(".config")
                .join("rython-editor")
                .join("preferences.json")
        })
    }

    fn load() -> Self {
        if let Some(path) = Self::config_path() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(p) = serde_json::from_str(&content) {
                    return p;
                }
            }
        }
        Self::default()
    }

    fn save(&self) {
        let Some(path) = Self::config_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(content) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&path, content);
        }
    }

    fn apply_theme(&self, ctx: &egui::Context) {
        match self.theme {
            EditorTheme::Dark => ctx.set_visuals(egui::Visuals::dark()),
            EditorTheme::Light => ctx.set_visuals(egui::Visuals::light()),
        }
    }

    pub fn resolved_gizmo_mode(&self) -> GizmoMode {
        match self.default_gizmo_mode {
            1 => GizmoMode::Rotate,
            2 => GizmoMode::Scale,
            _ => GizmoMode::Translate,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Phase 5: Recent projects
// ─────────────────────────────────────────────────────────────────────────────

fn recent_projects_path() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(|h| {
        PathBuf::from(h)
            .join(".config")
            .join("rython-editor")
            .join("recent.json")
    })
}

fn load_recent_projects() -> Vec<PathBuf> {
    let Some(path) = recent_projects_path() else {
        return Vec::new();
    };
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(paths): Result<Vec<String>, _> = serde_json::from_str(&content) else {
        return Vec::new();
    };
    paths
        .into_iter()
        .map(PathBuf::from)
        .filter(|p| p.join("project.json").exists())
        .take(10)
        .collect()
}

fn save_recent_projects(projects: &[PathBuf]) {
    let Some(path) = recent_projects_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let strs: Vec<String> = projects
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    if let Ok(content) = serde_json::to_string_pretty(&strs) {
        let _ = std::fs::write(&path, content);
    }
}

fn push_recent(projects: &mut Vec<PathBuf>, dir: PathBuf) {
    projects.retain(|p| p != &dir);
    projects.insert(0, dir);
    projects.truncate(10);
}

// ─────────────────────────────────────────────────────────────────────────────
// Phase 5: Play session
// ─────────────────────────────────────────────────────────────────────────────

pub struct PlaySession {
    child: std::process::Child,
    output_rx: Receiver<(LogLevel, String)>,
}

impl PlaySession {
    fn launch(
        project_root: &Path,
        entry_point: Option<&str>,
        console: &mut Vec<ConsoleEntry>,
    ) -> Option<Self> {
        let bin = find_rython_bin();
        let Some(bin) = bin else {
            console.push(ConsoleEntry::new(
                LogLevel::Error,
                "rython binary not found. Make sure it is in PATH or ./target/.",
            ));
            return None;
        };

        let mut cmd = std::process::Command::new(&bin);
        cmd.arg("--script-dir")
            .arg(project_root.join("scripts"))
            .arg("--config")
            .arg(project_root.join("project.json"));
        if let Some(ep) = entry_point {
            cmd.arg("--entry-point").arg(ep);
        }
        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                console.push(ConsoleEntry::new(
                    LogLevel::Error,
                    format!("Failed to launch game: {e}"),
                ));
                return None;
            }
        };

        let (tx, rx) = mpsc::channel::<(LogLevel, String)>();
        let tx2 = tx.clone();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send((LogLevel::Info, line)).is_err() {
                    break;
                }
            }
        });
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if tx2.send((LogLevel::Warn, line)).is_err() {
                    break;
                }
            }
        });

        console.push(ConsoleEntry::new(LogLevel::Info, "▶ Game started"));
        Some(PlaySession {
            child,
            output_rx: rx,
        })
    }

    fn poll(&mut self, console: &mut Vec<ConsoleEntry>) {
        while let Ok((level, line)) = self.output_rx.try_recv() {
            console.push(ConsoleEntry::new(level, line));
        }
    }

    fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    fn stop(&mut self, console: &mut Vec<ConsoleEntry>) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        // Drain remaining output
        while let Ok((level, line)) = self.output_rx.try_recv() {
            console.push(ConsoleEntry::new(level, line));
        }
        console.push(ConsoleEntry::new(LogLevel::Info, "■ Game stopped"));
    }
}

fn find_rython_bin() -> Option<PathBuf> {
    // Check in PATH by scanning PATH env var
    if let Ok(path_var) = std::env::var("PATH") {
        let sep = if cfg!(windows) { ';' } else { ':' };
        for dir in path_var.split(sep) {
            let candidate = PathBuf::from(dir).join("rython");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    // Check relative to cwd
    for candidate in ["./target/debug/rython", "./target/release/rython"] {
        let p = PathBuf::from(candidate);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────────
// Editor tab enums
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Default, PartialEq)]
enum EditorTab {
    #[default]
    Viewport,
    UiEditor,
}

#[derive(Default, PartialEq)]
enum BottomTab {
    #[default]
    AssetBrowser,
    Console,
}

// ─────────────────────────────────────────────────────────────────────────────
// App
// ─────────────────────────────────────────────────────────────────────────────

pub struct EditorApp {
    // Phase 2–4 state
    renderer: Option<RendererState>,
    scene: Arc<Scene>,
    viewport_texture: Option<ViewportTexture>,
    viewport_state: ViewportState,
    show_hierarchy: bool,
    show_inspector: bool,
    selection: SelectionState,
    undo_stack: UndoStack,
    project: ProjectState,
    hierarchy_panel: SceneHierarchyPanel,
    inspector_panel: ComponentInspectorPanel,
    asset_browser: AssetBrowserPanel,
    ui_editor: UiEditorPanel,
    script_panel: ScriptPanel,
    show_scripts: bool,
    active_tab: EditorTab,

    // Phase 5 state
    multi_select: Vec<EntityId>,
    clipboard: Vec<EntitySnapshot>,
    console_lines: Vec<ConsoleEntry>,
    console_auto_scroll: bool,
    show_console: bool,
    bottom_tab: BottomTab,
    play_session: Option<PlaySession>,
    preferences: Preferences,
    show_preferences: bool,
    recent_projects: Vec<PathBuf>,
    log_filter: Option<LogLevel>,
    last_autosave: Instant,
    /// Tracks the last theme applied to egui so we skip redundant set_visuals calls.
    last_applied_theme: Option<EditorTheme>,
}

impl EditorApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let wgpu_state = cc
            .wgpu_render_state
            .as_ref()
            .expect("rython-editor requires the wgpu renderer (--renderer wgpu)");

        let device: wgpu::Device = wgpu_state.device.clone();
        let queue: wgpu::Queue = wgpu_state.queue.clone();
        let adapter: wgpu::Adapter = wgpu_state.adapter.clone();
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());

        let gpu = GpuContext::from_existing(
            instance,
            adapter,
            device,
            queue,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            1,
        )
        .expect("GpuContext::from_existing failed");

        let mut renderer = RendererState::new(gpu, RendererConfig::default());

        // Upload built-in cube mesh so scene entities with mesh_id "cube" render correctly
        let cube = rython_resources::generate_cube();
        renderer.upload_mesh("cube", bytemuck::cast_slice(&cube.vertices), &cube.indices);
        // Upload built-in sphere mesh so scene entities with mesh_id "sphere" render correctly
        let sphere = rython_resources::generate_uv_sphere();
        renderer.upload_mesh("sphere", bytemuck::cast_slice(&sphere.vertices), &sphere.indices);

        let preferences = Preferences::load();
        let recent_projects = load_recent_projects();

        // Apply theme immediately
        preferences.apply_theme(&cc.egui_ctx);

        Self {
            renderer: Some(renderer),
            scene: Arc::new(Scene::new()),
            viewport_texture: None,
            viewport_state: ViewportState::new(),
            show_hierarchy: true,
            show_inspector: true,
            selection: SelectionState::default(),
            undo_stack: UndoStack::new(),
            project: ProjectState::default(),
            hierarchy_panel: SceneHierarchyPanel::new(),
            inspector_panel: ComponentInspectorPanel::new(),
            asset_browser: AssetBrowserPanel::new(),
            ui_editor: UiEditorPanel::new(),
            script_panel: ScriptPanel::new(),
            show_scripts: false,
            active_tab: EditorTab::Viewport,
            multi_select: Vec::new(),
            clipboard: Vec::new(),
            console_lines: Vec::new(),
            console_auto_scroll: true,
            show_console: false,
            bottom_tab: BottomTab::AssetBrowser,
            play_session: None,
            preferences,
            show_preferences: false,
            recent_projects,
            log_filter: None,
            last_autosave: Instant::now(),
            last_applied_theme: None,
        }
    }

    fn window_title(&self) -> String {
        let scene_part = match &self.project.open_scene_name {
            Some(n) => format!(" — {n}"),
            None => String::new(),
        };
        format!("Rython Editor | {}{scene_part}", self.project.title())
    }

    fn save_current_scene(&mut self) {
        let Some(root) = self.project.root_dir.clone() else {
            return;
        };
        let name = self
            .project
            .open_scene_name
            .clone()
            .unwrap_or_else(|| "default".to_string());
        if save_scene(&root, &name, &self.scene).is_ok() {
            self.project.open_scene_name = Some(name);
            self.project.mark_clean();
            self.console_lines
                .push(ConsoleEntry::new(LogLevel::Info, "Scene saved"));
        }
    }

    /// Open an existing project at `dir`, updating all relevant state.
    fn open_project_at(&mut self, dir: &Path) {
        if let Ok(config) = open_project(dir) {
            self.asset_browser.refresh(dir);
            self.script_panel.refresh(dir);
            self.project.root_dir = Some(dir.to_path_buf());
            self.project.config = config;
            self.project.open_scene_name = None;
            self.scene = Arc::new(Scene::new());
            self.undo_stack.clear();
            self.multi_select.clear();
            self.project.mark_clean();
            push_recent(&mut self.recent_projects, dir.to_path_buf());
            save_recent_projects(&self.recent_projects);
            if let Some(name) = self.project.config.default_scene.clone() {
                if load_scene(dir, &name, &self.scene).is_ok() {
                    self.project.open_scene_name = Some(name);
                }
            }
            self.console_lines.push(ConsoleEntry::new(
                LogLevel::Info,
                format!("Opened project: {}", dir.display()),
            ));
        } else {
            self.console_lines.push(ConsoleEntry::new(
                LogLevel::Error,
                format!("Failed to open project at: {}", dir.display()),
            ));
        }
    }

    /// Create a new project at `dir` with `name`.
    fn create_project_at(&mut self, dir: &Path, name: &str) {
        if let Ok(config) = create_project(dir, name) {
            self.asset_browser.refresh(dir);
            self.script_panel.refresh(dir);
            self.project.root_dir = Some(dir.to_path_buf());
            self.project.config = config;
            self.project.open_scene_name = None;
            self.scene = Arc::new(Scene::new());
            self.undo_stack.clear();
            self.multi_select.clear();
            self.project.mark_clean();
            push_recent(&mut self.recent_projects, dir.to_path_buf());
            save_recent_projects(&self.recent_projects);
            self.console_lines.push(ConsoleEntry::new(
                LogLevel::Info,
                format!("Created project: {name}"),
            ));
        }
    }

    fn start_play(&mut self) {
        if self.project.dirty {
            self.save_current_scene();
        }
        let Some(root) = self.project.root_dir.clone() else {
            self.console_lines
                .push(ConsoleEntry::new(LogLevel::Error, "No project open"));
            return;
        };
        let entry = self.project.config.entry_point.clone();
        self.play_session = PlaySession::launch(&root, entry.as_deref(), &mut self.console_lines);
        if self.play_session.is_some() {
            self.show_console = true;
            self.bottom_tab = BottomTab::Console;
            self.console_auto_scroll = true;
        }
    }

    fn stop_play(&mut self) {
        if let Some(mut session) = self.play_session.take() {
            session.stop(&mut self.console_lines);
        }
    }

    fn duplicate_selected(&mut self) {
        let Some(entity) = self.selection.selected_entity() else {
            return;
        };
        if !self.scene.entity_exists(entity) {
            return;
        }
        let new_id = EntityId::next();
        let comps = self.scene.components.snapshot_entity(entity);
        let parent = self.scene.hierarchy.get_parent(entity);
        let cmd = SpawnEntity::new(
            new_id,
            comps.into_iter().map(|(n, v)| (n.to_string(), v)).collect(),
            parent,
        );
        self.undo_stack.push(Box::new(cmd), &self.scene);
        self.selection.select_entity(new_id);
        self.multi_select.clear();
        self.project.dirty = true;
    }

    fn copy_selected(&mut self) {
        self.clipboard.clear();
        if let Some(entity) = self.selection.selected_entity() {
            if self.scene.entity_exists(entity) {
                self.clipboard
                    .push(EntitySnapshot::capture(entity, &self.scene));
            }
        }
        for &entity in &self.multi_select {
            if self.scene.entity_exists(entity)
                && !self.clipboard.iter().any(|s| s.entity == entity)
            {
                self.clipboard
                    .push(EntitySnapshot::capture(entity, &self.scene));
            }
        }
        if !self.clipboard.is_empty() {
            self.console_lines.push(ConsoleEntry::new(
                LogLevel::Info,
                format!("Copied {} entity/entities", self.clipboard.len()),
            ));
        }
    }

    fn paste_clipboard(&mut self) {
        if self.clipboard.is_empty() {
            return;
        }
        self.selection.clear();
        self.multi_select.clear();
        let snapshots = self.clipboard.clone();
        for snap in snapshots {
            let new_id = EntityId::next();
            let mut comps = snap.components.clone();
            // Offset X by +1 to visually distinguish from originals
            if let Some((_, v)) = comps.iter_mut().find(|(n, _)| n == "TransformComponent") {
                if let Some(x) = v.get("x").and_then(|x| x.as_f64()) {
                    v["x"] = serde_json::json!(x + 1.0);
                }
            }
            let cmd = SpawnEntity::new(new_id, comps, None);
            self.undo_stack.push(Box::new(cmd), &self.scene);
            self.multi_select.push(new_id);
            self.selection.select_entity(new_id);
        }
        self.project.dirty = true;
    }

    fn delete_selected(&mut self) {
        if !self.multi_select.is_empty() {
            let entities: Vec<EntityId> = self.multi_select.drain(..).collect();
            for entity in entities {
                if self.scene.entity_exists(entity) {
                    let cmd = DespawnEntity::capture(entity, &self.scene);
                    self.undo_stack.push(Box::new(cmd), &self.scene);
                }
            }
            self.selection.clear();
            self.project.dirty = true;
        } else if let Some(entity) = self.selection.selected_entity() {
            if self.scene.entity_exists(entity) {
                let cmd = DespawnEntity::capture(entity, &self.scene);
                self.undo_stack.push(Box::new(cmd), &self.scene);
                self.selection.clear();
                self.project.dirty = true;
            }
        }
    }

    fn focus_camera_on_selected(&mut self) {
        if let Some(entity) = self.selection.selected_entity() {
            if let Some(t) = self.scene.components.get::<TransformComponent>(entity) {
                self.viewport_state.camera_controller.target = Vec3::new(t.x, t.y, t.z);
            }
        }
    }

    /// Show the welcome screen (no project open).
    fn show_welcome(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);
            ui.heading("Welcome to Rython Editor");
            ui.add_space(20.0);

            ui.horizontal(|ui| {
                ui.add_space(ui.available_width() / 2.0 - 110.0);
                if ui
                    .button(egui::RichText::new("  New Project…  ").size(16.0))
                    .clicked()
                {
                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        let name = dir
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "Project".to_string());
                        let dir_clone = dir.clone();
                        self.create_project_at(&dir_clone, &name);
                    }
                }
                ui.add_space(8.0);
                if ui
                    .button(egui::RichText::new("  Open Project…  ").size(16.0))
                    .clicked()
                {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Project", &["json"])
                        .pick_file()
                    {
                        if let Some(dir) = path.parent() {
                            let dir = dir.to_path_buf();
                            self.open_project_at(&dir);
                        }
                    }
                }
            });

            if !self.recent_projects.is_empty() {
                ui.add_space(30.0);
                ui.separator();
                ui.add_space(10.0);
                ui.label(egui::RichText::new("Recent Projects").size(14.0));
                ui.add_space(6.0);

                let mut to_open: Option<usize> = None;
                for i in 0..self.recent_projects.len() {
                    let name = self.recent_projects[i]
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| self.recent_projects[i].to_string_lossy().to_string());
                    if ui
                        .link(format!("{name}  ({})", self.recent_projects[i].display()))
                        .clicked()
                    {
                        to_open = Some(i);
                    }
                }
                if let Some(idx) = to_open {
                    let dir = self.recent_projects[idx].clone();
                    self.open_project_at(&dir);
                }
            }
        });
    }

    /// Render the console panel contents inside the current Ui.
    fn show_console_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("Clear").clicked() {
                self.console_lines.clear();
            }
            ui.checkbox(&mut self.console_auto_scroll, "Auto-scroll");
            ui.separator();
            ui.label("Filter:");
            ui.selectable_value(&mut self.log_filter, None, "All");
            ui.selectable_value(&mut self.log_filter, Some(LogLevel::Info), "Info");
            ui.selectable_value(&mut self.log_filter, Some(LogLevel::Warn), "Warn");
            ui.selectable_value(&mut self.log_filter, Some(LogLevel::Error), "Error");
        });
        ui.separator();

        let auto_scroll = self.console_auto_scroll;
        let filter = self.log_filter;
        egui::ScrollArea::vertical()
            .stick_to_bottom(auto_scroll)
            .show(ui, |ui| {
                for entry in &self.console_lines {
                    if filter.is_none_or(|f| f == entry.level) {
                        let color = match entry.level {
                            LogLevel::Info => egui::Color32::LIGHT_GRAY,
                            LogLevel::Warn => egui::Color32::YELLOW,
                            LogLevel::Error => egui::Color32::from_rgb(255, 100, 100),
                        };
                        ui.colored_label(color, &entry.message);
                    }
                }
            });
    }

    /// Render the preferences dialog window if open.
    fn show_preferences_window(&mut self, ctx: &egui::Context) {
        let mut show = self.show_preferences;
        egui::Window::new("Preferences")
            .open(&mut show)
            .resizable(true)
            .collapsible(false)
            .min_width(340.0)
            .show(ctx, |ui| {
                egui::Grid::new("prefs_grid")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Theme:");
                        ui.horizontal(|ui| {
                            ui.selectable_value(
                                &mut self.preferences.theme,
                                EditorTheme::Dark,
                                "Dark",
                            );
                            ui.selectable_value(
                                &mut self.preferences.theme,
                                EditorTheme::Light,
                                "Light",
                            );
                        });
                        ui.end_row();

                        ui.label("Font size:");
                        ui.add(
                            egui::Slider::new(&mut self.preferences.font_size, 10.0..=20.0)
                                .suffix(" pt"),
                        );
                        ui.end_row();

                        ui.label("Grid spacing:");
                        ui.add(
                            egui::DragValue::new(&mut self.preferences.grid_spacing)
                                .speed(0.1)
                                .range(0.1..=100.0_f32),
                        );
                        ui.end_row();

                        ui.label("Auto-save:");
                        egui::ComboBox::from_id_salt("autosave_combo")
                            .selected_text(match self.preferences.auto_save_secs {
                                0 => "Off",
                                60 => "1 min",
                                300 => "5 min",
                                600 => "10 min",
                                _ => "Custom",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.preferences.auto_save_secs, 0, "Off");
                                ui.selectable_value(
                                    &mut self.preferences.auto_save_secs,
                                    60,
                                    "1 min",
                                );
                                ui.selectable_value(
                                    &mut self.preferences.auto_save_secs,
                                    300,
                                    "5 min",
                                );
                                ui.selectable_value(
                                    &mut self.preferences.auto_save_secs,
                                    600,
                                    "10 min",
                                );
                            });
                        ui.end_row();

                        ui.label("Default gizmo:");
                        egui::ComboBox::from_id_salt("gizmo_combo")
                            .selected_text(match self.preferences.default_gizmo_mode {
                                1 => "Rotate",
                                2 => "Scale",
                                _ => "Translate",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.preferences.default_gizmo_mode,
                                    0,
                                    "Translate",
                                );
                                ui.selectable_value(
                                    &mut self.preferences.default_gizmo_mode,
                                    1,
                                    "Rotate",
                                );
                                ui.selectable_value(
                                    &mut self.preferences.default_gizmo_mode,
                                    2,
                                    "Scale",
                                );
                            });
                        ui.end_row();

                        ui.label("External editor:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.preferences.external_editor)
                                .hint_text("auto-detect"),
                        );
                        ui.end_row();
                    });

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        self.preferences.save();
                        self.show_preferences = false;
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_preferences = false;
                    }
                });
            });
        self.show_preferences = show && self.show_preferences;
    }
}

impl eframe::App for EditorApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // ── Apply preferences (only when theme actually changes) ─────────────
        if self.last_applied_theme != Some(self.preferences.theme) {
            self.preferences.apply_theme(ctx);
            self.last_applied_theme = Some(self.preferences.theme);
        }

        // ── Poll play session ─────────────────────────────────────────────────
        if let Some(session) = &mut self.play_session {
            session.poll(&mut self.console_lines);
            if !session.is_running() {
                // Drain final output before dropping
                while let Ok((level, line)) = session.output_rx.try_recv() {
                    self.console_lines.push(ConsoleEntry::new(level, line));
                }
                self.console_lines
                    .push(ConsoleEntry::new(LogLevel::Info, "■ Game exited"));
                self.play_session = None;
            }
        }

        // ── Auto-save ─────────────────────────────────────────────────────────
        if self.preferences.auto_save_secs > 0
            && self.project.dirty
            && self.project.root_dir.is_some()
            && self.last_autosave.elapsed().as_secs() >= self.preferences.auto_save_secs
        {
            self.save_current_scene();
            self.last_autosave = Instant::now();
        }

        // ── Global keyboard shortcuts ─────────────────────────────────────────
        let undo_pressed =
            ctx.input(|i| i.key_pressed(egui::Key::Z) && i.modifiers.ctrl && !i.modifiers.shift);
        let redo_pressed =
            ctx.input(|i| i.key_pressed(egui::Key::Z) && i.modifiers.ctrl && i.modifiers.shift);
        let save_pressed =
            ctx.input(|i| i.key_pressed(egui::Key::S) && i.modifiers.ctrl && !i.modifiers.shift);
        let save_as_pressed =
            ctx.input(|i| i.key_pressed(egui::Key::S) && i.modifiers.ctrl && i.modifiers.shift);
        let new_project_pressed = ctx.input(|i| i.key_pressed(egui::Key::N) && i.modifiers.ctrl);
        let open_project_pressed = ctx.input(|i| i.key_pressed(egui::Key::O) && i.modifiers.ctrl);
        let prefs_pressed = ctx.input(|i| i.key_pressed(egui::Key::Comma) && i.modifiers.ctrl);
        let play_pressed = ctx.input(|i| i.key_pressed(egui::Key::F5) && !i.modifiers.shift);
        let stop_pressed = ctx.input(|i| i.key_pressed(egui::Key::F5) && i.modifiers.shift);

        // Entity shortcuts (not when typing in a text field)
        let delete_pressed =
            !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::Delete));
        let duplicate_pressed = !ctx.wants_keyboard_input()
            && ctx.input(|i| i.key_pressed(egui::Key::D) && i.modifiers.ctrl);
        let copy_pressed = !ctx.wants_keyboard_input()
            && ctx.input(|i| i.key_pressed(egui::Key::C) && i.modifiers.ctrl);
        let paste_pressed = !ctx.wants_keyboard_input()
            && ctx.input(|i| i.key_pressed(egui::Key::V) && i.modifiers.ctrl);

        // ── Viewport shortcuts (only when not typing, only in Viewport tab) ───
        if !ctx.wants_keyboard_input() && self.active_tab == EditorTab::Viewport {
            let vs = &mut self.viewport_state;
            if ctx.input(|i| i.key_pressed(egui::Key::W)) {
                vs.gizmo_mode = GizmoMode::Translate;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::E)) {
                vs.gizmo_mode = GizmoMode::Rotate;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::R)) {
                vs.gizmo_mode = GizmoMode::Scale;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::X)) {
                use crate::viewport::gizmo::GizmoSpace;
                vs.gizmo_space = match vs.gizmo_space {
                    GizmoSpace::World => GizmoSpace::Local,
                    GizmoSpace::Local => GizmoSpace::World,
                };
            }
            if ctx.input(|i| i.key_pressed(egui::Key::G)) {
                vs.show_grid = !vs.show_grid;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                if let Some(drag) = vs.active_drag.take() {
                    self.scene
                        .components
                        .insert(drag.entity, drag.initial_transform.clone());
                }
            }
            // Camera view presets (Numpad equivalents via digit row)
            if ctx.input(|i| i.key_pressed(egui::Key::Num1) && !i.modifiers.ctrl) {
                // Front view: look along -Z, yaw=0, pitch=0
                let vs = &mut self.viewport_state;
                vs.camera_controller.yaw = 0.0;
                vs.camera_controller.pitch = 0.0;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Num3) && !i.modifiers.ctrl) {
                // Right view: look along -X
                let vs = &mut self.viewport_state;
                vs.camera_controller.yaw = std::f32::consts::FRAC_PI_2;
                vs.camera_controller.pitch = 0.0;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Num7) && !i.modifiers.ctrl) {
                // Top view: look along -Y
                let vs = &mut self.viewport_state;
                vs.camera_controller.yaw = 0.0;
                vs.camera_controller.pitch = std::f32::consts::FRAC_PI_2 - 0.001;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Num0) && !i.modifiers.ctrl) {
                // Reset to default position
                let vs = &mut self.viewport_state;
                vs.camera_controller = crate::viewport::camera_controller::CameraController::new();
            }
        }

        // Focus camera (F) — can fire from any tab if entity selected
        if !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::F)) {
            self.focus_camera_on_selected();
        }

        // ── Apply shortcuts ───────────────────────────────────────────────────
        if undo_pressed {
            self.undo_stack.undo(&self.scene);
        }
        if redo_pressed {
            self.undo_stack.redo(&self.scene);
        }
        if save_pressed {
            self.save_current_scene();
        }
        if save_as_pressed && self.project.root_dir.is_some() {
            let root = self.project.root_dir.clone().unwrap();
            if let Some(path) = rfd::FileDialog::new()
                .set_directory(root.join("scenes"))
                .add_filter("Scene", &["json"])
                .save_file()
            {
                let name = path
                    .file_stem()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "scene".to_string());
                let _ = save_scene(&root, &name, &self.scene);
                self.project.open_scene_name = Some(name);
                self.project.mark_clean();
            }
        }
        if new_project_pressed {
            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                let name = dir
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Project".to_string());
                let dir_clone = dir.clone();
                self.create_project_at(&dir_clone, &name);
            }
        }
        if open_project_pressed {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Project", &["json"])
                .pick_file()
            {
                if let Some(dir) = path.parent() {
                    let dir = dir.to_path_buf();
                    self.open_project_at(&dir);
                }
            }
        }
        if prefs_pressed {
            self.show_preferences = true;
        }
        if play_pressed && self.play_session.is_none() {
            self.start_play();
        }
        if stop_pressed && self.play_session.is_some() {
            self.stop_play();
        }
        if delete_pressed {
            self.delete_selected();
        }
        if duplicate_pressed {
            self.duplicate_selected();
        }
        if copy_pressed {
            self.copy_selected();
        }
        if paste_pressed {
            self.paste_clipboard();
        }

        // ── Window title ──────────────────────────────────────────────────────
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));

        // ── Preferences dialog ────────────────────────────────────────────────
        if self.show_preferences {
            self.show_preferences_window(ctx);
        }

        // ── Top menu bar ──────────────────────────────────────────────────────
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New Project…  Ctrl+N").clicked() {
                        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                            let name = dir
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| "Project".to_string());
                            let dir_clone = dir.clone();
                            self.create_project_at(&dir_clone, &name);
                        }
                        ui.close_menu();
                    }
                    if ui.button("Open Project…  Ctrl+O").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Project", &["json"])
                            .pick_file()
                        {
                            if let Some(dir) = path.parent() {
                                let dir = dir.to_path_buf();
                                self.open_project_at(&dir);
                            }
                        }
                        ui.close_menu();
                    }

                    // Recent Projects submenu
                    if !self.recent_projects.is_empty() {
                        ui.menu_button("Recent Projects", |ui| {
                            let mut to_open: Option<usize> = None;
                            for i in 0..self.recent_projects.len() {
                                let label = self.recent_projects[i]
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_else(|| {
                                        self.recent_projects[i].to_string_lossy().to_string()
                                    });
                                if ui.button(label).clicked() {
                                    to_open = Some(i);
                                    ui.close_menu();
                                }
                            }
                            if let Some(idx) = to_open {
                                let dir = self.recent_projects[idx].clone();
                                self.open_project_at(&dir);
                            }
                        });
                    }

                    // Scenes submenu
                    if let Some(root) = self.project.root_dir.clone() {
                        let scenes = list_scenes(&root);
                        if !scenes.is_empty() {
                            ui.menu_button("Open Scene", |ui| {
                                let mut to_load: Option<String> = None;
                                for name in &scenes {
                                    if ui.button(name).clicked() {
                                        to_load = Some(name.clone());
                                        ui.close_menu();
                                    }
                                }
                                if let Some(name) = to_load {
                                    let new_scene = Arc::new(Scene::new());
                                    if load_scene(&root, &name, &new_scene).is_ok() {
                                        self.scene = new_scene;
                                        self.project.open_scene_name = Some(name.clone());
                                        self.undo_stack.clear();
                                        self.selection.clear();
                                        self.multi_select.clear();
                                        self.console_lines.push(ConsoleEntry::new(
                                            LogLevel::Info,
                                            format!("Opened scene: {name}"),
                                        ));
                                    }
                                }
                            });
                        }
                    }

                    ui.separator();
                    if ui.button("Save Scene  Ctrl+S").clicked() {
                        self.save_current_scene();
                        ui.close_menu();
                    }
                    if ui.button("Save Scene As…  Ctrl+Shift+S").clicked() {
                        if let Some(root) = self.project.root_dir.clone() {
                            if let Some(path) = rfd::FileDialog::new()
                                .set_directory(root.join("scenes"))
                                .add_filter("Scene", &["json"])
                                .save_file()
                            {
                                let name = path
                                    .file_stem()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_else(|| "scene".to_string());
                                let _ = save_scene(&root, &name, &self.scene);
                                self.project.open_scene_name = Some(name);
                                self.project.mark_clean();
                            }
                        }
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button("Edit", |ui| {
                    let can_undo = self.undo_stack.can_undo();
                    if ui
                        .add_enabled(can_undo, egui::Button::new("Undo  Ctrl+Z"))
                        .clicked()
                    {
                        self.undo_stack.undo(&self.scene);
                        ui.close_menu();
                    }
                    let can_redo = self.undo_stack.can_redo();
                    if ui
                        .add_enabled(can_redo, egui::Button::new("Redo  Ctrl+Shift+Z"))
                        .clicked()
                    {
                        self.undo_stack.redo(&self.scene);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Copy  Ctrl+C").clicked() {
                        self.copy_selected();
                        ui.close_menu();
                    }
                    let can_paste = !self.clipboard.is_empty();
                    if ui
                        .add_enabled(can_paste, egui::Button::new("Paste  Ctrl+V"))
                        .clicked()
                    {
                        self.paste_clipboard();
                        ui.close_menu();
                    }
                    if ui.button("Duplicate  Ctrl+D").clicked() {
                        self.duplicate_selected();
                        ui.close_menu();
                    }
                    if ui.button("Delete  Del").clicked() {
                        self.delete_selected();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Preferences…  Ctrl+,").clicked() {
                        self.show_preferences = true;
                        ui.close_menu();
                    }
                });

                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut self.show_hierarchy, "Hierarchy");
                    ui.checkbox(&mut self.show_inspector, "Inspector");
                    ui.checkbox(&mut self.show_scripts, "Scripts");
                    ui.checkbox(&mut self.show_console, "Console");
                    ui.separator();
                    if ui.button("Reset Layout").clicked() {
                        self.show_hierarchy = true;
                        self.show_inspector = true;
                        self.show_scripts = false;
                        self.show_console = false;
                        self.active_tab = EditorTab::Viewport;
                        self.bottom_tab = BottomTab::AssetBrowser;
                        ui.close_menu();
                    }
                });

                // Gizmo mode (Viewport only)
                ui.separator();
                ui.label("Gizmo:");
                let vs = &mut self.viewport_state;
                ui.selectable_value(&mut vs.gizmo_mode, GizmoMode::Translate, "Translate (W)");
                ui.selectable_value(&mut vs.gizmo_mode, GizmoMode::Rotate, "Rotate (E)");
                ui.selectable_value(&mut vs.gizmo_mode, GizmoMode::Scale, "Scale (R)");

                // Central panel tabs
                ui.separator();
                ui.selectable_value(&mut self.active_tab, EditorTab::Viewport, "Scene");
                ui.selectable_value(&mut self.active_tab, EditorTab::UiEditor, "UI Editor");

                // Play / Stop button
                ui.separator();
                if self.project.root_dir.is_some() {
                    if self.play_session.is_none() {
                        if ui
                            .button(
                                egui::RichText::new("▶ Play")
                                    .color(egui::Color32::from_rgb(100, 220, 100)),
                            )
                            .on_hover_text("F5")
                            .clicked()
                        {
                            self.start_play();
                        }
                    } else {
                        if ui
                            .button(
                                egui::RichText::new("■ Stop")
                                    .color(egui::Color32::from_rgb(220, 100, 100)),
                            )
                            .on_hover_text("Shift+F5")
                            .clicked()
                        {
                            self.stop_play();
                        }
                        ui.label(
                            egui::RichText::new("Playing…")
                                .color(egui::Color32::from_rgb(100, 220, 100))
                                .small(),
                        );
                    }
                }
            });
        });

        // ── Bottom panel — Asset Browser | Console ────────────────────────────
        egui::TopBottomPanel::bottom("bottom_panel")
            .resizable(true)
            .min_height(120.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut self.bottom_tab,
                        BottomTab::AssetBrowser,
                        "Asset Browser",
                    );
                    if self.show_console {
                        ui.selectable_value(&mut self.bottom_tab, BottomTab::Console, "Console");
                    }
                });
                ui.separator();

                match self.bottom_tab {
                    BottomTab::AssetBrowser => {
                        let root_clone = self.project.root_dir.clone();
                        self.asset_browser
                            .show(ui, root_clone.as_ref(), &mut self.selection);
                    }
                    BottomTab::Console => {
                        self.show_console_ui(ui);
                    }
                }
            });

        // ── Left panel — Hierarchy + Scripts ──────────────────────────────────
        if self.show_hierarchy || self.show_scripts {
            egui::SidePanel::left("hierarchy")
                .resizable(true)
                .min_width(180.0)
                .show(ctx, |ui| {
                    if self.show_hierarchy {
                        let scene = Arc::clone(&self.scene);
                        self.hierarchy_panel.show(
                            ui,
                            &scene,
                            &mut self.selection,
                            &mut self.undo_stack,
                            &mut self.project.dirty,
                        );
                    }
                    if self.show_scripts {
                        if self.show_hierarchy {
                            ui.separator();
                        }
                        let root_clone = self.project.root_dir.clone();
                        self.script_panel.show(
                            ui,
                            root_clone.as_ref(),
                            &mut self.project.config,
                            &self.selection,
                            &mut self.project.dirty,
                        );
                    }
                });
        }

        // ── Right panel — Component Inspector ────────────────────────────────
        if self.show_inspector {
            egui::SidePanel::right("inspector")
                .resizable(true)
                .min_width(200.0)
                .show(ctx, |ui| {
                    if let Some(entity) = self.selection.selected_entity() {
                        if self.scene.entity_exists(entity) {
                            let scene = Arc::clone(&self.scene);
                            self.inspector_panel.show(
                                ui,
                                entity,
                                &scene,
                                &mut self.undo_stack,
                                &mut self.project.dirty,
                                &self.asset_browser,
                            );
                        } else {
                            self.selection.clear();
                            ui.heading("Inspector");
                            ui.separator();
                            ui.label("Select an entity to inspect");
                        }
                    } else if !self.multi_select.is_empty() {
                        ui.heading("Inspector");
                        ui.separator();
                        ui.label(format!("{} entities selected", self.multi_select.len()));
                        ui.small("(Multi-select: shared operations via Edit menu)");
                    } else {
                        ui.heading("Inspector");
                        ui.separator();
                        ui.label("Select an entity to inspect");
                    }
                });
        }

        // ── Central panel — Viewport, UI Editor, or Welcome ───────────────────
        let render_state_opt = frame.wgpu_render_state();

        egui::CentralPanel::default().show(ctx, |ui| {
            // Show welcome screen when no project is open
            if self.project.root_dir.is_none() {
                self.show_welcome(ui);
                return;
            }

            match self.active_tab {
                EditorTab::Viewport => {
                    let Some(render_state) = render_state_opt else {
                        ui.label("Renderer unavailable");
                        ctx.request_repaint();
                        return;
                    };

                    let viewport_rect = ui.available_rect_before_wrap();

                    let viewport_response = if let Some(renderer) = self.renderer.as_mut() {
                        viewport_panel::show(
                            ui,
                            render_state,
                            renderer,
                            &self.scene,
                            &mut self.viewport_texture,
                            &mut self.viewport_state.camera,
                        )
                    } else {
                        ui.label("Renderer unavailable")
                    };

                    // Compute the view-projection matrix once per frame for all
                    // gizmo hit-tests (drag start, click picking, hover overlay).
                    let gizmo_vp = self.viewport_state.camera.view_projection();

                    // ── Gizmo drag start ──────────────────────────────────────
                    if viewport_response.drag_started() {
                        if let Some(mouse_pos) = viewport_response.interact_pointer_pos() {
                            if let Some(entity) = self.selection.selected_entity() {
                                if let Some(transform) =
                                    self.scene.components.get::<TransformComponent>(entity)
                                {
                                    let hit = hit_test_gizmo_with_vp(
                                        self.viewport_state.gizmo_mode,
                                        &transform,
                                        gizmo_vp,
                                        viewport_rect,
                                        mouse_pos,
                                    );
                                    if let Some(axis) = hit {
                                        self.viewport_state.active_drag = Some(GizmoDrag {
                                            axis,
                                            entity,
                                            initial_transform: transform.clone(),
                                            start_mouse: mouse_pos,
                                            origin_screen: mouse_pos,
                                        });
                                    }
                                }
                            }
                        }
                    }

                    // ── Gizmo drag update ─────────────────────────────────────
                    if viewport_response.dragged() {
                        if let Some(drag) = &self.viewport_state.active_drag {
                            if let Some(mouse_pos) = viewport_response.interact_pointer_pos() {
                                let entity = drag.entity;
                                let mut new_t = drag.initial_transform.clone();

                                match self.viewport_state.gizmo_mode {
                                    GizmoMode::Translate => {
                                        let (x, y, z) = apply_translate_drag(
                                            drag,
                                            mouse_pos,
                                            &self.viewport_state.camera,
                                            viewport_rect,
                                        );
                                        new_t.x = x;
                                        new_t.y = y;
                                        new_t.z = z;
                                    }
                                    GizmoMode::Rotate => {
                                        let (rx, ry, rz) = apply_rotate_drag(
                                            drag,
                                            mouse_pos,
                                            &self.viewport_state.camera,
                                            viewport_rect,
                                        );
                                        new_t.rot_x = rx;
                                        new_t.rot_y = ry;
                                        new_t.rot_z = rz;
                                    }
                                    GizmoMode::Scale => {
                                        let (sx, sy, sz) = apply_scale_drag(
                                            drag,
                                            mouse_pos,
                                            &self.viewport_state.camera,
                                            viewport_rect,
                                        );
                                        new_t.scale_x = sx;
                                        new_t.scale_y = sy;
                                        new_t.scale_z = sz;
                                    }
                                }

                                self.scene.components.insert(entity, new_t);
                            }
                        }
                    }

                    // ── Gizmo drag end → push undo ────────────────────────────
                    if viewport_response.drag_stopped() {
                        if let Some(drag) = self.viewport_state.active_drag.take() {
                            if let Some(current_t) =
                                self.scene.components.get::<TransformComponent>(drag.entity)
                            {
                                let old_json =
                                    serde_json::to_value(&drag.initial_transform).unwrap();
                                let new_json = serde_json::to_value(&current_t).unwrap();
                                if old_json != new_json {
                                    let cmd = ModifyComponent {
                                        entity: drag.entity,
                                        type_name: "TransformComponent".to_string(),
                                        old_value: old_json,
                                        new_value: new_json,
                                    };
                                    self.undo_stack.push_no_execute(Box::new(cmd));
                                    self.project.dirty = true;
                                }
                            }
                        }
                    }

                    // ── Camera controller (skip if gizmo active) ──────────────
                    if self.viewport_state.active_drag.is_none() {
                        let vs = &mut self.viewport_state;
                        vs.camera_controller
                            .update(&viewport_response, &mut vs.camera);
                    }

                    // ── Ray-cast picking on click ─────────────────────────────
                    if viewport_response.clicked() && !viewport_response.dragged() {
                        if let Some(click_pos) = viewport_response.interact_pointer_pos() {
                            use crate::viewport::picking::{pick_entity, ray_from_viewport_click};
                            let ray = ray_from_viewport_click(
                                click_pos,
                                viewport_rect,
                                &self.viewport_state.camera,
                            );
                            let ctrl_held = ctx.input(|i| i.modifiers.ctrl);
                            let scene = Arc::clone(&self.scene);

                            if let Some(hit) = pick_entity(&ray, &scene) {
                                if ctrl_held {
                                    // Ctrl+Click: toggle multi-select
                                    if let Some(pos) =
                                        self.multi_select.iter().position(|&e| e == hit)
                                    {
                                        self.multi_select.remove(pos);
                                    } else {
                                        self.multi_select.push(hit);
                                    }
                                } else {
                                    self.selection.select_entity(hit);
                                    self.multi_select.clear();
                                }
                            } else {
                                let on_gizmo = self
                                    .selection
                                    .selected_entity()
                                    .and_then(|e| {
                                        self.scene.components.get::<TransformComponent>(e)
                                    })
                                    .and_then(|t| {
                                        hit_test_gizmo_with_vp(
                                            self.viewport_state.gizmo_mode,
                                            &t,
                                            gizmo_vp,
                                            viewport_rect,
                                            click_pos,
                                        )
                                    })
                                    .is_some();
                                if !on_gizmo && !ctrl_held {
                                    self.selection.clear();
                                    self.multi_select.clear();
                                }
                            }
                        }
                    }

                    // ── Gizmo overlay ─────────────────────────────────────────
                    if let Some(entity) = self.selection.selected_entity() {
                        if let Some(transform) =
                            self.scene.components.get::<TransformComponent>(entity)
                        {
                            let painter = ui.painter_at(viewport_rect);

                            let hovered_axis = if self.viewport_state.active_drag.is_none() {
                                viewport_response.hover_pos().and_then(|pos| {
                                    hit_test_gizmo_with_vp(
                                        self.viewport_state.gizmo_mode,
                                        &transform,
                                        gizmo_vp,
                                        viewport_rect,
                                        pos,
                                    )
                                })
                            } else {
                                self.viewport_state.active_drag.as_ref().map(|d| d.axis)
                            };

                            draw_gizmo(
                                &painter,
                                self.viewport_state.gizmo_mode,
                                entity,
                                &transform,
                                &self.viewport_state.camera,
                                viewport_rect,
                                hovered_axis,
                                self.viewport_state.active_drag.as_ref(),
                            );
                        }
                    }
                }

                EditorTab::UiEditor => {
                    let root_clone = self.project.root_dir.clone();
                    self.ui_editor
                        .show(ui, root_clone.as_ref(), &mut self.selection);
                }
            }
        });

        // Only request continuous repaints when something is actively animating.
        // When the editor is idle, egui will still repaint on user input events.
        if self.play_session.is_some()
            || self.viewport_state.active_drag.is_some()
            || self.viewport_state.camera_controller.is_moving()
        {
            ctx.request_repaint();
        }
    }
}

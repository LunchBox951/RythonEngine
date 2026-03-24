use std::sync::Arc;

use rython_ecs::Scene;
use rython_renderer::{Camera, GpuContext, RendererConfig, RendererState};

use crate::panels::component_inspector::ComponentInspectorPanel;
use crate::panels::scene_hierarchy::SceneHierarchyPanel;
use crate::panels::viewport_panel;
use crate::project::io::{create_project, open_project, save_scene};
use crate::state::{ProjectState, SelectionState, UndoStack};
use crate::viewport::{CameraController, ViewportTexture};

pub struct EditorApp {
    renderer: Option<RendererState>,
    scene: Arc<Scene>,
    viewport_texture: Option<ViewportTexture>,
    viewport_camera: Camera,
    camera_controller: CameraController,
    show_hierarchy: bool,
    show_inspector: bool,

    // Phase 2 state
    selection: SelectionState,
    undo_stack: UndoStack,
    project: ProjectState,
    hierarchy_panel: SceneHierarchyPanel,
    inspector_panel: ComponentInspectorPanel,
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

        let renderer = RendererState::new(gpu, RendererConfig::default());

        let mut camera = Camera::new();
        camera.set_position(0.0, 5.0, -10.0);
        camera.set_look_at(0.0, 0.0, 0.0);
        camera.fov_degrees = 60.0;

        Self {
            renderer: Some(renderer),
            scene: Arc::new(Scene::new()),
            viewport_texture: None,
            viewport_camera: camera,
            camera_controller: CameraController::new(),
            show_hierarchy: true,
            show_inspector: true,
            selection: SelectionState::default(),
            undo_stack: UndoStack::new(),
            project: ProjectState::default(),
            hierarchy_panel: SceneHierarchyPanel::new(),
            inspector_panel: ComponentInspectorPanel::new(),
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
        let Some(root) = &self.project.root_dir.clone() else {
            return;
        };
        let name = self
            .project
            .open_scene_name
            .clone()
            .unwrap_or_else(|| "default".to_string());
        if save_scene(root, &name, &self.scene).is_ok() {
            self.project.open_scene_name = Some(name);
            self.project.mark_clean();
        }
    }
}

impl eframe::App for EditorApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // ── Keyboard shortcuts ────────────────────────────────────────────────
        let undo_pressed = ctx.input(|i| {
            i.key_pressed(egui::Key::Z) && i.modifiers.ctrl && !i.modifiers.shift
        });
        let redo_pressed = ctx.input(|i| {
            i.key_pressed(egui::Key::Z) && i.modifiers.ctrl && i.modifiers.shift
        });
        let save_pressed = ctx.input(|i| i.key_pressed(egui::Key::S) && i.modifiers.ctrl);

        if undo_pressed {
            self.undo_stack.undo(&self.scene);
        }
        if redo_pressed {
            self.undo_stack.redo(&self.scene);
        }
        if save_pressed {
            self.save_current_scene();
        }

        // Update title bar
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));

        // ── Top menu bar ──────────────────────────────────────────────────────
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New Project…").clicked() {
                        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                            let name = dir
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| "Project".to_string());
                            if let Ok(config) = create_project(&dir, &name) {
                                self.project.root_dir = Some(dir);
                                self.project.config = config;
                                self.project.open_scene_name = None;
                                self.scene = Arc::new(Scene::new());
                                self.undo_stack.clear();
                                self.project.mark_clean();
                            }
                        }
                        ui.close_menu();
                    }
                    if ui.button("Open Project…").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Project", &["json"])
                            .pick_file()
                        {
                            if let Some(dir) = path.parent() {
                                if let Ok(config) = open_project(dir) {
                                    self.project.root_dir = Some(dir.to_path_buf());
                                    self.project.config = config;
                                    self.project.open_scene_name = None;
                                    self.scene = Arc::new(Scene::new());
                                    self.undo_stack.clear();
                                    self.project.mark_clean();
                                }
                            }
                        }
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Save Scene  Ctrl+S").clicked() {
                        self.save_current_scene();
                        ui.close_menu();
                    }
                    if ui.button("Save Scene As…").clicked() {
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
                });

                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut self.show_hierarchy, "Hierarchy");
                    ui.checkbox(&mut self.show_inspector, "Inspector");
                });
            });
        });

        // ── Bottom panel ──────────────────────────────────────────────────────
        egui::TopBottomPanel::bottom("asset_browser")
            .resizable(true)
            .min_height(80.0)
            .show(ctx, |ui| {
                ui.heading("Asset Browser");
                ui.label("(Phase 3)");
            });

        // ── Left panel — scene hierarchy ──────────────────────────────────────
        if self.show_hierarchy {
            egui::SidePanel::left("hierarchy")
                .resizable(true)
                .min_width(180.0)
                .show(ctx, |ui| {
                    let scene = Arc::clone(&self.scene);
                    self.hierarchy_panel.show(
                        ui,
                        &scene,
                        &mut self.selection,
                        &mut self.undo_stack,
                        &mut self.project.dirty,
                    );
                });
        }

        // ── Right panel — component inspector ────────────────────────────────
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
                            );
                        } else {
                            self.selection.clear();
                            ui.heading("Inspector");
                            ui.separator();
                            ui.label("Select an entity to inspect");
                        }
                    } else {
                        ui.heading("Inspector");
                        ui.separator();
                        ui.label("Select an entity to inspect");
                    }
                });
        }

        // ── Central panel — 3D viewport ───────────────────────────────────────
        // Extract render_state and renderer at the point of use so they don't
        // conflict with the self borrows in closures above.
        let Some(render_state) = frame.wgpu_render_state() else {
            ctx.request_repaint();
            return;
        };

        egui::CentralPanel::default().show(ctx, |ui| {
            let viewport_rect = ui.available_rect_before_wrap();

            let viewport_response = if let Some(renderer) = self.renderer.as_mut() {
                viewport_panel::show(
                    ui,
                    render_state,
                    renderer,
                    &self.scene,
                    &mut self.viewport_texture,
                    &mut self.viewport_camera,
                )
            } else {
                ui.label("Renderer unavailable")
            };

            self.camera_controller
                .update(&viewport_response, &mut self.viewport_camera);

            // Ray-cast picking on click (not drag)
            if viewport_response.clicked() && !viewport_response.dragged() {
                if let Some(click_pos) = viewport_response.interact_pointer_pos() {
                    use crate::viewport::picking::{pick_entity, ray_from_viewport_click};
                    let ray = ray_from_viewport_click(
                        click_pos,
                        viewport_rect,
                        &self.viewport_camera,
                    );
                    let scene = Arc::clone(&self.scene);
                    if let Some(hit) = pick_entity(&ray, &scene) {
                        self.selection.select_entity(hit);
                    } else {
                        self.selection.clear();
                    }
                }
            }
        });

        ctx.request_repaint();
    }
}

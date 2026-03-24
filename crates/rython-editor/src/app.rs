use std::sync::Arc;

use rython_ecs::component::TransformComponent;
use rython_ecs::Scene;
use rython_renderer::{GpuContext, RendererConfig, RendererState};

use crate::panels::asset_browser::AssetBrowserPanel;
use crate::panels::component_inspector::ComponentInspectorPanel;
use crate::panels::scene_hierarchy::SceneHierarchyPanel;
use crate::panels::viewport_panel;
use crate::project::io::{create_project, open_project, save_scene};
use crate::state::undo::ModifyComponent;
use crate::state::{ProjectState, SelectionState, UndoStack, ViewportState};
use crate::viewport::gizmo::{
    apply_rotate_drag, apply_scale_drag, apply_translate_drag, draw_gizmo, hit_test_gizmo,
    GizmoDrag, GizmoMode,
};
use crate::viewport::ViewportTexture;

pub struct EditorApp {
    renderer: Option<RendererState>,
    scene: Arc<Scene>,
    viewport_texture: Option<ViewportTexture>,
    viewport_state: ViewportState,
    show_hierarchy: bool,
    show_inspector: bool,

    // Phase 2 state
    selection: SelectionState,
    undo_stack: UndoStack,
    project: ProjectState,
    hierarchy_panel: SceneHierarchyPanel,
    inspector_panel: ComponentInspectorPanel,

    // Phase 3
    asset_browser: AssetBrowserPanel,
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

        // Gizmo mode shortcuts (only when not in a text field)
        if !ctx.wants_keyboard_input() {
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
            // Escape cancels active gizmo drag — restore initial transform
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                if let Some(drag) = vs.active_drag.take() {
                    self.scene.components.insert(drag.entity, drag.initial_transform.clone());
                }
            }
        }

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
                                self.asset_browser.refresh(&dir);
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
                                    self.asset_browser.refresh(dir);
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

                // Gizmo mode toolbar
                ui.separator();
                ui.label("Gizmo:");
                let vs = &mut self.viewport_state;
                ui.selectable_value(&mut vs.gizmo_mode, GizmoMode::Translate, "Translate (W)");
                ui.selectable_value(&mut vs.gizmo_mode, GizmoMode::Rotate, "Rotate (E)");
                ui.selectable_value(&mut vs.gizmo_mode, GizmoMode::Scale, "Scale (R)");
            });
        });

        // ── Bottom panel — asset browser ──────────────────────────────────────
        egui::TopBottomPanel::bottom("asset_browser_panel")
            .resizable(true)
            .min_height(120.0)
            .show(ctx, |ui| {
                let root_clone = self.project.root_dir.clone();
                self.asset_browser.show(ui, root_clone.as_ref(), &mut self.selection);
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
                                &self.asset_browser,
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
                    &mut self.viewport_state.camera,
                )
            } else {
                ui.label("Renderer unavailable")
            };

            // ── Gizmo drag start ──────────────────────────────────────────────
            if viewport_response.drag_started() {
                if let Some(mouse_pos) = viewport_response.interact_pointer_pos() {
                    if let Some(entity) = self.selection.selected_entity() {
                        if let Some(transform) =
                            self.scene.components.get::<TransformComponent>(entity)
                        {
                            let hit = hit_test_gizmo(
                                self.viewport_state.gizmo_mode,
                                &transform,
                                &self.viewport_state.camera,
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

            // ── Gizmo drag update (live transform) ────────────────────────────
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

                        // Apply live (no undo push yet — undo on release)
                        self.scene.components.insert(entity, new_t);
                    }
                }
            }

            // ── Gizmo drag end → push undo command ───────────────────────────
            if viewport_response.drag_stopped() {
                if let Some(drag) = self.viewport_state.active_drag.take() {
                    if let Some(current_t) =
                        self.scene.components.get::<TransformComponent>(drag.entity)
                    {
                        let old_json = serde_json::to_value(&drag.initial_transform).unwrap();
                        let new_json = serde_json::to_value(&current_t).unwrap();
                        if old_json != new_json {
                            let cmd = ModifyComponent {
                                entity: drag.entity,
                                type_name: "TransformComponent".to_string(),
                                old_value: old_json,
                                new_value: new_json,
                            };
                            // Use push_no_execute: transform already applied live
                            self.undo_stack.push_no_execute(Box::new(cmd));
                            self.project.dirty = true;
                        }
                    }
                }
            }

            // ── Camera controller (skip if gizmo drag active) ─────────────────
            if self.viewport_state.active_drag.is_none() {
                let vs = &mut self.viewport_state;
                vs.camera_controller.update(&viewport_response, &mut vs.camera);
            }

            // ── Ray-cast picking on click (not drag) ─────────────────────────
            if viewport_response.clicked() && !viewport_response.dragged() {
                if let Some(click_pos) = viewport_response.interact_pointer_pos() {
                    use crate::viewport::picking::{pick_entity, ray_from_viewport_click};
                    let ray = ray_from_viewport_click(
                        click_pos,
                        viewport_rect,
                        &self.viewport_state.camera,
                    );
                    let scene = Arc::clone(&self.scene);
                    if let Some(hit) = pick_entity(&ray, &scene) {
                        self.selection.select_entity(hit);
                    } else {
                        // Only deselect if click was not on a gizmo handle
                        let on_gizmo = self
                            .selection
                            .selected_entity()
                            .and_then(|e| self.scene.components.get::<TransformComponent>(e))
                            .and_then(|t| {
                                hit_test_gizmo(
                                    self.viewport_state.gizmo_mode,
                                    &t,
                                    &self.viewport_state.camera,
                                    viewport_rect,
                                    click_pos,
                                )
                            })
                            .is_some();
                        if !on_gizmo {
                            self.selection.clear();
                        }
                    }
                }
            }

            // ── Gizmo overlay ─────────────────────────────────────────────────
            if let Some(entity) = self.selection.selected_entity() {
                if let Some(transform) =
                    self.scene.components.get::<TransformComponent>(entity)
                {
                    let painter = ui.painter_at(viewport_rect);

                    let hovered_axis = if self.viewport_state.active_drag.is_none() {
                        viewport_response.hover_pos().and_then(|pos| {
                            hit_test_gizmo(
                                self.viewport_state.gizmo_mode,
                                &transform,
                                &self.viewport_state.camera,
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
        });

        ctx.request_repaint();
    }
}

use std::sync::Arc;

use rython_ecs::Scene;
use rython_renderer::{Camera, GpuContext, RendererConfig, RendererState};

use crate::panels::viewport_panel;
use crate::viewport::{CameraController, ViewportTexture};

pub struct EditorApp {
    renderer: Option<RendererState>,
    scene: Arc<Scene>,
    viewport_texture: Option<ViewportTexture>,
    viewport_camera: Camera,
    camera_controller: CameraController,
    show_hierarchy: bool,
    show_inspector: bool,
}

impl EditorApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let wgpu_state = cc
            .wgpu_render_state
            .as_ref()
            .expect("rython-editor requires the wgpu renderer (--renderer wgpu)");

        // Clone wgpu handles from eframe's render state.
        // In wgpu 24, Device/Queue/Adapter implement Clone (internally reference-counted).
        let device: wgpu::Device = wgpu_state.device.clone();
        let queue: wgpu::Queue = wgpu_state.queue.clone();
        let adapter: wgpu::Adapter = wgpu_state.adapter.clone();
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());

        // Build pipelines against the offscreen texture format (RGBA8UnormSrgb).
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

        // Position camera at (0, 5, -10) looking at origin
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
        }
    }
}

impl eframe::App for EditorApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let Some(render_state) = frame.wgpu_render_state() else {
            return;
        };
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };

        // Top menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New Scene").clicked() {
                        self.scene = Arc::new(Scene::new());
                        ui.close_menu();
                    }
                    if ui.button("Open Scene…").clicked() {
                        // TODO: rfd file dialog
                        ui.close_menu();
                    }
                    if ui.button("Save Scene").clicked() {
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Edit", |ui| {
                    ui.label("(no actions yet)");
                });
                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut self.show_hierarchy, "Hierarchy");
                    ui.checkbox(&mut self.show_inspector, "Inspector");
                });
            });
        });

        // Bottom panel — asset browser placeholder
        egui::TopBottomPanel::bottom("asset_browser")
            .resizable(true)
            .min_height(80.0)
            .show(ctx, |ui| {
                ui.heading("Asset Browser");
                ui.label("(drag assets here — not yet implemented)");
            });

        // Left panel — hierarchy placeholder
        if self.show_hierarchy {
            egui::SidePanel::left("hierarchy")
                .resizable(true)
                .min_width(180.0)
                .show(ctx, |ui| {
                    ui.heading("Hierarchy");
                    ui.separator();
                    let count = self.scene.entity_count();
                    ui.label(format!("{count} entities"));
                    ui.label("(scene tree — not yet implemented)");
                });
        }

        // Right panel — inspector placeholder
        if self.show_inspector {
            egui::SidePanel::right("inspector")
                .resizable(true)
                .min_width(200.0)
                .show(ctx, |ui| {
                    ui.heading("Inspector");
                    ui.separator();
                    ui.label("Select an entity to inspect");
                    ui.label("(component editor — not yet implemented)");
                });
        }

        // Central panel — 3D viewport
        egui::CentralPanel::default().show(ctx, |ui| {
            let viewport_response = viewport_panel::show(
                ui,
                render_state,
                renderer,
                &self.scene,
                &mut self.viewport_texture,
                &mut self.viewport_camera,
            );
            self.camera_controller
                .update(&viewport_response, &mut self.viewport_camera);
        });

        // Request repaint so the viewport animates (orbit camera, etc.)
        ctx.request_repaint();
    }
}

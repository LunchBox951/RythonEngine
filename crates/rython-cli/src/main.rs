#![deny(warnings)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use pyo3::prelude::*;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, MouseButton as WinitMouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode as WinitKeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use rython_audio::AudioManager;
use rython_core::{EngineConfig, WindowConfig};
use rython_ecs::{RenderSystem, Scene, TransformSystem};
use rython_engine::{Engine, EngineBuilder};
use rython_input::{AxisBinding, ButtonBinding, InputMap, PlayerController};
use rython_physics::PhysicsModule;
use rython_renderer::{Camera, RendererConfig, RendererState};
use rython_resources::ResourceManager;
use rython_scripting::{
    drain_draw_commands, drain_ui_draw_commands, flush_recurring_callbacks, reset_quit_requested,
    set_active_audio, set_active_input, set_active_physics, set_active_ui, set_elapsed_secs,
    ScriptingConfig, ScriptingModule, was_quit_requested,
};
use rython_ui::{Theme, UIManager};
use rython_window::{KeyCode, MouseButton, RawInputEvent, WindowModule};

// ── CLI args ──────────────────────────────────────────────────────────────────

struct CliArgs {
    script_dir: String,
    entry_point: Option<String>,
    config_path: Option<String>,
    headless: bool,
}

fn parse_args() -> CliArgs {
    let mut args = CliArgs {
        script_dir: "./scripts".to_string(),
        entry_point: None,
        config_path: None,
        headless: false,
    };

    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("Usage: rython [OPTIONS]");
                println!();
                println!("Options:");
                println!("  --script-dir <DIR>      Directory containing Python scripts (default: ./scripts)");
                println!("  --entry-point <MODULE>  Python module to import and call init()");
                println!("  --config <FILE>         Engine config JSON file");
                println!("  --headless              Run without creating a window");
                println!("  -h, --help              Print this help");
                std::process::exit(0);
            }
            "--script-dir" => {
                if let Some(val) = iter.next() {
                    args.script_dir = val;
                }
            }
            "--entry-point" => {
                if let Some(val) = iter.next() {
                    args.entry_point = Some(val);
                }
            }
            "--config" => {
                if let Some(val) = iter.next() {
                    args.config_path = Some(val);
                }
            }
            "--headless" => {
                args.headless = true;
            }
            _ => {}
        }
    }

    args
}

// ── Winit → rython key mapping ────────────────────────────────────────────────

fn winit_key_to_rython(key: &WinitKeyCode) -> Option<KeyCode> {
    use WinitKeyCode::*;
    match key {
        KeyA => Some(KeyCode::A),
        KeyB => Some(KeyCode::B),
        KeyC => Some(KeyCode::C),
        KeyD => Some(KeyCode::D),
        KeyE => Some(KeyCode::E),
        KeyF => Some(KeyCode::F),
        KeyG => Some(KeyCode::G),
        KeyH => Some(KeyCode::H),
        KeyI => Some(KeyCode::I),
        KeyJ => Some(KeyCode::J),
        KeyK => Some(KeyCode::K),
        KeyL => Some(KeyCode::L),
        KeyM => Some(KeyCode::M),
        KeyN => Some(KeyCode::N),
        KeyO => Some(KeyCode::O),
        KeyP => Some(KeyCode::P),
        KeyQ => Some(KeyCode::Q),
        KeyR => Some(KeyCode::R),
        KeyS => Some(KeyCode::S),
        KeyT => Some(KeyCode::T),
        KeyU => Some(KeyCode::U),
        KeyV => Some(KeyCode::V),
        KeyW => Some(KeyCode::W),
        KeyX => Some(KeyCode::X),
        KeyY => Some(KeyCode::Y),
        KeyZ => Some(KeyCode::Z),
        Digit0 => Some(KeyCode::Key0),
        Digit1 => Some(KeyCode::Key1),
        Digit2 => Some(KeyCode::Key2),
        Digit3 => Some(KeyCode::Key3),
        Digit4 => Some(KeyCode::Key4),
        Digit5 => Some(KeyCode::Key5),
        Digit6 => Some(KeyCode::Key6),
        Digit7 => Some(KeyCode::Key7),
        Digit8 => Some(KeyCode::Key8),
        Digit9 => Some(KeyCode::Key9),
        Space => Some(KeyCode::Space),
        Enter => Some(KeyCode::Enter),
        Escape => Some(KeyCode::Escape),
        Tab => Some(KeyCode::Tab),
        Backspace => Some(KeyCode::Backspace),
        ShiftLeft => Some(KeyCode::LeftShift),
        ShiftRight => Some(KeyCode::RightShift),
        ControlLeft => Some(KeyCode::LeftControl),
        ControlRight => Some(KeyCode::RightControl),
        AltLeft => Some(KeyCode::LeftAlt),
        AltRight => Some(KeyCode::RightAlt),
        ArrowUp => Some(KeyCode::Up),
        ArrowDown => Some(KeyCode::Down),
        ArrowLeft => Some(KeyCode::Left),
        ArrowRight => Some(KeyCode::Right),
        F1 => Some(KeyCode::F1),
        F2 => Some(KeyCode::F2),
        F3 => Some(KeyCode::F3),
        F4 => Some(KeyCode::F4),
        F5 => Some(KeyCode::F5),
        F6 => Some(KeyCode::F6),
        F7 => Some(KeyCode::F7),
        F8 => Some(KeyCode::F8),
        F9 => Some(KeyCode::F9),
        F10 => Some(KeyCode::F10),
        F11 => Some(KeyCode::F11),
        F12 => Some(KeyCode::F12),
        _ => None,
    }
}

// ── Engine construction ───────────────────────────────────────────────────────

fn build_engine(
    engine_config: &EngineConfig,
    scripting_config: ScriptingConfig,
) -> (
    Engine,
    Arc<Scene>,
    Arc<parking_lot::Mutex<rython_physics::PhysicsWorld>>,
    Arc<parking_lot::Mutex<UIManager>>,
    Arc<std::sync::Mutex<PlayerController>>,
) {
    let scene = Arc::new(Scene::new());
    let physics_world = Arc::new(parking_lot::Mutex::new(
        rython_physics::PhysicsWorld::with_default_config(),
    ));
    set_active_physics(Arc::clone(&physics_world));

    // UIManager — shared with scripting bridge for draw commands and mouse routing
    let ui_manager = Arc::new(parking_lot::Mutex::new(UIManager::new(Theme::default())));
    set_active_ui(Arc::clone(&ui_manager));

    // AudioManager — shared with scripting bridge for Python audio API
    let audio_manager = Arc::new(parking_lot::Mutex::new(AudioManager::new(Default::default())));
    set_active_audio(Arc::clone(&audio_manager));
    audio_manager.lock().ensure_initialized().expect("failed to init audio");

    // PlayerController — managed directly in the main loop; register default input map
    let mut pc = PlayerController::new(0);
    let mut default_map = InputMap::new("default");
    default_map.bind_axis("move_x", AxisBinding::KBAxis { negative: KeyCode::D, positive: KeyCode::A });
    default_map.bind_axis("move_x", AxisBinding::KBAxis { negative: KeyCode::Right, positive: KeyCode::Left });
    default_map.bind_axis("move_z", AxisBinding::KBAxis { negative: KeyCode::S, positive: KeyCode::W });
    default_map.bind_axis("move_z", AxisBinding::KBAxis { negative: KeyCode::Down, positive: KeyCode::Up });
    default_map.bind_button("jump", ButtonBinding::Keyboard(KeyCode::Space));
    default_map.bind_button("pause", ButtonBinding::Keyboard(KeyCode::Escape));
    pc.register_map(default_map);
    let player_controller = Arc::new(std::sync::Mutex::new(pc));

    let engine = EngineBuilder::new()
        .with_config(engine_config.clone())
        .with_scene(Arc::clone(&scene))
        .add_module(Box::new(WindowModule::new(engine_config.window.clone())))
        .add_module(Box::new(ScriptingModule::new(
            scripting_config,
            Arc::clone(&scene),
        )))
        .add_module(Box::new(PhysicsModule::new(Default::default())))
        .add_module(Box::new(ResourceManager::new(Default::default())))
        .build()
        .expect("failed to build engine");

    (engine, scene, physics_world, ui_manager, player_controller)
}

// ── Headless mode ─────────────────────────────────────────────────────────────

fn run_headless(engine_config: EngineConfig, scripting_config: ScriptingConfig) {
    let (mut engine, scene, physics_world, _ui_manager, player_controller) =
        build_engine(&engine_config, scripting_config);
    engine.boot().expect("failed to boot engine");
    let start = Instant::now();
    loop {
        set_elapsed_secs(start.elapsed().as_secs_f64());
        Python::attach(|py| flush_recurring_callbacks(py));
        scene.drain_commands();
        physics_world.lock().sync_step(&scene);
        {
            let mut pc = player_controller.lock().unwrap();
            pc.tick(&[]);
            let snapshot = pc.get_snapshot(0).unwrap().clone();
            drop(pc);
            set_active_input(snapshot);
        }
        engine.tick().ok();
        if was_quit_requested() {
            reset_quit_requested();
            break;
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    engine.shutdown().ok();
}

// ── Windowed mode (winit 0.30 ApplicationHandler) ─────────────────────────────

struct App {
    engine: Option<Engine>,
    scene: Arc<Scene>,
    window_config: WindowConfig,
    window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    renderer: Option<RendererState>,
    surface_config: Option<wgpu::SurfaceConfiguration>,
    start_time: Instant,
    // Input wiring
    player_controller: Arc<std::sync::Mutex<PlayerController>>,
    raw_events: Vec<RawInputEvent>,
    cursor_pos: (f64, f64),
    // Physics wiring
    physics_world: Arc<parking_lot::Mutex<rython_physics::PhysicsWorld>>,
    // UI wiring
    ui_manager: Arc<parking_lot::Mutex<UIManager>>,
}

impl App {
    fn new(
        engine: Engine,
        scene: Arc<Scene>,
        window_config: WindowConfig,
        physics_world: Arc<parking_lot::Mutex<rython_physics::PhysicsWorld>>,
        ui_manager: Arc<parking_lot::Mutex<UIManager>>,
        player_controller: Arc<std::sync::Mutex<PlayerController>>,
    ) -> Self {
        Self {
            engine: Some(engine),
            scene,
            window_config,
            window: None,
            surface: None,
            renderer: None,
            surface_config: None,
            start_time: Instant::now(),
            player_controller,
            raw_events: Vec::new(),
            cursor_pos: (0.0, 0.0),
            physics_world,
            ui_manager,
        }
    }

    fn tick_and_render(&mut self, event_loop: &ActiveEventLoop) {
        let Some(renderer) = self.renderer.as_mut() else { return };
        let Some(surface) = self.surface.as_ref() else { return };
        let Some(surface_cfg) = self.surface_config.as_ref() else { return };
        let Some(engine) = self.engine.as_mut() else { return };

        let width = surface_cfg.width;
        let height = surface_cfg.height;

        // Update time and run Python callbacks
        set_elapsed_secs(self.start_time.elapsed().as_secs_f64());
        Python::attach(|py| flush_recurring_callbacks(py));
        self.scene.drain_commands();

        // Physics step
        self.physics_world.lock().sync_step(&self.scene);

        // Input: tick player controller and publish snapshot
        {
            let mut pc = self.player_controller.lock().unwrap();
            pc.tick(&self.raw_events);
            let snapshot = pc.get_snapshot(0).unwrap().clone();
            drop(pc);
            set_active_input(snapshot);
        }

        // UI: route mouse move and clicks from accumulated events this frame
        let norm_x = (self.cursor_pos.0 / width.max(1) as f64) as f32;
        let norm_y = (self.cursor_pos.1 / height.max(1) as f64) as f32;
        let click_cb = {
            let mut ui = self.ui_manager.lock();
            ui.on_mouse_move(norm_x, norm_y);
            let mut cb = None;
            for event in &self.raw_events {
                if matches!(event, RawInputEvent::MouseButtonPressed(MouseButton::Left)) {
                    cb = ui.on_mouse_click(norm_x, norm_y);
                }
            }
            cb
        }; // lock dropped here — callbacks may re-enter the UI manager safely
        if let Some(cb) = click_cb {
            cb();
        }

        // Clear per-frame events
        self.raw_events.clear();

        // ECS systems
        let world_transforms = TransformSystem::run(&self.scene.components, &self.scene.hierarchy);
        let ecs_cmds = RenderSystem::run(&self.scene.components, &world_transforms);

        // Compute UI layout so abs positions are current before drawing
        self.ui_manager.lock().compute_layout();

        // Drain script draw commands (from renderer bridge) and UI draw commands
        let script_cmds = drain_draw_commands();
        let ui_cmds = drain_ui_draw_commands();

        // Build camera from Python bridge state
        let mut camera = Camera::new();
        Python::attach(|py| {
            if let Ok(m) = py.import(pyo3::intern!(py, "rython")) {
                if let Ok(cam) = m.getattr("camera") {
                    let px: f32 = cam
                        .getattr("pos_x")
                        .and_then(|v| v.extract())
                        .unwrap_or(0.0);
                    let py_val: f32 = cam
                        .getattr("pos_y")
                        .and_then(|v| v.extract())
                        .unwrap_or(0.0);
                    let pz: f32 = cam
                        .getattr("pos_z")
                        .and_then(|v| v.extract())
                        .unwrap_or(-10.0);
                    camera.set_position(px, py_val, pz);
                    let tx: f32 = cam
                        .getattr("target_x")
                        .and_then(|v| v.extract())
                        .unwrap_or(0.0);
                    let ty: f32 = cam
                        .getattr("target_y")
                        .and_then(|v| v.extract())
                        .unwrap_or(0.0);
                    let tz: f32 = cam
                        .getattr("target_z")
                        .and_then(|v| v.extract())
                        .unwrap_or(0.0);
                    camera.set_look_at(tx, ty, tz);
                }
            }
        });
        camera.aspect = width as f32 / height.max(1) as f32;

        // Render
        let frame = match surface.get_current_texture() {
            Ok(f) => f,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                // Reconfigure and skip this frame
                surface.configure(&renderer.gpu.device, surface_cfg);
                return;
            }
            Err(e) => {
                log::warn!("surface error: {e}");
                return;
            }
        };

        let color_view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());

        // MSAA: ensure multisampled texture is ready
        let sample_count = renderer.gpu.sample_count;
        if sample_count > 1 {
            let fmt = renderer.gpu.surface_format;
            renderer.ensure_msaa_texture(width, height, fmt);
        }

        // MSAA-aware clear pass (inline to support resolve target)
        {
            let [r, g, b, a] = renderer.config.clear_color;
            let clear_color = wgpu::Color {
                r: r as f64 / 255.0,
                g: g as f64 / 255.0,
                b: b as f64 / 255.0,
                a: a as f64 / 255.0,
            };
            let (att_view, att_resolve): (&wgpu::TextureView, Option<&wgpu::TextureView>) =
                match (sample_count > 1, renderer.msaa_view()) {
                    (true, Some(mv)) => (mv, Some(&color_view)),
                    _ => (&color_view, None),
                };
            let mut enc = renderer.gpu.device.create_command_encoder(
                &wgpu::CommandEncoderDescriptor { label: Some("clear encoder") },
            );
            {
                let _pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("clear pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: att_view,
                        resolve_target: att_resolve,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(clear_color),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
            }
            renderer.gpu.queue.submit(std::iter::once(enc.finish()));
        }

        // Collect mesh draw commands from ECS
        let mesh_cmds: Vec<rython_renderer::DrawMesh> = ecs_cmds
            .into_iter()
            .filter_map(|cmd| {
                if let rython_ecs::DrawCommand::DrawMesh { mesh_id, texture_id, transform, .. } = cmd {
                    Some(rython_renderer::DrawMesh { mesh_id, texture_id, transform, z: 0.0 })
                } else {
                    None
                }
            })
            .collect();

        if !mesh_cmds.is_empty() {
            renderer.ensure_depth_texture(width, height);
            renderer.render_meshes(&mesh_cmds, &camera, &color_view);
        }

        // Collect all overlay draw commands from scripts and UI
        let all_overlay_cmds: Vec<rython_renderer::DrawCommand> =
            script_cmds.into_iter().chain(ui_cmds).collect();

        // Render solid-color rect overlays (UI button backgrounds, panels, etc.)
        let rect_cmds: Vec<rython_renderer::DrawRect> = all_overlay_cmds
            .iter()
            .filter_map(|cmd| {
                if let rython_renderer::DrawCommand::Rect(r) = cmd { Some(r.clone()) } else { None }
            })
            .collect();
        if !rect_cmds.is_empty() {
            renderer.render_rects(&rect_cmds, &color_view, width, height);
        }

        // Render text overlays
        let text_cmds: Vec<rython_renderer::DrawText> = all_overlay_cmds
            .into_iter()
            .filter_map(|cmd| {
                if let rython_renderer::DrawCommand::Text(t) = cmd { Some(t) } else { None }
            })
            .collect();
        if !text_cmds.is_empty() {
            renderer.render_text(&text_cmds, &color_view, width, height);
        }

        frame.present();

        engine.tick().ok();

        // Check quit
        if was_quit_requested() {
            reset_quit_requested();
            if let Some(eng) = self.engine.as_mut() {
                eng.shutdown().ok();
            }
            event_loop.exit();
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attrs = winit::window::WindowAttributes::default()
            .with_title(self.window_config.title.clone())
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.window_config.width,
                self.window_config.height,
            ));

        let window =
            Arc::new(event_loop.create_window(attrs).expect("failed to create window"));

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());

        // SAFETY: the window Arc keeps the window alive as long as the surface.
        let surface = instance
            .create_surface(Arc::clone(&window))
            .expect("failed to create wgpu surface");

        let gpu = pollster::block_on(rython_renderer::GpuContext::new_for_surface(
            instance, &surface, 4,
        ))
        .expect("failed to create GPU context");

        let size = window.inner_size();
        let surface_cfg = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: gpu.surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&gpu.device, &surface_cfg);

        let mut renderer = RendererState::new(gpu, RendererConfig::default());

        // Upload built-in cube mesh
        let cube = rython_resources::generate_cube();
        renderer.upload_mesh(
            "cube",
            bytemuck::cast_slice(&cube.vertices),
            &cube.indices,
        );

        self.surface = Some(surface);
        self.surface_config = Some(surface_cfg);
        self.renderer = Some(renderer);
        self.window = Some(window);

        // Boot engine
        if let Some(engine) = self.engine.as_mut() {
            engine.boot().expect("engine boot failed");
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                if let Some(engine) = self.engine.as_mut() {
                    engine.shutdown().ok();
                }
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                if let (Some(surface), Some(cfg), Some(renderer)) =
                    (self.surface.as_ref(), self.surface_config.as_mut(), self.renderer.as_ref())
                {
                    cfg.width = new_size.width.max(1);
                    cfg.height = new_size.height.max(1);
                    surface.configure(&renderer.gpu.device, cfg);
                }
            }
            WindowEvent::KeyboardInput {
                event: KeyEvent { physical_key: PhysicalKey::Code(winit_key), state, .. },
                ..
            } => {
                if let Some(key) = winit_key_to_rython(&winit_key) {
                    match state {
                        ElementState::Pressed => {
                            self.raw_events.push(RawInputEvent::KeyPressed(key));
                        }
                        ElementState::Released => {
                            self.raw_events.push(RawInputEvent::KeyReleased(key));
                        }
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let mb = match button {
                    WinitMouseButton::Left => Some(MouseButton::Left),
                    WinitMouseButton::Right => Some(MouseButton::Right),
                    WinitMouseButton::Middle => Some(MouseButton::Middle),
                    _ => None,
                };
                if let Some(mb) = mb {
                    match state {
                        ElementState::Pressed => {
                            self.raw_events.push(RawInputEvent::MouseButtonPressed(mb));
                        }
                        ElementState::Released => {
                            self.raw_events.push(RawInputEvent::MouseButtonReleased(mb));
                        }
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let dx = position.x - self.cursor_pos.0;
                let dy = position.y - self.cursor_pos.1;
                self.cursor_pos = (position.x, position.y);
                self.raw_events.push(RawInputEvent::MouseMoved { dx, dy });
            }
            WindowEvent::RedrawRequested => {
                self.tick_and_render(event_loop);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}

fn run_windowed(engine_config: EngineConfig, scripting_config: ScriptingConfig) {
    let (engine, scene, physics_world, ui_manager, player_controller) =
        build_engine(&engine_config, scripting_config);
    let event_loop = EventLoop::new().expect("failed to create event loop");
    let mut app = App::new(
        engine,
        scene,
        engine_config.window.clone(),
        physics_world,
        ui_manager,
        player_controller,
    );
    event_loop.run_app(&mut app).expect("event loop error");
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    env_logger::init();

    let cli = parse_args();

    let engine_config = cli
        .config_path
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<EngineConfig>(&s).ok())
        .unwrap_or_default();

    let scripting_config = ScriptingConfig::Dev {
        script_dir: cli.script_dir,
        entry_point: cli.entry_point,
    };

    if cli.headless {
        run_headless(engine_config, scripting_config);
    } else {
        run_windowed(engine_config, scripting_config);
    }
}

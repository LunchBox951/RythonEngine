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
use rython_core::{EngineConfig, ProjectConfig, WindowConfig};
use rython_ecs::{LightSystem, RenderSystem, Scene, TransformSystem};
use rython_engine::{Engine, EngineBuilder};
use rython_input::{AxisBinding, ButtonBinding, InputActionEvent, InputMap, PlayerController};
use rython_physics::PhysicsModule;
use rython_renderer::{Camera, RendererConfig, RendererState};
use rython_resources::ResourceManager;
use rython_scripting::{
    drain_draw_commands, drain_ui_draw_commands, flush_python_bg_completions,
    flush_python_bg_tasks, flush_python_par_tasks, flush_python_seq_tasks,
    flush_recurring_callbacks, flush_timers, get_scene_settings, reset_quit_requested,
    set_active_audio, set_active_input, set_active_physics, set_active_ui, set_elapsed_secs,
    was_quit_requested, ScriptingConfig, ScriptingModule,
};
use rython_ui::{Theme, UIManager};
use rython_window::{KeyCode, MouseButton, RawInputEvent, WindowModule};

// ── CLI args ──────────────────────────────────────────────────────────────────

struct CliArgs {
    script_dir: String,
    entry_point: Option<String>,
    config_path: Option<String>,
    headless: bool,
    project_path: Option<String>,
}

fn parse_args_from(mut iter: impl Iterator<Item = String>) -> CliArgs {
    let mut args = CliArgs {
        script_dir: "./scripts".to_string(),
        entry_point: None,
        config_path: None,
        headless: false,
        project_path: None,
    };

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("Usage: rython [OPTIONS]");
                println!();
                println!("Options:");
                println!("  --project <DIR>         Game project directory containing project.json (release mode)");
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
            "--project" => {
                if let Some(val) = iter.next() {
                    args.project_path = Some(val);
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

fn parse_args() -> CliArgs {
    parse_args_from(std::env::args().skip(1))
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

type EngineWithShared = (
    Engine,
    Arc<Scene>,
    Arc<parking_lot::Mutex<rython_physics::PhysicsWorld>>,
    Arc<parking_lot::Mutex<UIManager>>,
    Arc<std::sync::Mutex<PlayerController>>,
);

fn build_engine(
    engine_config: &EngineConfig,
    scripting_config: ScriptingConfig,
) -> EngineWithShared {
    let scene = Arc::new(Scene::new());
    let physics_world = Arc::new(parking_lot::Mutex::new(
        rython_physics::PhysicsWorld::with_default_config(),
    ));
    set_active_physics(Arc::clone(&physics_world));

    // UIManager — shared with scripting bridge for draw commands and mouse routing
    let ui_manager = Arc::new(parking_lot::Mutex::new(UIManager::new(Theme::default())));
    set_active_ui(Arc::clone(&ui_manager));

    // AudioManager — shared with scripting bridge for Python audio API
    let audio_manager = Arc::new(parking_lot::Mutex::new(AudioManager::new(
        Default::default(),
    )));
    set_active_audio(Arc::clone(&audio_manager));
    audio_manager
        .lock()
        .ensure_initialized()
        .expect("failed to init audio");

    // PlayerController — managed directly in the main loop; register default input map
    let mut pc = PlayerController::new(0);
    let mut default_map = InputMap::new("default");
    default_map.bind_axis(
        "move_x",
        AxisBinding::KBAxis {
            negative: KeyCode::D,
            positive: KeyCode::A,
        },
    );
    default_map.bind_axis(
        "move_x",
        AxisBinding::KBAxis {
            negative: KeyCode::Right,
            positive: KeyCode::Left,
        },
    );
    default_map.bind_axis(
        "move_z",
        AxisBinding::KBAxis {
            negative: KeyCode::S,
            positive: KeyCode::W,
        },
    );
    default_map.bind_axis(
        "move_z",
        AxisBinding::KBAxis {
            negative: KeyCode::Down,
            positive: KeyCode::Up,
        },
    );
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
    if let Err(e) = engine.boot() {
        eprintln!("engine boot failed: {e}");
        return;
    }
    let start = Instant::now();
    loop {
        set_elapsed_secs(start.elapsed().as_secs_f64());
        Python::attach(|py| {
            flush_python_bg_completions(py);
            flush_python_seq_tasks(py);
            flush_python_par_tasks(py);
            flush_recurring_callbacks(py);
            flush_timers(py);
        });
        flush_python_bg_tasks();
        scene.drain_commands();
        physics_world.lock().sync_step(&scene);
        // Propagate parent→child world transforms exactly as the windowed
        // event loop does (step 8 of the frame pipeline). Without this,
        // any physics body whose ECS transform cascades through a hierarchy
        // sees permanently stale child transforms in headless mode.
        let _world_transforms =
            TransformSystem::run(&scene.components, &scene.hierarchy);
        {
            // Recover from poison so a single panicking drainer doesn't kill
            // subsequent frames.
            let mut pc = match player_controller.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            pc.tick(&[]);
            let snapshot = match pc.get_snapshot(0) {
                Ok(s) => s.clone(),
                Err(_) => {
                    drop(pc);
                    std::thread::sleep(Duration::from_millis(16));
                    continue;
                }
            };
            let events_arc = pc.pending_events();
            let input_events: Vec<InputActionEvent> = {
                let mut guard = match events_arc.lock() {
                    Ok(g) => g,
                    Err(p) => p.into_inner(),
                };
                std::mem::take(&mut *guard)
            };
            drop(pc);
            set_active_input(snapshot);
            for ev in input_events {
                scene.emit(
                    &format!("input:{}", ev.action),
                    serde_json::json!({ "value": ev.value }),
                );
            }
        }
        // Drain any draw commands emitted by Python scripts / UI code so the
        // static command queues don't grow unboundedly in headless mode.
        // Results are dropped — headless has no renderer to consume them.
        let _ = drain_draw_commands();
        let _ = drain_ui_draw_commands();
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
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let Some(surface) = self.surface.as_ref() else {
            return;
        };
        let Some(surface_cfg) = self.surface_config.as_ref() else {
            return;
        };
        let Some(engine) = self.engine.as_mut() else {
            return;
        };

        let width = surface_cfg.width;
        let height = surface_cfg.height;

        // Update time and run Python callbacks
        set_elapsed_secs(self.start_time.elapsed().as_secs_f64());
        Python::attach(|py| {
            flush_python_bg_completions(py);
            flush_python_seq_tasks(py);
            flush_python_par_tasks(py);
            flush_recurring_callbacks(py);
            flush_timers(py);
        });
        flush_python_bg_tasks();
        self.scene.drain_commands();

        // Physics step
        self.physics_world.lock().sync_step(&self.scene);

        // Input: tick player controller, publish snapshot, and emit input events
        {
            let mut pc = self.player_controller.lock().unwrap();
            pc.tick(&self.raw_events);
            let snapshot = pc.get_snapshot(0).unwrap().clone();
            let input_events: Vec<InputActionEvent> =
                std::mem::take(&mut pc.pending_events().lock().unwrap());
            drop(pc);
            set_active_input(snapshot);
            for ev in input_events {
                self.scene.emit(
                    &format!("input:{}", ev.action),
                    serde_json::json!({ "value": ev.value }),
                );
            }
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
        let collected_lights = LightSystem::run(&self.scene.components, &world_transforms);

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

        let color_view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // MSAA: ensure multisampled texture is ready
        let sample_count = renderer.gpu.sample_count;
        if sample_count > 1 {
            let fmt = renderer.gpu.surface_format;
            renderer.ensure_msaa_texture(width, height, fmt);
        }

        // Apply scene settings from Python (clear color, light direction, etc.)
        renderer.scene_settings = get_scene_settings();

        // MSAA-aware clear pass (inline to support resolve target)
        {
            let [r, g, b, a] = renderer.scene_settings.clear_color;
            let clear_color = wgpu::Color {
                r: r as f64,
                g: g as f64,
                b: b as f64,
                a: a as f64,
            };
            let (att_view, att_resolve): (&wgpu::TextureView, Option<&wgpu::TextureView>) =
                match (sample_count > 1, renderer.msaa_view()) {
                    (true, Some(mv)) => (mv, Some(&color_view)),
                    _ => (&color_view, None),
                };
            let mut enc =
                renderer
                    .gpu
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("clear encoder"),
                    });
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
                if let rython_ecs::DrawCommand::DrawMesh {
                    mesh_id,
                    texture_id,
                    normal_map_id,
                    specular_map_id,
                    specular_color,
                    shininess,
                    transform,
                    metallic,
                    roughness,
                    ..
                } = cmd
                {
                    Some(rython_renderer::DrawMesh {
                        mesh_id,
                        texture_id,
                        normal_map_id,
                        specular_map_id,
                        specular_color,
                        shininess,
                        transform,
                        z: 0.0,
                        metallic,
                        roughness,
                        ..Default::default()
                    })
                } else {
                    None
                }
            })
            .collect();

        if !mesh_cmds.is_empty() {
            renderer.ensure_depth_texture(width, height);
            // Build LightBuffer from scene LightComponents, or fall back to scene_settings.
            let light_buffer: Option<rython_renderer::LightBuffer> = if collected_lights.is_empty()
            {
                None
            } else {
                let [ar, ag, ab] = renderer.scene_settings.ambient_color;
                let ai = renderer.scene_settings.ambient_intensity;
                let mut lb = rython_renderer::LightBuffer::empty();
                lb.ambient = [ar * ai, ag * ai, ab * ai];
                for cl in &collected_lights {
                    if lb.light_count as usize >= rython_renderer::MAX_LIGHTS {
                        break;
                    }
                    let idx = lb.light_count as usize;
                    lb.lights[idx] = rython_renderer::GpuLight {
                        position_or_dir: if cl.kind == 0 {
                            [cl.direction[0], cl.direction[1], cl.direction[2], 0.0]
                        } else {
                            [
                                cl.position[0],
                                cl.position[1],
                                cl.position[2],
                                cl.kind as f32,
                            ]
                        },
                        color_intensity: [cl.color[0], cl.color[1], cl.color[2], cl.intensity],
                        spot_params: [cl.inner_cos, cl.outer_cos, cl.radius, 1.0],
                        spot_dir_pad: [cl.direction[0], cl.direction[1], cl.direction[2], 0.0],
                    };
                    lb.light_count += 1;
                }
                Some(lb)
            };
            renderer.render_meshes(&mesh_cmds, &camera, &color_view, light_buffer.as_ref());
        }

        // Collect all overlay draw commands from scripts and UI
        let all_overlay_cmds: Vec<rython_renderer::DrawCommand> =
            script_cmds.into_iter().chain(ui_cmds).collect();

        // Render solid-color rect overlays (UI button backgrounds, panels, etc.)
        let rect_cmds: Vec<rython_renderer::DrawRect> = all_overlay_cmds
            .iter()
            .filter_map(|cmd| {
                if let rython_renderer::DrawCommand::Rect(r) = cmd {
                    Some(r.clone())
                } else {
                    None
                }
            })
            .collect();
        if !rect_cmds.is_empty() {
            renderer.render_rects(&rect_cmds, &color_view, width, height);
        }

        // Render text overlays
        let text_cmds: Vec<rython_renderer::DrawText> = all_overlay_cmds
            .into_iter()
            .filter_map(|cmd| {
                if let rython_renderer::DrawCommand::Text(t) = cmd {
                    Some(t)
                } else {
                    None
                }
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

        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .expect("failed to create window"),
        );

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
        renderer.upload_mesh("cube", bytemuck::cast_slice(&cube.vertices), &cube.indices);

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
                if let (Some(surface), Some(cfg), Some(renderer)) = (
                    self.surface.as_ref(),
                    self.surface_config.as_mut(),
                    self.renderer.as_ref(),
                ) {
                    cfg.width = new_size.width.max(1);
                    cfg.height = new_size.height.max(1);
                    surface.configure(&renderer.gpu.device, cfg);
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(winit_key),
                        state,
                        ..
                    },
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use winit::keyboard::KeyCode as WinitKeyCode;

    fn args(v: &[&str]) -> impl Iterator<Item = String> {
        v.iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .into_iter()
    }

    // ── parse_args_from ───────────────────────────────────────────────────────

    #[test]
    fn parse_defaults() {
        let a = parse_args_from(args(&[]));
        assert_eq!(a.script_dir, "./scripts");
        assert!(a.entry_point.is_none());
        assert!(a.config_path.is_none());
        assert!(!a.headless);
    }

    #[test]
    fn parse_script_dir() {
        let a = parse_args_from(args(&["--script-dir", "game/scripts"]));
        assert_eq!(a.script_dir, "game/scripts");
    }

    #[test]
    fn parse_entry_point() {
        let a = parse_args_from(args(&["--entry-point", "main"]));
        assert_eq!(a.entry_point.as_deref(), Some("main"));
    }

    #[test]
    fn parse_config() {
        let a = parse_args_from(args(&["--config", "engine.json"]));
        assert_eq!(a.config_path.as_deref(), Some("engine.json"));
    }

    #[test]
    fn parse_headless() {
        let a = parse_args_from(args(&["--headless"]));
        assert!(a.headless);
    }

    #[test]
    fn parse_multiple_options() {
        let a = parse_args_from(args(&[
            "--script-dir",
            "scripts",
            "--entry-point",
            "game.main",
            "--config",
            "cfg.json",
            "--headless",
        ]));
        assert_eq!(a.script_dir, "scripts");
        assert_eq!(a.entry_point.as_deref(), Some("game.main"));
        assert_eq!(a.config_path.as_deref(), Some("cfg.json"));
        assert!(a.headless);
    }

    #[test]
    fn parse_unknown_flags_ignored() {
        let a = parse_args_from(args(&["--unknown", "--also-unknown"]));
        assert_eq!(a.script_dir, "./scripts");
        assert!(!a.headless);
    }

    #[test]
    fn parse_script_dir_missing_value_keeps_default() {
        // --script-dir with no following value: default is preserved
        let a = parse_args_from(args(&["--script-dir"]));
        assert_eq!(a.script_dir, "./scripts");
    }

    // ── winit_key_to_rython ───────────────────────────────────────────────────

    #[test]
    fn key_mapping_letters() {
        let cases = [
            (WinitKeyCode::KeyA, KeyCode::A),
            (WinitKeyCode::KeyB, KeyCode::B),
            (WinitKeyCode::KeyC, KeyCode::C),
            (WinitKeyCode::KeyD, KeyCode::D),
            (WinitKeyCode::KeyE, KeyCode::E),
            (WinitKeyCode::KeyF, KeyCode::F),
            (WinitKeyCode::KeyG, KeyCode::G),
            (WinitKeyCode::KeyH, KeyCode::H),
            (WinitKeyCode::KeyI, KeyCode::I),
            (WinitKeyCode::KeyJ, KeyCode::J),
            (WinitKeyCode::KeyK, KeyCode::K),
            (WinitKeyCode::KeyL, KeyCode::L),
            (WinitKeyCode::KeyM, KeyCode::M),
            (WinitKeyCode::KeyN, KeyCode::N),
            (WinitKeyCode::KeyO, KeyCode::O),
            (WinitKeyCode::KeyP, KeyCode::P),
            (WinitKeyCode::KeyQ, KeyCode::Q),
            (WinitKeyCode::KeyR, KeyCode::R),
            (WinitKeyCode::KeyS, KeyCode::S),
            (WinitKeyCode::KeyT, KeyCode::T),
            (WinitKeyCode::KeyU, KeyCode::U),
            (WinitKeyCode::KeyV, KeyCode::V),
            (WinitKeyCode::KeyW, KeyCode::W),
            (WinitKeyCode::KeyX, KeyCode::X),
            (WinitKeyCode::KeyY, KeyCode::Y),
            (WinitKeyCode::KeyZ, KeyCode::Z),
        ];
        for (winit, expected) in cases {
            assert_eq!(
                winit_key_to_rython(&winit),
                Some(expected),
                "failed for {winit:?}"
            );
        }
    }

    #[test]
    fn key_mapping_digits() {
        let cases = [
            (WinitKeyCode::Digit0, KeyCode::Key0),
            (WinitKeyCode::Digit1, KeyCode::Key1),
            (WinitKeyCode::Digit2, KeyCode::Key2),
            (WinitKeyCode::Digit3, KeyCode::Key3),
            (WinitKeyCode::Digit4, KeyCode::Key4),
            (WinitKeyCode::Digit5, KeyCode::Key5),
            (WinitKeyCode::Digit6, KeyCode::Key6),
            (WinitKeyCode::Digit7, KeyCode::Key7),
            (WinitKeyCode::Digit8, KeyCode::Key8),
            (WinitKeyCode::Digit9, KeyCode::Key9),
        ];
        for (winit, expected) in cases {
            assert_eq!(
                winit_key_to_rython(&winit),
                Some(expected),
                "failed for {winit:?}"
            );
        }
    }

    #[test]
    fn key_mapping_special_keys() {
        let cases = [
            (WinitKeyCode::Space, KeyCode::Space),
            (WinitKeyCode::Enter, KeyCode::Enter),
            (WinitKeyCode::Escape, KeyCode::Escape),
            (WinitKeyCode::Tab, KeyCode::Tab),
            (WinitKeyCode::Backspace, KeyCode::Backspace),
        ];
        for (winit, expected) in cases {
            assert_eq!(
                winit_key_to_rython(&winit),
                Some(expected),
                "failed for {winit:?}"
            );
        }
    }

    #[test]
    fn key_mapping_modifiers() {
        let cases = [
            (WinitKeyCode::ShiftLeft, KeyCode::LeftShift),
            (WinitKeyCode::ShiftRight, KeyCode::RightShift),
            (WinitKeyCode::ControlLeft, KeyCode::LeftControl),
            (WinitKeyCode::ControlRight, KeyCode::RightControl),
            (WinitKeyCode::AltLeft, KeyCode::LeftAlt),
            (WinitKeyCode::AltRight, KeyCode::RightAlt),
        ];
        for (winit, expected) in cases {
            assert_eq!(
                winit_key_to_rython(&winit),
                Some(expected),
                "failed for {winit:?}"
            );
        }
    }

    #[test]
    fn key_mapping_arrows() {
        let cases = [
            (WinitKeyCode::ArrowUp, KeyCode::Up),
            (WinitKeyCode::ArrowDown, KeyCode::Down),
            (WinitKeyCode::ArrowLeft, KeyCode::Left),
            (WinitKeyCode::ArrowRight, KeyCode::Right),
        ];
        for (winit, expected) in cases {
            assert_eq!(
                winit_key_to_rython(&winit),
                Some(expected),
                "failed for {winit:?}"
            );
        }
    }

    #[test]
    fn key_mapping_function_keys() {
        let cases = [
            (WinitKeyCode::F1, KeyCode::F1),
            (WinitKeyCode::F2, KeyCode::F2),
            (WinitKeyCode::F3, KeyCode::F3),
            (WinitKeyCode::F4, KeyCode::F4),
            (WinitKeyCode::F5, KeyCode::F5),
            (WinitKeyCode::F6, KeyCode::F6),
            (WinitKeyCode::F7, KeyCode::F7),
            (WinitKeyCode::F8, KeyCode::F8),
            (WinitKeyCode::F9, KeyCode::F9),
            (WinitKeyCode::F10, KeyCode::F10),
            (WinitKeyCode::F11, KeyCode::F11),
            (WinitKeyCode::F12, KeyCode::F12),
        ];
        for (winit, expected) in cases {
            assert_eq!(
                winit_key_to_rython(&winit),
                Some(expected),
                "failed for {winit:?}"
            );
        }
    }

    #[test]
    fn key_mapping_unmapped_returns_none() {
        // Keys not in the game-relevant subset should return None
        assert_eq!(winit_key_to_rython(&WinitKeyCode::F13), None);
        assert_eq!(winit_key_to_rython(&WinitKeyCode::Numpad0), None);
        assert_eq!(winit_key_to_rython(&WinitKeyCode::CapsLock), None);
    }
}

// ── Mode resolution ───────────────────────────────────────────────────────────

/// Resolves whether to run in Dev or Release mode, and sets PYTHONHOME if
/// a bundled Python runtime is found adjacent to the binary.
///
/// MUST be called before any PyO3 GIL acquisition: `auto-initialize` fires
/// lazily on first GIL access, so PYTHONHOME must be in the environment
/// before that point to take effect.
///
/// Priority:
///   1. `--project <dir>` was given explicitly
///   2. `project.json` + `python/` exist adjacent to the binary (release dist)
///   3. Fall back to Dev mode using `--script-dir` / `--entry-point`
fn resolve_mode(args: &CliArgs) -> Result<(ScriptingConfig, EngineConfig), String> {
    let project_dir: Option<std::path::PathBuf> = if let Some(ref p) = args.project_path {
        Some(std::path::PathBuf::from(p))
    } else {
        let exe =
            std::env::current_exe().map_err(|e| format!("could not determine exe path: {e}"))?;
        let exe_dir = exe
            .parent()
            .ok_or_else(|| "exe has no parent directory".to_string())?;
        if exe_dir.join("project.json").exists() && exe_dir.join("python").is_dir() {
            Some(exe_dir.to_path_buf())
        } else {
            None
        }
    };

    if let Some(proj_dir) = project_dir {
        let proj_json = std::fs::read_to_string(proj_dir.join("project.json"))
            .map_err(|e| format!("failed to read project.json: {e}"))?;
        let project: ProjectConfig = serde_json::from_str(&proj_json)
            .map_err(|e| format!("failed to parse project.json: {e}"))?;

        // Set PYTHONHOME before the GIL is ever acquired. At this point the
        // process is single-threaded, so set_var is safe.
        let python_home = proj_dir.join("python");
        unsafe {
            std::env::set_var("PYTHONHOME", &python_home);
            std::env::set_var("PYTHONNOUSERSITE", "1");
            std::env::set_var("PYTHONPATH", "");
        }

        let bundle_path = proj_dir.join("game.bundle").to_string_lossy().into_owned();
        Ok((
            ScriptingConfig::Release {
                bundle_path,
                entry_point: project.entry_point,
            },
            project.engine_config,
        ))
    } else {
        let engine_config = args
            .config_path
            .as_deref()
            .map(|p| {
                std::fs::read_to_string(p)
                    .map_err(|e| format!("failed to read config {p}: {e}"))
                    .and_then(|s| {
                        serde_json::from_str::<EngineConfig>(&s)
                            .map_err(|e| format!("invalid engine config {p}: {e}"))
                    })
            })
            .transpose()?
            .unwrap_or_default();
        Ok((
            ScriptingConfig::Dev {
                script_dir: args.script_dir.clone(),
                entry_point: args.entry_point.clone(),
            },
            engine_config,
        ))
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    env_logger::init();

    let cli = parse_args();

    let (scripting_config, engine_config) = match resolve_mode(&cli) {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    if cli.headless {
        run_headless(engine_config, scripting_config);
    } else {
        run_windowed(engine_config, scripting_config);
    }
}

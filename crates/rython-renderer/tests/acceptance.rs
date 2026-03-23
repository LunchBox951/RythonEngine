use rython_renderer::{
    norm_to_clip, rect_to_clip_verts, validate_wgsl, Camera, Color, CommandQueue, DrawCommand,
    DrawRect, RendererConfig,
};

// ─── T-REND-01: Renderer Initialization ──────────────────────────────────────

#[test]
#[ignore = "requires hardware GPU (Vulkan/Metal/DX12)"]
fn t_rend_01_renderer_initialization() {
    // Initialise a headless GpuContext and verify adapter/device are obtained.
    pollster::block_on(async {
        use rython_renderer::GpuContext;

        let ctx = GpuContext::new_headless()
            .await
            .expect("wgpu adapter and device should be available");

        let info = ctx.adapter.get_info();
        // Spec: adapter must not be software fallback
        assert_ne!(
            info.device_type,
            wgpu::DeviceType::Cpu,
            "expected hardware adapter, got {:?}",
            info.device_type
        );

        // Spec: primitive, image, and text shader pipelines compile without error
        // (pipelines are created inside new_headless; reaching here means they compiled)
        let _ = &ctx.pipelines.primitive;
        let _ = &ctx.pipelines.image;
        let _ = &ctx.pipelines.text;
    });
}

// ─── T-REND-02: Empty Frame Renders Without Error ────────────────────────────

#[test]
#[ignore = "requires hardware GPU and a wgpu surface (window)"]
fn t_rend_02_empty_frame_renders_without_error() {
    // With a real surface, this test would:
    // 1. Create a GpuContext with a surface.
    // 2. Call render_clear() with zero draw commands.
    // 3. Call surface_texture.present().
    // 4. Assert no validation errors.
    //
    // Without a window, this is exercised by integration tests run with a GPU.
    unimplemented!("requires window surface — run in integration environment with GPU");
}

// ─── T-REND-03: Draw Command Z-Sorting ───────────────────────────────────────

#[test]
fn t_rend_03_draw_command_z_sorting() {
    let queue = CommandQueue::new(64);

    // Submit 5 DrawRect commands in scrambled z order
    for &z in &[5.0f32, 1.0, 3.0, 2.0, 4.0] {
        queue.push(DrawCommand::Rect(DrawRect {
            x: 0.0,
            y: 0.0,
            w: 0.1,
            h: 0.1,
            color: Color::rgb(255, 255, 255),
            border: None,
            border_width: 0.0,
            z,
        }));
    }

    queue.swap();
    assert_eq!(queue.front_len(), 5);

    let sorted = queue.take_sorted_front();
    let z_values: Vec<f32> = sorted.iter().map(|c| c.z()).collect();

    assert_eq!(
        z_values,
        vec![1.0, 2.0, 3.0, 4.0, 5.0],
        "commands must be sorted ascending by z: {z_values:?}"
    );
    // Command with z=1.0 is first (furthest back, drawn first)
    assert_eq!(
        sorted[0].z(),
        1.0,
        "lowest z must be drawn first"
    );
}

// ─── T-REND-04: Normalized Coordinate Mapping ────────────────────────────────

#[test]
fn t_rend_04_normalized_coordinate_mapping() {
    // Full-screen rect (0,0,1,1) must cover all of clip space [-1, 1]
    let verts = rect_to_clip_verts(0.0, 0.0, 1.0, 1.0);
    let [tl, tr, bl, br] = verts;

    assert_eq!(tl, [-1.0, 1.0], "top-left: clip (-1, +1)");
    assert_eq!(tr, [1.0, 1.0], "top-right: clip (+1, +1)");
    assert_eq!(bl, [-1.0, -1.0], "bottom-left: clip (-1, -1)");
    assert_eq!(br, [1.0, -1.0], "bottom-right: clip (+1, -1)");

    // Quarter rect at (0.5, 0.5, 0.5, 0.5): top-left at (0.5,0.5), size (0.5,0.5)
    let q_verts = rect_to_clip_verts(0.5, 0.5, 0.5, 0.5);
    let [qtl, qtr, qbl, qbr] = q_verts;

    // norm (0.5, 0.5) → clip (0.0, 0.0); norm (1.0, 1.0) → clip (1.0, -1.0)
    assert_eq!(qtl, [0.0, 0.0], "quarter top-left: clip (0, 0)");
    assert_eq!(qtr, [1.0, 0.0], "quarter top-right: clip (1, 0)");
    assert_eq!(qbl, [0.0, -1.0], "quarter bottom-left: clip (0, -1)");
    assert_eq!(qbr, [1.0, -1.0], "quarter bottom-right: clip (1, -1)");

    // Spot-check norm_to_clip at origin and centre
    assert_eq!(norm_to_clip(0.0, 0.0), [-1.0, 1.0]);
    assert_eq!(norm_to_clip(1.0, 1.0), [1.0, -1.0]);
    assert_eq!(norm_to_clip(0.5, 0.5), [0.0, 0.0]);
}

// ─── T-REND-05: Color Value Mapping ──────────────────────────────────────────

#[test]
fn t_rend_05_color_value_mapping() {
    let color = Color::new(255, 0, 128, 200);
    let [r, g, b, a] = color.to_linear();

    assert_eq!(r, 1.0, "R=255 → 1.0");
    assert_eq!(g, 0.0, "G=0 → 0.0");
    assert!(
        (b - 128.0 / 255.0).abs() < 1e-5,
        "B=128 → ~0.502, got {b}"
    );
    assert!(
        (a - 200.0 / 255.0).abs() < 1e-5,
        "A=200 → ~0.784, got {a}"
    );
}

// ─── T-REND-06: Double-Buffered Command Queue ─────────────────────────────────

#[test]
fn t_rend_06_double_buffered_command_queue() {
    let queue = CommandQueue::new(65536);

    // Before any swap, front buffer is empty (last frame had nothing)
    assert_eq!(queue.front_len(), 0, "front buffer initially empty");

    // Game systems push 100 commands to the back buffer (RENDER_ENQUEUE)
    for i in 0..100u32 {
        queue.push(DrawCommand::Rect(DrawRect {
            x: 0.0,
            y: 0.0,
            w: 0.1,
            h: 0.1,
            color: Color::rgb(255, 255, 255),
            border: None,
            border_width: 0.0,
            z: i as f32,
        }));
    }

    // Front buffer is still empty — renderer is safely reading last frame's data
    assert_eq!(queue.front_len(), 0, "front unchanged during enqueue");
    assert_eq!(queue.back_len(), 100, "back holds 100 pending commands");

    // Phase boundary swap
    queue.swap();

    // After swap: front has this frame's 100 commands; back is cleared
    assert_eq!(queue.front_len(), 100, "after swap, front has exactly 100 commands");
    assert_eq!(queue.back_len(), 0, "after swap, back is cleared for next frame");

    // Renderer drains the front buffer
    let cmds = queue.take_sorted_front();
    assert_eq!(cmds.len(), 100, "renderer receives all 100 commands");
    assert_eq!(queue.front_len(), 0, "front drained after take");
}

// ─── T-REND-07: GPU Texture Upload from Background Decode ────────────────────

#[test]
#[ignore = "requires hardware GPU"]
fn t_rend_07_gpu_texture_upload_from_background_decode() {
    use std::sync::{Arc, Mutex};
    use rython_renderer::GpuUploadRequest;

    pollster::block_on(async {
        use rython_renderer::GpuContext;

        let ctx = GpuContext::new_headless()
            .await
            .expect("GPU required for texture upload test");

        let (tx, rx) = std::sync::mpsc::channel::<wgpu::Texture>();
        let tx = Arc::new(Mutex::new(tx));

        // Simulate a 256×256 RGBA image decoded on a background thread
        let width = 256u32;
        let height = 256u32;
        let pixels: Vec<u8> = vec![128u8; (width * height * 4) as usize];

        let tx2 = Arc::clone(&tx);
        let upload = GpuUploadRequest {
            width,
            height,
            pixels,
            on_ready: Box::new(move |texture| {
                tx2.lock().unwrap().send(texture).ok();
            }),
        };

        // Process the upload on the main thread (spec T-REND-07)
        ctx.process_uploads(vec![upload]);

        let texture = rx.recv().expect("on_ready callback must fire");

        // Spec: texture dimensions are 256×256, format is RGBA8Unorm
        assert_eq!(texture.size().width, 256);
        assert_eq!(texture.size().height, 256);
        assert_eq!(texture.format(), wgpu::TextureFormat::Rgba8Unorm);
    });
}

// ─── T-REND-08: DrawImage with Loaded Texture ────────────────────────────────

#[test]
#[ignore = "requires hardware GPU and texture management"]
fn t_rend_08_draw_image_with_loaded_texture() {
    // Full test requires:
    //   1. Load a test PNG → decode to pixels
    //   2. Upload via GpuContext::process_uploads
    //   3. Submit a DrawImage command
    //   4. Render one frame via the image pipeline
    //   5. Assert no GPU validation errors
    //
    // Without an integrated asset manager and surface this is an integration test.
    unimplemented!("requires asset manager + GPU surface — run in integration environment");
}

// ─── T-REND-09: DrawText Glyph Atlas ─────────────────────────────────────────

#[test]
#[ignore = "requires GPU and font rendering (glyph atlas generation)"]
fn t_rend_09_draw_text_glyph_atlas() {
    // Full test requires:
    //   1. Load a TTF font
    //   2. Rasterise glyphs for "Hello" into an atlas
    //   3. Upload atlas texture via GpuContext::process_uploads
    //   4. Verify atlas contains glyphs H, e, l, o
    //   5. Verify each character produces a separate textured quad
    //   6. Verify quads are positioned left-to-right with correct kerning
    //
    // Font rendering pipeline (e.g., ab_glyph) is a Layer 3 concern.
    unimplemented!("requires font rasterisation + GPU surface — run in integration environment");
}

// ─── T-REND-10: Camera View Matrix (Phase 3) ─────────────────────────────────

#[test]
fn t_rend_10_camera_view_matrix() {
    use rython_core::math::vec4;

    let mut camera = Camera::new();
    camera.set_position(0.0, 10.0, -20.0);
    camera.set_look_at(0.0, 0.0, 0.0);

    // Spec: forward vector ≈ (0, -0.447, 0.894)
    let fwd = camera.forward();
    assert!(
        fwd.x.abs() < 1e-5,
        "forward.x should be ≈ 0, got {}",
        fwd.x
    );
    assert!(
        (fwd.y - (-10.0_f32 / 500.0_f32.sqrt())).abs() < 1e-4,
        "forward.y ≈ -0.447, got {}",
        fwd.y
    );
    assert!(
        (fwd.z - (20.0_f32 / 500.0_f32.sqrt())).abs() < 1e-4,
        "forward.z ≈ 0.894, got {}",
        fwd.z
    );

    // Spec: view matrix transforms world origin to a point in front of the camera
    // (negative Z in right-handed view space = in front)
    let view = camera.view_matrix();
    let world_origin = vec4(0.0, 0.0, 0.0, 1.0);
    let view_pos = view * world_origin;

    assert!(
        view_pos.z < 0.0,
        "world origin must be in front of camera (view z < 0), got {}",
        view_pos.z
    );
}

// ─── T-REND-11: Camera Projection Matrix (Phase 3) ───────────────────────────

#[test]
fn t_rend_11_camera_projection_matrix() {
    use rython_core::math::vec4;

    let mut camera = Camera::new();
    camera.set_fov(90.0);
    camera.near = 0.1;
    camera.far = 1000.0;
    camera.aspect = 16.0 / 9.0;

    let proj = camera.projection_matrix();

    // Spec: points at z=near map to NDC z=0 (wgpu zero-to-one depth convention)
    let near_view = vec4(0.0, 0.0, -camera.near, 1.0);
    let near_clip = proj * near_view;
    let near_ndc_z = near_clip.z / near_clip.w;
    assert!(
        near_ndc_z.abs() < 1e-4,
        "near plane should map to NDC z=0 (got {near_ndc_z})"
    );

    // Spec: points at z=far map to NDC z=1
    let far_view = vec4(0.0, 0.0, -camera.far, 1.0);
    let far_clip = proj * far_view;
    let far_ndc_z = far_clip.z / far_clip.w;
    assert!(
        (far_ndc_z - 1.0).abs() < 1e-4,
        "far plane should map to NDC z=1 (got {far_ndc_z})"
    );

    // Spec: projection matrix has correct FOV — for 90° FOV, tan(45°)=1, so
    // the diagonal element should be 1/tan(fov/2) = 1.0 (for the non-aspect column)
    // proj[1][1] = h = 1 / tan(fov_y/2)
    let fov_rad = 90.0_f32.to_radians();
    let expected_h = 1.0 / (fov_rad / 2.0).tan();
    let actual_h = proj.col(1).y; // column 1, row 1
    assert!(
        (actual_h - expected_h).abs() < 1e-4,
        "proj[1][1] should be {expected_h}, got {actual_h}"
    );
}

// ─── T-REND-12: Max Draw Commands Enforcement ────────────────────────────────

#[test]
fn t_rend_12_max_draw_commands_enforcement() {
    // Initialize with max_draw_commands = 100
    let config = RendererConfig {
        max_draw_commands: 100,
        ..Default::default()
    };
    let queue = CommandQueue::new(config.max_draw_commands);

    // Submit 150 commands
    for i in 0..150u32 {
        queue.push(DrawCommand::Rect(DrawRect {
            x: 0.0,
            y: 0.0,
            w: 0.1,
            h: 0.1,
            color: Color::rgb(255, 255, 255),
            border: None,
            border_width: 0.0,
            z: i as f32,
        }));
    }

    // Spec: only first 100 commands processed
    assert_eq!(
        queue.back_len(),
        100,
        "back buffer must not exceed max_draw_commands=100"
    );

    // Spec: 50 commands were dropped with a logged warning
    assert_eq!(
        queue.dropped_count(),
        50,
        "exactly 50 commands should have been dropped"
    );

    // Spec: no buffer overflow or crash — swap and drain succeed
    queue.swap();
    let cmds = queue.take_sorted_front();
    assert_eq!(cmds.len(), 100, "renderer processes exactly 100 commands");
}

// ─── T-REND-13: Shader Hot-Reload Resilience ─────────────────────────────────

#[test]
fn t_rend_13_shader_hot_reload_resilience() {
    // Spec: intentionally malformed WGSL
    let malformed = "fn broken() { this_is_not_valid }"; // no @vertex or @fragment

    // Spec: shader compilation returns an error (not a panic)
    let result = validate_wgsl(malformed);
    assert!(
        result.is_err(),
        "malformed WGSL must return Err, not Ok"
    );

    let err = result.unwrap_err();

    // Spec: the renderer logs the error with the shader source location
    assert!(
        !err.message.is_empty(),
        "error must carry a non-empty message"
    );
    assert!(
        !err.location.is_empty(),
        "error must carry a source location hint"
    );
    assert!(
        err.to_string().contains("shader error"),
        "Display impl must include 'shader error': {}",
        err
    );

    // Spec: the error does not cause a panic — already proven by reaching here.

    // Verify that the built-in shaders all pass validation
    use rython_renderer::{IMAGE_WGSL, MESH_WGSL, PRIMITIVE_WGSL, TEXT_WGSL};
    for (name, src) in [
        ("primitive", PRIMITIVE_WGSL),
        ("image", IMAGE_WGSL),
        ("text", TEXT_WGSL),
        ("mesh", MESH_WGSL),
    ] {
        let r = validate_wgsl(src);
        assert!(r.is_ok(), "built-in shader '{name}' should pass validation: {r:?}");
    }
}

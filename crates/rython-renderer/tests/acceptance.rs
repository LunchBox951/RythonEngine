use rython_renderer::{
    norm_to_clip, rect_to_clip_verts, validate_wgsl, Camera, Color, CommandQueue, DrawCommand,
    DrawMesh, DrawRect, RendererConfig, RendererState,
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
#[ignore = "requires hardware GPU (Vulkan/Metal/DX12)"]
fn t_rend_02_empty_frame_renders_without_error() {
    pollster::block_on(async {
        use rython_renderer::GpuContext;

        let gpu = GpuContext::new_headless()
            .await
            .expect("GPU required for headless render test");

        // Headless render target: 64x64 texture using the surface colour format.
        let render_texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("headless render target"),
            size: wgpu::Extent3d { width: 64, height: 64, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: gpu.surface_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = render_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Clear pass — exercises the render/submit path without a window surface.
        let mut encoder = gpu.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("clear encoder") },
        );
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("clear pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }
        gpu.queue.submit(std::iter::once(encoder.finish()));
        // Reaching here without panic satisfies the spec.
    });
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
#[ignore = "requires hardware GPU (Vulkan/Metal/DX12)"]
fn t_rend_08_draw_image_with_loaded_texture() {
    use std::sync::{Arc, Mutex};
    use rython_renderer::GpuUploadRequest;

    pollster::block_on(async {
        use rython_renderer::GpuContext;

        let gpu = GpuContext::new_headless()
            .await
            .expect("GPU required for image pipeline test");

        // Upload a 2×2 RGBA test image via the standard upload path.
        let (tx, rx) = std::sync::mpsc::channel::<wgpu::Texture>();
        let tx = Arc::new(Mutex::new(tx));
        let tx2 = Arc::clone(&tx);

        let pixels: Vec<u8> = vec![
            255, 0,   0,   255, // red
            0,   255, 0,   255, // green
            0,   0,   255, 255, // blue
            255, 255, 0,   255, // yellow
        ];
        gpu.process_uploads(vec![GpuUploadRequest {
            width: 2,
            height: 2,
            pixels,
            on_ready: Box::new(move |tex| { tx2.lock().unwrap().send(tex).ok(); }),
        }]);
        let img_texture = rx.recv().expect("on_ready must fire");
        let img_view = img_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Sampler (filtering, matches SamplerBindingType::Filtering in the layout).
        let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("image sampler"),
            ..Default::default()
        });

        // Uniform buffer: rect(vec4) + alpha(f32) + _pad×3 = 32 bytes.
        // Full-screen clip rect (-1,-1, 2,2), alpha=1.
        let uniform_data: [f32; 8] = [-1.0, -1.0, 2.0, 2.0, 1.0, 0.0, 0.0, 0.0];
        let uniform_bytes: &[u8] = bytemuck::cast_slice(&uniform_data);
        let uniform_buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("image uniforms"),
            size: uniform_bytes.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: true,
        });
        uniform_buf.slice(..).get_mapped_range_mut().copy_from_slice(uniform_bytes);
        uniform_buf.unmap();

        // Bind group matching the image shader layout (uniform + texture + sampler).
        let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("image bind group"),
            layout: &gpu.bind_group_layouts.image,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&img_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });

        // Headless render target.
        let render_texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("image render target"),
            size: wgpu::Extent3d { width: 64, height: 64, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: gpu.surface_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let rt_view = render_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Draw one textured quad via the image pipeline.
        let mut encoder = gpu.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("image test encoder") },
        );
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("image test pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &rt_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&gpu.pipelines.image);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..6, 0..1); // one quad = 6 vertices (two triangles)
        }
        gpu.queue.submit(std::iter::once(encoder.finish()));
        // No panic = image pipeline executed correctly headlessly.
    });
}

// ─── T-REND-09: DrawText Glyph Atlas ─────────────────────────────────────────

#[test]
#[ignore = "requires hardware GPU (Vulkan/Metal/DX12)"]
fn t_rend_09_draw_text_glyph_atlas() {
    pollster::block_on(async {
        use rython_renderer::GpuContext;

        let gpu = GpuContext::new_headless()
            .await
            .expect("GPU required for text pipeline test");

        // Synthetic 32×32 Rgba8Unorm glyph atlas — white pixels simulate rasterised glyphs.
        // The text shader reads the .r channel for alpha, so any non-zero value is sufficient.
        let atlas_pixels: Vec<u8> = vec![255u8; 32 * 32 * 4];
        let atlas_texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph atlas"),
            size: wgpu::Extent3d { width: 32, height: 32, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &atlas_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &atlas_pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * 32),
                rows_per_image: Some(32),
            },
            wgpu::Extent3d { width: 32, height: 32, depth_or_array_layers: 1 },
        );
        let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Filtering sampler (required by text bind group layout).
        let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("atlas sampler"),
            ..Default::default()
        });

        // Uniform buffer: rect(vec4) + uv_rect(vec4) + color(vec4) = 48 bytes.
        // One full-screen glyph quad, UV covering the whole atlas, white colour.
        let uniform_data: [f32; 12] = [
            -1.0, -1.0, 2.0, 2.0, // rect: full clip-space quad
            0.0,   0.0, 1.0, 1.0, // uv_rect: full atlas
            1.0,   1.0, 1.0, 1.0, // color: white opaque
        ];
        let uniform_bytes: &[u8] = bytemuck::cast_slice(&uniform_data);
        let uniform_buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("text uniforms"),
            size: uniform_bytes.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: true,
        });
        uniform_buf.slice(..).get_mapped_range_mut().copy_from_slice(uniform_bytes);
        uniform_buf.unmap();

        // Bind group matching the text shader layout (uniform + atlas texture + sampler).
        let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("text bind group"),
            layout: &gpu.bind_group_layouts.text,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&atlas_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });

        // Headless render target.
        let render_texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("text render target"),
            size: wgpu::Extent3d { width: 64, height: 64, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: gpu.surface_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let rt_view = render_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Draw one glyph quad via the text pipeline.
        let mut encoder = gpu.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("text test encoder") },
        );
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("text test pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &rt_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&gpu.pipelines.text);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..6, 0..1); // one glyph quad = 6 vertices
        }
        gpu.queue.submit(std::iter::once(encoder.finish()));
        // No panic = text pipeline (atlas sampling) executed correctly headlessly.
    });
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

// ─── T-REND-14: Mesh Render Pipeline with Depth Buffer ───────────────────────
//
// Headless test that exercises the full mesh render dispatch:
//   1. Upload a procedural cube (24 verts, 36 indices) to the GPU buffer cache.
//   2. Create a Depth32Float depth texture.
//   3. Call render_meshes() with one DrawMesh command.
//   4. Assert no panic (pipeline compilation + draw_indexed succeeded).

#[test]
#[ignore = "requires hardware GPU (Vulkan/Metal/DX12)"]
fn t_rend_14_mesh_render_pipeline_with_depth_buffer() {
    pollster::block_on(async {
        use rython_renderer::GpuContext;
        use rython_core::math::Mat4;

        let gpu = GpuContext::new_headless()
            .await
            .expect("GPU required for mesh pipeline test");
        let mut state = RendererState::new(gpu, RendererConfig::default());

        // --- Build procedural cube mesh inline (matches generate_cube() output) ---
        // 24 vertices × 32 bytes each (position[3] + normal[3] + uv[2])
        // Each face: 4 verts with a shared face normal, two CCW triangles.
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct Vert { pos: [f32; 3], norm: [f32; 3], uv: [f32; 2] }
        unsafe impl bytemuck::Pod for Vert {}
        unsafe impl bytemuck::Zeroable for Vert {}

        let face_data: [([f32; 3], [[f32; 3]; 4]); 6] = [
            ([1.,0.,0.],  [[ 0.5,-0.5,-0.5],[ 0.5, 0.5,-0.5],[ 0.5, 0.5, 0.5],[ 0.5,-0.5, 0.5]]),
            ([-1.,0.,0.], [[-0.5,-0.5, 0.5],[-0.5, 0.5, 0.5],[-0.5, 0.5,-0.5],[-0.5,-0.5,-0.5]]),
            ([0.,1.,0.],  [[-0.5, 0.5, 0.5],[ 0.5, 0.5, 0.5],[ 0.5, 0.5,-0.5],[-0.5, 0.5,-0.5]]),
            ([0.,-1.,0.], [[-0.5,-0.5,-0.5],[ 0.5,-0.5,-0.5],[ 0.5,-0.5, 0.5],[-0.5,-0.5, 0.5]]),
            ([0.,0.,1.],  [[ 0.5,-0.5, 0.5],[ 0.5, 0.5, 0.5],[-0.5, 0.5, 0.5],[-0.5,-0.5, 0.5]]),
            ([0.,0.,-1.], [[-0.5,-0.5,-0.5],[-0.5, 0.5,-0.5],[ 0.5, 0.5,-0.5],[ 0.5,-0.5,-0.5]]),
        ];
        let uvs: [[f32; 2]; 4] = [[0.,0.],[1.,0.],[1.,1.],[0.,1.]];

        let mut verts: Vec<Vert> = Vec::with_capacity(24);
        let mut indices: Vec<u32> = Vec::with_capacity(36);
        for (norm, positions) in &face_data {
            let base = verts.len() as u32;
            for (i, pos) in positions.iter().enumerate() {
                verts.push(Vert { pos: *pos, norm: *norm, uv: uvs[i] });
            }
            indices.extend_from_slice(&[base, base+1, base+2, base, base+2, base+3]);
        }

        assert_eq!(verts.len(), 24, "procedural cube: 24 vertices");
        assert_eq!(indices.len(), 36, "procedural cube: 36 indices");

        let verts_bytes: &[u8] = bytemuck::cast_slice(&verts);
        state.upload_mesh("cube", verts_bytes, &indices);

        // Confirm the mesh is in the cache.
        assert!(state.mesh_cache.contains_key("cube"), "mesh must be in cache after upload");

        // --- Create headless render targets ---
        let width = 64u32;
        let height = 64u32;

        let color_tex = state.gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("color rt"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: state.gpu.surface_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let color_view = color_tex.create_view(&wgpu::TextureViewDescriptor::default());

        // --- Clear the color target (required before mesh pass uses LoadOp::Load) ---
        {
            let mut enc = state.gpu.device.create_command_encoder(
                &wgpu::CommandEncoderDescriptor { label: Some("clear enc") },
            );
            {
                let _p = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("clear pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &color_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
            }
            state.gpu.queue.submit(std::iter::once(enc.finish()));
        }

        // --- Create depth texture ---
        state.ensure_depth_texture(width, height);
        let depth_view = state.depth_view().expect("depth view must be available after ensure");

        // --- Camera: position (3, 3, 3) looking at origin ---
        let mut camera = Camera::new();
        camera.set_position(3.0, 3.0, 3.0);
        camera.set_look_at(0.0, 0.0, 0.0);
        camera.aspect = width as f32 / height as f32;

        // --- Dispatch one DrawMesh ---
        let cmd = DrawMesh {
            mesh_id: "cube".to_string(),
            material_id: "default".to_string(),
            transform: Mat4::IDENTITY,
            z: 0.0,
        };
        state.render_meshes(&[cmd], &camera, &color_view, depth_view);

        // Reaching here without a wgpu validation error means:
        //   - Depth32Float texture was created successfully.
        //   - Vertex/index buffers were uploaded and bound correctly.
        //   - Mesh pipeline executed draw_indexed without error.
    });
}

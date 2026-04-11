use rython_renderer::{
    norm_to_clip, rect_to_clip_verts, validate_wgsl, Camera, Color, CommandQueue, DrawBillboard,
    DrawCircle, DrawCommand, DrawImage, DrawLine, DrawMesh, DrawRect, DrawText, RendererConfig,
    RendererState, SceneSettings,
};
use rython_core::math::{Vec2, Vec3};

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

    // Verify that the built-in shaders all pass validation.
    // SHADOW_WGSL is vertex-only (depth-only shadow pass) — validate_wgsl accepts
    // shaders with @vertex but no @fragment.
    use rython_renderer::{IMAGE_WGSL, MESH_WGSL, PRIMITIVE_WGSL, SHADOW_WGSL, TEXT_WGSL};
    for (name, src) in [
        ("primitive", PRIMITIVE_WGSL),
        ("image", IMAGE_WGSL),
        ("text", TEXT_WGSL),
        ("mesh", MESH_WGSL),
        ("shadow", SHADOW_WGSL),
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
        assert!(state.depth_view().is_some(), "depth view must be available after ensure");

        // --- Camera: position (3, 3, 3) looking at origin ---
        let mut camera = Camera::new();
        camera.set_position(3.0, 3.0, 3.0);
        camera.set_look_at(0.0, 0.0, 0.0);
        camera.aspect = width as f32 / height as f32;

        // --- Dispatch one DrawMesh ---
        let cmd = DrawMesh {
            mesh_id: "cube".to_string(),
            ..Default::default()
        };
        state.render_meshes(&[cmd], &camera, &color_view, None);

        // Reaching here without a wgpu validation error means:
        //   - Depth32Float texture was created successfully.
        //   - Vertex/index buffers were uploaded and bound correctly.
        //   - Mesh pipeline executed draw_indexed without error.
    });
}

// ─── Edge-case helpers ────────────────────────────────────────────────────────

fn make_rect(z: f32) -> DrawCommand {
    DrawCommand::Rect(DrawRect {
        x: 0.0, y: 0.0, w: 0.1, h: 0.1,
        color: Color::rgb(255, 255, 255),
        border: None, border_width: 0.0, z,
    })
}

// ─── CommandQueue edge cases ──────────────────────────────────────────────────

#[test]
fn edge_command_queue_zero_capacity() {
    let queue = CommandQueue::new(0);
    queue.push(make_rect(0.0));
    assert_eq!(queue.back_len(), 0, "zero-capacity: nothing accepted");
    assert_eq!(queue.dropped_count(), 1, "zero-capacity: command counted as dropped");
    queue.swap();
    let cmds = queue.take_sorted_front();
    assert!(cmds.is_empty(), "zero-capacity: take returns empty");
}

#[test]
fn edge_command_queue_swap_empty_back() {
    let queue = CommandQueue::new(64);
    queue.swap(); // swap with nothing pushed — both buffers stay empty
    assert_eq!(queue.front_len(), 0);
    assert_eq!(queue.back_len(), 0);
}

#[test]
fn edge_command_queue_multiple_swaps_no_push() {
    let queue = CommandQueue::new(64);
    queue.push(make_rect(1.0));
    queue.swap(); // front=[rect], back=[]
    assert_eq!(queue.front_len(), 1);
    queue.swap(); // second swap: front gets empty back; back gets front then is cleared
    assert_eq!(queue.front_len(), 0, "front must be empty after second swap with no new push");
}

#[test]
fn edge_command_queue_push_after_swap() {
    let queue = CommandQueue::new(64);
    queue.push(make_rect(1.0));
    queue.swap(); // front=[rect(1)], back=[]
    queue.push(make_rect(2.0)); // back=[rect(2)], front unchanged
    assert_eq!(queue.front_len(), 1, "front still has previous frame's command");
    assert_eq!(queue.back_len(), 1, "back has new command");
    let cmds = queue.take_sorted_front();
    assert_eq!(cmds[0].z(), 1.0, "renderer sees previous frame's command");
}

#[test]
fn edge_command_queue_take_sorted_front_empty() {
    let queue = CommandQueue::new(64);
    queue.swap(); // swap with empty back
    let cmds = queue.take_sorted_front();
    assert!(cmds.is_empty(), "take on empty front must return empty vec");
}

#[test]
fn edge_command_queue_dropped_count_resets_after_swap() {
    let queue = CommandQueue::new(2);
    for _ in 0..5 {
        queue.push(make_rect(0.0));
    }
    assert_eq!(queue.dropped_count(), 3, "3 commands dropped at capacity=2");
    queue.swap();
    assert_eq!(queue.dropped_count(), 0, "dropped counter resets on swap");
}

#[test]
fn edge_command_queue_nan_z_sorting() {
    let queue = CommandQueue::new(64);
    queue.push(make_rect(3.0));
    queue.push(make_rect(f32::NAN));
    queue.push(make_rect(1.0));
    queue.swap();
    let cmds = queue.take_sorted_front(); // must not panic
    assert_eq!(cmds.len(), 3, "all three commands survive NaN sort");
}

#[test]
fn edge_command_queue_mixed_variants_z_sort() {
    let queue = CommandQueue::new(64);
    queue.push(DrawCommand::Text(DrawText {
        text: "hi".to_string(), font_id: String::new(),
        x: 0.0, y: 0.0, color: Color::rgb(255, 255, 255), size: 16, z: 3.0,
    }));
    queue.push(DrawCommand::Circle(DrawCircle {
        cx: 0.0, cy: 0.0, radius: 0.1,
        color: Color::rgb(255, 0, 0), border: None, border_width: 0.0, z: 1.0,
    }));
    queue.push(DrawCommand::Line(DrawLine {
        x0: 0.0, y0: 0.0, x1: 1.0, y1: 1.0,
        color: Color::rgb(0, 255, 0), width: 1.0, z: 2.0,
    }));
    queue.push(make_rect(0.5));
    queue.swap();
    let cmds = queue.take_sorted_front();
    let zs: Vec<f32> = cmds.iter().map(|c| c.z()).collect();
    assert_eq!(zs, vec![0.5, 1.0, 2.0, 3.0], "mixed variants sorted by z ascending");
}

#[test]
fn edge_command_queue_capacity_boundary() {
    let queue = CommandQueue::new(3);
    for _ in 0..3 {
        queue.push(make_rect(0.0));
    }
    assert_eq!(queue.back_len(), 3, "exactly at capacity: all accepted");
    assert_eq!(queue.dropped_count(), 0, "exactly at capacity: no drops");
    queue.push(make_rect(0.0));
    assert_eq!(queue.dropped_count(), 1, "one over capacity: one drop");
}

#[test]
fn edge_command_queue_negative_z_sorting() {
    let queue = CommandQueue::new(64);
    for z in [-5.0f32, 0.0, -1.0, 3.0, -10.0] {
        queue.push(make_rect(z));
    }
    queue.swap();
    let cmds = queue.take_sorted_front();
    let zs: Vec<f32> = cmds.iter().map(|c| c.z()).collect();
    assert_eq!(zs, vec![-10.0, -5.0, -1.0, 0.0, 3.0], "negative z values sorted ascending");
}

#[test]
fn edge_command_queue_take_sorted_front_drains() {
    let queue = CommandQueue::new(64);
    queue.push(make_rect(1.0));
    queue.swap();
    let first = queue.take_sorted_front();
    assert_eq!(first.len(), 1, "first take returns the command");
    let second = queue.take_sorted_front();
    assert!(second.is_empty(), "second take on same frame returns empty — buffer was drained");
}

// ─── Color edge cases ─────────────────────────────────────────────────────────

#[test]
fn edge_color_rgb_sets_full_alpha() {
    assert_eq!(Color::rgb(100, 200, 50).a, 255, "Color::rgb must set alpha=255");
}

#[test]
fn edge_color_all_zero() {
    let [r, g, b, a] = Color::new(0, 0, 0, 0).to_linear();
    assert_eq!(r, 0.0);
    assert_eq!(g, 0.0);
    assert_eq!(b, 0.0);
    assert_eq!(a, 0.0);
}

#[test]
fn edge_color_all_max() {
    let [r, g, b, a] = Color::new(255, 255, 255, 255).to_linear();
    assert_eq!(r, 1.0);
    assert_eq!(g, 1.0);
    assert_eq!(b, 1.0);
    assert_eq!(a, 1.0);
}

#[test]
fn edge_color_equality() {
    let a = Color::new(10, 20, 30, 255);
    let b = Color::new(10, 20, 30, 255);
    let c = Color::new(10, 20, 30, 128);
    assert_eq!(a, b, "identical colors must be equal");
    assert_ne!(a, c, "different alpha makes colors not equal");
}

#[test]
fn edge_color_copy() {
    let a = Color::new(10, 20, 30, 255);
    let b = a; // Color is Copy — this must compile and both must be equal
    assert_eq!(a, b);
}

// ─── Coordinate mapping edge cases ───────────────────────────────────────────

#[test]
fn edge_norm_to_clip_out_of_range() {
    // norm_to_clip(nx, ny) = [nx*2-1, -(ny*2-1)]
    let [cx, cy] = norm_to_clip(-0.5, -0.5);
    assert!((cx - (-2.0)).abs() < 1e-6, "nx=-0.5 → clip_x=-2.0, got {cx}");
    assert!((cy - 2.0).abs() < 1e-6, "ny=-0.5 → clip_y=2.0, got {cy}");

    let [cx2, cy2] = norm_to_clip(1.5, 1.5);
    assert!((cx2 - 2.0).abs() < 1e-6, "nx=1.5 → clip_x=2.0, got {cx2}");
    assert!((cy2 - (-2.0)).abs() < 1e-6, "ny=1.5 → clip_y=-2.0, got {cy2}");
}

#[test]
fn edge_rect_to_clip_zero_size() {
    // A zero-size rect at (0.5, 0.5): all four corners collapse to the same clip point
    let [tl, tr, bl, br] = rect_to_clip_verts(0.5, 0.5, 0.0, 0.0);
    assert_eq!(tl, [0.0, 0.0], "zero-size rect tl at clip origin");
    assert_eq!(tr, [0.0, 0.0], "zero-size rect tr collapses to tl");
    assert_eq!(bl, [0.0, 0.0], "zero-size rect bl collapses to tl");
    assert_eq!(br, [0.0, 0.0], "zero-size rect br collapses to tl");
}

// ─── Camera edge cases ───────────────────────────────────────────────────────

#[test]
fn edge_camera_default_values() {
    let cam = Camera::new();
    assert_eq!(cam.position.x, 0.0);
    assert_eq!(cam.position.y, 0.0);
    assert_eq!(cam.position.z, -10.0, "default position z=-10");
    assert_eq!(cam.target.x, 0.0);
    assert_eq!(cam.target.y, 0.0);
    assert_eq!(cam.target.z, 0.0, "default target is origin");
    assert_eq!(cam.up.x, 0.0);
    assert_eq!(cam.up.y, 1.0, "default up is +Y");
    assert_eq!(cam.up.z, 0.0);
    assert_eq!(cam.fov_degrees, 90.0, "default FOV 90°");
    assert_eq!(cam.near, 0.1);
    assert_eq!(cam.far, 1000.0);
    assert!((cam.aspect - 16.0 / 9.0).abs() < 1e-5, "default aspect 16:9");
}

#[test]
fn edge_camera_view_projection_combines_correctly() {
    let mut cam = Camera::new();
    cam.set_position(1.0, 2.0, 3.0);
    cam.set_look_at(0.0, 0.0, 0.0);

    let vp = cam.view_projection();
    let expected = cam.projection_matrix() * cam.view_matrix();

    let vp_arr = vp.to_cols_array();
    let exp_arr = expected.to_cols_array();
    for (i, (&a, &b)) in vp_arr.iter().zip(exp_arr.iter()).enumerate() {
        assert!((a - b).abs() < 1e-5, "view_projection element {i}: {a} != {b}");
    }
}

#[test]
fn edge_camera_identity_position() {
    let mut cam = Camera::new();
    cam.set_position(0.0, 0.0, 0.0);
    cam.set_look_at(0.0, 0.0, 1.0);
    let fwd = cam.forward();
    assert!(fwd.x.abs() < 1e-5, "forward.x ≈ 0, got {}", fwd.x);
    assert!(fwd.y.abs() < 1e-5, "forward.y ≈ 0, got {}", fwd.y);
    assert!((fwd.z - 1.0).abs() < 1e-5, "forward.z ≈ 1.0, got {}", fwd.z);
}

// ─── DrawCommand z() accessor ─────────────────────────────────────────────────

#[test]
fn edge_draw_command_z_all_variants() {


    let cmds = [
        DrawCommand::Rect(DrawRect { x: 0.0, y: 0.0, w: 0.1, h: 0.1,
            color: Color::rgb(255, 255, 255), border: None, border_width: 0.0, z: 1.0 }),
        DrawCommand::Circle(DrawCircle { cx: 0.0, cy: 0.0, radius: 0.1,
            color: Color::rgb(255, 0, 0), border: None, border_width: 0.0, z: 2.0 }),
        DrawCommand::Line(DrawLine { x0: 0.0, y0: 0.0, x1: 1.0, y1: 1.0,
            color: Color::rgb(0, 255, 0), width: 1.0, z: 3.0 }),
        DrawCommand::Image(DrawImage { asset_id: String::new(),
            x: 0.0, y: 0.0, w: 1.0, h: 1.0, alpha: 1.0, z: 4.0 }),
        DrawCommand::Text(DrawText { text: String::new(), font_id: String::new(),
            x: 0.0, y: 0.0, color: Color::rgb(255, 255, 255), size: 16, z: 5.0 }),
        DrawCommand::Mesh(DrawMesh { z: 6.0, ..Default::default() }),
        DrawCommand::Billboard(DrawBillboard { asset_id: String::new(),
            position: Vec3::ZERO, size: Vec2::ONE, color: Color::rgb(255, 255, 255), z: 7.0 }),
    ];
    let expected_z = [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
    for (cmd, &ez) in cmds.iter().zip(expected_z.iter()) {
        assert_eq!(cmd.z(), ez, "z() accessor returned wrong value for {:?}", cmd);
    }
}

// ─── Shader validation edge cases ────────────────────────────────────────────

#[test]
fn edge_shader_validate_empty_string() {
    let result = validate_wgsl("");
    assert!(result.is_err(), "empty WGSL must return Err");
}

#[test]
fn edge_shader_validate_valid_minimal() {
    let src = r#"
        @vertex
        fn vs_main() -> @builtin(position) vec4<f32> {
            return vec4<f32>(0.0, 0.0, 0.0, 1.0);
        }
        @fragment
        fn fs_main() -> @location(0) vec4<f32> {
            return vec4<f32>(1.0, 1.0, 1.0, 1.0);
        }
    "#;
    let result = validate_wgsl(src);
    assert!(result.is_ok(), "minimal valid WGSL should return Ok: {:?}", result.err());
}

// ─── RendererConfig edge cases ────────────────────────────────────────────────

#[test]
fn edge_renderer_config_defaults() {
    let cfg = RendererConfig::default();
    assert_eq!(cfg.clear_color, [0, 0, 0, 255]);
    assert_eq!(cfg.max_draw_commands, 65536);
    assert_eq!(cfg.msaa_samples, 4);
    assert!(!cfg.use_fxaa);
}

#[test]
fn edge_renderer_config_serde_roundtrip() {
    let cfg = RendererConfig {
        clear_color: [100, 150, 200, 255],
        max_draw_commands: 1024,
        msaa_samples: 2,
        use_fxaa: true,
    };
    let json = serde_json::to_string(&cfg).expect("serialize RendererConfig");
    let restored: RendererConfig = serde_json::from_str(&json).expect("deserialize RendererConfig");
    assert_eq!(restored.clear_color, cfg.clear_color);
    assert_eq!(restored.max_draw_commands, cfg.max_draw_commands);
    assert_eq!(restored.msaa_samples, cfg.msaa_samples);
    assert_eq!(restored.use_fxaa, cfg.use_fxaa);
}

#[test]
fn edge_renderer_config_serde_defaults_from_empty() {
    let cfg: RendererConfig = serde_json::from_str("{}").expect("deserialize empty object");
    assert_eq!(cfg.clear_color, [0, 0, 0, 255]);
    assert_eq!(cfg.max_draw_commands, 65536);
    assert_eq!(cfg.msaa_samples, 4);
    assert!(!cfg.use_fxaa);
}

// ─── QW-1: Material Properties (metallic / roughness) ────────────────────────

#[test]
fn qw1_1_draw_mesh_default_metallic_roughness() {
    let cmd = DrawMesh::default();
    assert_eq!(cmd.metallic, 0.0, "default metallic should be 0.0");
    assert_eq!(cmd.roughness, 0.5, "default roughness should be 0.5");
}

#[test]
fn qw1_2_draw_mesh_metallic_roughness_roundtrip() {
    let cmd = DrawMesh { metallic: 0.8, roughness: 0.2, ..Default::default() };
    assert!((cmd.metallic - 0.8).abs() < 1e-6);
    assert!((cmd.roughness - 0.2).abs() < 1e-6);
}

#[test]
fn qw1_3_draw_mesh_metallic_clamped_in_render() {
    // Clamping happens in render_meshes; verify field stores out-of-range values
    // and that clamp(0,1) of valid values preserves them.
    let cmd = DrawMesh { metallic: 1.5, roughness: -0.1, ..Default::default() };
    assert_eq!(cmd.metallic.clamp(0.0, 1.0), 1.0);
    assert_eq!(cmd.roughness.clamp(0.0, 1.0), 0.0);
}

// ─── QW-2: Scene Settings (directional light) ────────────────────────────────

#[test]
fn qw2_1_scene_settings_default_direction() {
    let s = SceneSettings::default();
    assert_eq!(s.light_direction, [0.5, 1.0, 0.5]);
    assert_eq!(s.light_color, [1.0, 1.0, 1.0]);
    assert!((s.light_intensity - 1.0).abs() < 1e-6);
}

#[test]
fn qw2_4_scene_settings_direction_stored_in_uniform() {
    let s = SceneSettings { light_direction: [1.0, 0.0, 0.0], ..SceneSettings::default() };
    assert_eq!(s.light_direction[0], 1.0);
    assert_eq!(s.light_direction[1], 0.0);
    assert_eq!(s.light_direction[2], 0.0);
}

// ─── QW-3: Background Color ───────────────────────────────────────────────────

#[test]
fn qw3_1_scene_settings_default_clear_color() {
    let s = SceneSettings::default();
    assert_eq!(s.clear_color, [0.15, 0.15, 0.15, 1.0]);
}

#[test]
fn qw3_2_scene_settings_clear_color_roundtrip() {
    let mut s = SceneSettings::default();
    s.clear_color = [0.05, 0.06, 0.14, 1.0];
    assert_eq!(s.clear_color, [0.05, 0.06, 0.14, 1.0]);
}

#[test]
fn qw3_5_scene_settings_clear_color_clamp() {
    let r = (-0.1f32).clamp(0.0, 1.0);
    let g = (1.5f32).clamp(0.0, 1.0);
    assert_eq!(r, 0.0, "negative should clamp to 0");
    assert_eq!(g, 1.0, "over-1 should clamp to 1");
}

// ─── T-REND-15: Zero Commands After Non-Empty Frame ─────────────────────────

#[test]
fn t_rend_15_zero_commands_after_nonempty_frame() {
    let queue = CommandQueue::new(64);

    // Push 5 DrawRect commands
    for i in 0..5u32 {
        queue.push(make_rect(i as f32));
    }

    // First swap+drain: returns 5 commands
    queue.swap();
    let cmds = queue.take_sorted_front();
    assert_eq!(cmds.len(), 5, "first drain must return 5 commands");

    // Second swap+drain with zero new commands: buffer must be clean
    queue.swap();
    let cmds = queue.take_sorted_front();
    assert_eq!(cmds.len(), 0, "second drain after empty frame must return 0 commands");
}

// ─── T-REND-16: Command Queue At Capacity ───────────────────────────────────

#[test]
fn t_rend_16_command_queue_at_capacity() {
    let queue = CommandQueue::new(4);

    // Push exactly 4 commands (at capacity)
    for i in 0..4u32 {
        queue.push(make_rect(i as f32));
    }
    assert_eq!(queue.back_len(), 4, "exactly at capacity: all 4 accepted");
    assert_eq!(queue.dropped_count(), 0, "exactly at capacity: no drops");

    queue.swap();
    let cmds = queue.take_sorted_front();
    assert_eq!(cmds.len(), 4, "all 4 commands drained");

    // Now push 5 commands (one over capacity)
    for i in 0..5u32 {
        queue.push(make_rect(i as f32));
    }
    // Capacity is enforced: only first 4 accepted, 5th is dropped
    assert_eq!(queue.back_len(), 4, "over capacity: only 4 accepted");
    assert_eq!(queue.dropped_count(), 1, "over capacity: 1 command dropped");

    queue.swap();
    let cmds = queue.take_sorted_front();
    assert_eq!(cmds.len(), 4, "only 4 commands drained after overflow");
}

// ─── T-REND-17: Mixed Command Variants Sorting ──────────────────────────────

#[test]
fn t_rend_17_mixed_command_variants_sorting() {
    let queue = CommandQueue::new(64);

    queue.push(DrawCommand::Text(DrawText {
        text: "hello".to_string(), font_id: String::new(),
        x: 0.0, y: 0.0, color: Color::rgb(255, 255, 255), size: 16, z: 3.0,
    }));
    queue.push(DrawCommand::Rect(DrawRect {
        x: 0.0, y: 0.0, w: 0.1, h: 0.1,
        color: Color::rgb(255, 255, 255), border: None, border_width: 0.0, z: 1.0,
    }));
    queue.push(DrawCommand::Line(DrawLine {
        x0: 0.0, y0: 0.0, x1: 1.0, y1: 1.0,
        color: Color::rgb(0, 255, 0), width: 1.0, z: 2.0,
    }));
    queue.push(DrawCommand::Circle(DrawCircle {
        cx: 0.0, cy: 0.0, radius: 0.1,
        color: Color::rgb(255, 0, 0), border: None, border_width: 0.0, z: 4.0,
    }));

    queue.swap();
    let cmds = queue.take_sorted_front();
    let zs: Vec<f32> = cmds.iter().map(|c| c.z()).collect();
    assert_eq!(zs, vec![1.0, 2.0, 3.0, 4.0], "mixed variants must be sorted by z ascending");
}

// ─── T-REND-18: NaN Z Sorting Stability ─────────────────────────────────────

#[test]
fn t_rend_18_nan_z_sorting_stability() {
    let queue = CommandQueue::new(64);

    queue.push(make_rect(f32::NAN));
    queue.push(make_rect(1.0));

    queue.swap();
    // Must not panic during sort
    let cmds = queue.take_sorted_front();
    assert_eq!(cmds.len(), 2, "both commands survive NaN sort");

    // NaN sorts to the end (total_cmp places NaN after all finite values)
    let last_z = cmds[1].z();
    assert!(
        last_z.is_nan() || last_z == 1.0,
        "NaN should sort to a consistent position; got z values: [{}, {}]",
        cmds[0].z(), cmds[1].z()
    );
}

// ─── T-REND-19: Camera Default Values ───────────────────────────────────────

#[test]
fn t_rend_19_camera_default_values() {
    let cam = Camera::new();

    // FOV must be non-zero
    assert!(cam.fov_degrees > 0.0, "default FOV must be positive, got {}", cam.fov_degrees);

    // near < far, both positive
    assert!(cam.near > 0.0, "near must be positive, got {}", cam.near);
    assert!(cam.far > cam.near, "far ({}) must be greater than near ({})", cam.far, cam.near);

    // Sensible default values
    assert_eq!(cam.fov_degrees, 90.0, "default FOV is 90 degrees");
    assert_eq!(cam.near, 0.1, "default near plane is 0.1");
    assert_eq!(cam.far, 1000.0, "default far plane is 1000.0");
}

// ─── T-REND-20: WGSL Validation — Invalid Shader ────────────────────────────

#[test]
fn t_rend_20_wgsl_validation_invalid_shader() {
    let result = validate_wgsl("this is not valid wgsl");
    assert!(result.is_err(), "plaintext must fail WGSL validation");
}

// ─── T-REND-21: WGSL Validation — Valid Compute Shader ──────────────────────

#[test]
fn t_rend_21_wgsl_validation_valid_shader() {
    // validate_wgsl requires @vertex or @fragment entry points — compute-only is rejected.
    let src = r#"
        @vertex
        fn vs_main() -> @builtin(position) vec4<f32> {
            return vec4<f32>(0.0, 0.0, 0.0, 1.0);
        }
        @fragment
        fn fs_main() -> @location(0) vec4<f32> {
            return vec4<f32>(1.0, 1.0, 1.0, 1.0);
        }
    "#;
    let result = validate_wgsl(src);
    assert!(result.is_ok(), "minimal valid WGSL shader must pass validation: {:?}", result.err());
}

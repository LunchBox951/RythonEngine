use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use rython_ecs::{DrawCommand, RenderSystem, Scene, TransformSystem};
use rython_renderer::{Camera, DrawMesh, RendererState};

use crate::viewport::ViewportTexture;

/// Raw vertex for grid geometry: position + normal + uv (32 bytes, matches mesh pipeline).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
}

const GRID_MESH_ID: &str = "__editor_grid__";
const GRID_HALF_EXTENT: f32 = 10.0; // ±10 units → 20×20 grid
const GRID_STEP: f32 = 1.0;
const GRID_LINE_HALF_WIDTH: f32 = 0.01;

/// Generate a XZ-plane grid as a triangle mesh.
///
/// Each grid line is represented as a thin quad (two triangles) lying flat on Y=0.
fn generate_grid_vertices_indices() -> (Vec<u8>, Vec<u32>) {
    let mut verts: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    let normal = [0.0_f32, 1.0, 0.0];
    let steps = ((GRID_HALF_EXTENT * 2.0 / GRID_STEP) as i32) + 1;
    let start = -GRID_HALF_EXTENT;

    let mut push_line_quad = |x0: f32, z0: f32, x1: f32, z1: f32| {
        let base = verts.len() as u32;
        // Direction perpendicular to the line (in XZ)
        let dx = x1 - x0;
        let dz = z1 - z0;
        let len = (dx * dx + dz * dz).sqrt().max(1e-6);
        let px = -dz / len * GRID_LINE_HALF_WIDTH;
        let pz = dx / len * GRID_LINE_HALF_WIDTH;

        verts.push(Vertex { position: [x0 - px, 0.0, z0 - pz], normal, uv: [0.0, 0.0] });
        verts.push(Vertex { position: [x0 + px, 0.0, z0 + pz], normal, uv: [1.0, 0.0] });
        verts.push(Vertex { position: [x1 - px, 0.0, z1 - pz], normal, uv: [0.0, 1.0] });
        verts.push(Vertex { position: [x1 + px, 0.0, z1 + pz], normal, uv: [1.0, 1.0] });

        // Two triangles: 0,2,1 and 1,2,3
        indices.extend_from_slice(&[base, base + 2, base + 1, base + 1, base + 2, base + 3]);
    };

    for i in 0..steps {
        let t = start + i as f32 * GRID_STEP;
        // Line along X axis at z=t
        push_line_quad(-GRID_HALF_EXTENT, t, GRID_HALF_EXTENT, t);
        // Line along Z axis at x=t
        push_line_quad(t, -GRID_HALF_EXTENT, t, GRID_HALF_EXTENT);
    }

    let bytes: Vec<u8> = bytemuck::cast_slice(&verts).to_vec();
    (bytes, indices)
}

/// Ensure the grid mesh is uploaded to the renderer's mesh cache.
fn ensure_grid_mesh(renderer: &mut RendererState) {
    if renderer.mesh_cache.contains_key(GRID_MESH_ID) {
        return;
    }
    let (vbytes, indices) = generate_grid_vertices_indices();
    renderer.upload_mesh(GRID_MESH_ID, &vbytes, &indices);
}

/// Show the 3D viewport panel. Returns the egui Response for the image area
/// (used by the camera controller).
///
/// Performs the full render pipeline each frame:
/// 1. Ensure the offscreen texture matches the available panel size.
/// 2. Ensure the grid mesh is in the GPU cache.
/// 3. Run ECS TransformSystem + RenderSystem.
/// 4. Clear the offscreen texture.
/// 5. Render meshes (scene + grid) into the offscreen texture.
/// 6. Display the texture.
pub fn show(
    ui: &mut egui::Ui,
    render_state: &egui_wgpu::RenderState,
    renderer: &mut RendererState,
    scene: &Arc<Scene>,
    viewport_texture: &mut Option<ViewportTexture>,
    camera: &mut Camera,
) -> egui::Response {
    let available = ui.available_rect_before_wrap();
    let ppp = ui.ctx().pixels_per_point();
    let px_w = ((available.width() * ppp) as u32).max(1);
    let px_h = ((available.height() * ppp) as u32).max(1);

    let device = &render_state.device;
    let mut egui_renderer = render_state.renderer.write();

    // Resize or create offscreen texture
    match viewport_texture {
        Some(ref mut vt) => {
            vt.resize_if_needed(device, &mut egui_renderer, px_w, px_h);
        }
        None => {
            *viewport_texture = Some(ViewportTexture::new(device, &mut egui_renderer, px_w, px_h));
        }
    }
    drop(egui_renderer);

    let vt = viewport_texture.as_ref().unwrap();

    // Update camera aspect ratio
    camera.aspect = px_w as f32 / px_h as f32;

    // Ensure grid mesh
    ensure_grid_mesh(renderer);

    // ECS: compute world transforms + draw commands
    let world_transforms = TransformSystem::run(&scene.components, &scene.hierarchy);
    let ecs_commands = RenderSystem::run(&scene.components, &world_transforms);

    // Convert ECS DrawCommands → renderer DrawMesh commands
    let mut draw_meshes: Vec<DrawMesh> = ecs_commands
        .into_iter()
        .filter_map(|cmd| match cmd {
            DrawCommand::DrawMesh { mesh_id, texture_id, transform, .. } => Some(DrawMesh {
                mesh_id,
                texture_id,
                transform,
                z: 0.0,
            }),
            _ => None,
        })
        .collect();

    // Add grid
    draw_meshes.push(DrawMesh {
        mesh_id: GRID_MESH_ID.to_string(),
        texture_id: String::new(),
        transform: glam::Mat4::IDENTITY,
        z: 0.0,
    });

    // Clear the offscreen texture
    {
        let mut encoder = renderer
            .gpu
            .device
            .create_command_encoder(&Default::default());
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("viewport_clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &vt.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.15,
                            g: 0.15,
                            b: 0.15,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }
        renderer.gpu.queue.submit(std::iter::once(encoder.finish()));
    }

    // Render meshes into offscreen texture
    renderer.ensure_depth_texture(px_w, px_h);
    if !draw_meshes.is_empty() {
        renderer.render_meshes(&draw_meshes, camera, &vt.view);
    }

    // Note: renderer uses its own queue ref (cloned from eframe's device on init).

    // Display the texture
    let tex_id = vt.egui_texture_id;
    let display_size = egui::vec2(available.width(), available.height());
    let image = egui::Image::new(egui::load::SizedTexture::new(tex_id, display_size))
        .sense(egui::Sense::drag());

    ui.add_sized(display_size, image)
}

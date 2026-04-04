use glam::Mat4;
use std::collections::HashMap;

use crate::component::{BillboardComponent, ComponentStorage, MeshComponent};
use crate::entity::EntityId;
use crate::systems::transform::WorldTransform;

/// Draw command submitted to the renderer's command buffer.
#[derive(Debug, Clone)]
pub enum DrawCommand {
    DrawMesh {
        entity: EntityId,
        mesh_id: String,
        texture_id: String,
        normal_map_id: Option<String>,
        specular_map_id: Option<String>,
        emissive_map_id: Option<String>,
        yaw_offset: f32,
        shininess: f32,
        specular_color: [f32; 3],
        emissive_color: [f32; 4],
        emissive_intensity: f32,
        metallic: f32,
        roughness: f32,
        transform: Mat4,
    },
    DrawBillboard {
        entity: EntityId,
        asset_id: String,
        width: f32,
        height: f32,
        uv_rect: [f32; 4],
        alpha: f32,
        transform: Mat4,
    },
}

pub struct RenderSystem;

impl RenderSystem {
    /// Generate draw commands for all visible entities.
    pub fn run(
        components: &ComponentStorage,
        world_transforms: &HashMap<EntityId, WorldTransform>,
    ) -> Vec<DrawCommand> {
        let mut commands = Vec::new();

        // Mesh entities
        components.for_each::<MeshComponent, _>(|entity, mesh| {
            if !mesh.visible {
                return;
            }
            let transform = world_transforms
                .get(&entity)
                .map(|wt| wt.matrix)
                .unwrap_or(Mat4::IDENTITY);
            commands.push(DrawCommand::DrawMesh {
                entity,
                mesh_id: mesh.mesh_id.clone(),
                texture_id: mesh.texture_id.clone(),
                normal_map_id: mesh.normal_map_id.clone(),
                specular_map_id: mesh.specular_map_id.clone(),
                emissive_map_id: mesh.emissive_map_id.clone(),
                yaw_offset: mesh.yaw_offset,
                shininess: mesh.shininess,
                specular_color: mesh.specular_color,
                emissive_color: mesh.emissive_color,
                emissive_intensity: mesh.emissive_intensity.max(0.0),
                metallic: mesh.metallic.clamp(0.0, 1.0),
                roughness: mesh.roughness.clamp(0.0, 1.0),
                transform,
            });
        });

        // Billboard entities
        components.for_each::<BillboardComponent, _>(|entity, bb| {
            let transform = world_transforms
                .get(&entity)
                .map(|wt| wt.matrix)
                .unwrap_or(Mat4::IDENTITY);
            commands.push(DrawCommand::DrawBillboard {
                entity,
                asset_id: bb.asset_id.clone(),
                width: bb.width,
                height: bb.height,
                uv_rect: bb.uv_rect,
                alpha: bb.alpha,
                transform,
            });
        });

        commands
    }
}

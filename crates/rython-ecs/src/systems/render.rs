use std::collections::HashMap;
use glam::Mat4;

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
        yaw_offset: f32,
        shininess: f32,
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
                yaw_offset: mesh.yaw_offset,
                shininess: mesh.shininess,
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

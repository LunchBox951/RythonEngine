use std::collections::HashMap;
use glam::{Mat4, Quat, Vec3};

use crate::component::TransformComponent;
use crate::entity::EntityId;
use crate::hierarchy::{Hierarchy, MAX_HIERARCHY_DEPTH};
use crate::component::ComponentStorage;

/// World-space transform cache, keyed by entity ID.
#[derive(Debug, Clone)]
pub struct WorldTransform {
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: f32,
    /// Composed 4x4 matrix for the renderer.
    pub matrix: Mat4,
}

impl WorldTransform {
    pub fn identity() -> Self {
        Self {
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: 1.0,
            matrix: Mat4::IDENTITY,
        }
    }
}

pub struct TransformSystem;

impl TransformSystem {
    /// Run the transform system: compute world transforms for all entities
    /// that have a TransformComponent.
    pub fn run(
        components: &ComponentStorage,
        hierarchy: &Hierarchy,
    ) -> HashMap<EntityId, WorldTransform> {
        let entities = components.entities_with::<TransformComponent>();
        let mut cache: HashMap<EntityId, WorldTransform> = HashMap::new();

        for entity in entities {
            if !cache.contains_key(&entity) {
                Self::compute_world_transform(entity, components, hierarchy, &mut cache);
            }
        }

        cache
    }

    fn compute_world_transform(
        entity: EntityId,
        components: &ComponentStorage,
        hierarchy: &Hierarchy,
        cache: &mut HashMap<EntityId, WorldTransform>,
    ) {
        // Build ancestor chain
        let (chain, depth_exceeded) = hierarchy.ancestor_chain(entity);

        if depth_exceeded {
            eprintln!(
                "[TransformSystem] WARNING: entity {} exceeded max hierarchy depth of {}. \
                 World transform capped.",
                entity, MAX_HIERARCHY_DEPTH
            );
        }

        // Walk from root down, composing transforms
        // chain[0] = entity, chain[last] = topmost ancestor
        // We need to go root-first, so iterate in reverse
        let mut parent_world = WorldTransform::identity();

        for ancestor in chain.iter().rev() {
            if let Some(cached) = cache.get(ancestor) {
                parent_world = cached.clone();
                continue;
            }
            let local = components
                .get::<TransformComponent>(*ancestor)
                .unwrap_or_default();

            let local_pos = Vec3::new(local.x, local.y, local.z);
            let local_rot = Quat::from_euler(
                glam::EulerRot::XYZ,
                local.rot_x,
                local.rot_y,
                local.rot_z,
            );
            let local_scale = local.scale;

            // Compose with parent world transform
            let world_scale = parent_world.scale * local_scale;
            let world_rot = parent_world.rotation * local_rot;
            let world_pos = parent_world.position
                + parent_world.rotation * (parent_world.scale * local_pos);

            let matrix = Mat4::from_scale_rotation_translation(
                Vec3::splat(world_scale),
                world_rot,
                world_pos,
            );

            let wt = WorldTransform {
                position: world_pos,
                rotation: world_rot,
                scale: world_scale,
                matrix,
            };
            cache.insert(*ancestor, wt.clone());
            parent_world = wt;
        }
    }
}

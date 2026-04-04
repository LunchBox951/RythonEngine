use std::collections::HashMap;

use crate::component::{ComponentStorage, LightComponent, LightKind};
use crate::entity::EntityId;
use crate::systems::transform::WorldTransform;

/// A single enabled light collected from the ECS, ready for GPU conversion.
///
/// `kind`: 0=directional, 1=point, 2=spot
#[derive(Clone, Debug)]
pub struct CollectedLight {
    pub kind: u32,
    /// World-space position (point/spot); zero for directional.
    pub position: [f32; 3],
    /// Direction toward light (directional) or spotlight aim (spot); zero for point.
    pub direction: [f32; 3],
    pub color: [f32; 3],
    pub intensity: f32,
    /// Effective range (point/spot).
    pub radius: f32,
    /// Cosine of inner cone half-angle (spot only).
    pub inner_cos: f32,
    /// Cosine of outer cone half-angle (spot only).
    pub outer_cos: f32,
}

pub struct LightSystem;

impl LightSystem {
    /// Collect all enabled `LightComponent` entities into GPU-ready form.
    ///
    /// When more than 16 enabled lights are present the 16 with the highest
    /// intensity are kept (others are silently dropped).
    pub fn run(
        components: &ComponentStorage,
        world_transforms: &HashMap<EntityId, WorldTransform>,
    ) -> Vec<CollectedLight> {
        let mut lights: Vec<CollectedLight> = Vec::new();

        components.for_each::<LightComponent, _>(|entity, light| {
            if !light.enabled {
                return;
            }
            let pos = world_transforms
                .get(&entity)
                .map(|wt| [wt.position.x, wt.position.y, wt.position.z])
                .unwrap_or([0.0, 0.0, 0.0]);

            let cl = match &light.kind {
                LightKind::Directional { direction } => CollectedLight {
                    kind: 0,
                    position: [0.0; 3],
                    direction: *direction,
                    color: light.color,
                    intensity: light.intensity,
                    radius: 0.0,
                    inner_cos: 0.0,
                    outer_cos: 0.0,
                },
                LightKind::Point { radius } => CollectedLight {
                    kind: 1,
                    position: pos,
                    direction: [0.0; 3],
                    color: light.color,
                    intensity: light.intensity,
                    radius: *radius,
                    inner_cos: 0.0,
                    outer_cos: 0.0,
                },
                LightKind::Spot {
                    direction,
                    inner_angle,
                    outer_angle,
                } => CollectedLight {
                    kind: 2,
                    position: pos,
                    direction: *direction,
                    color: light.color,
                    intensity: light.intensity,
                    radius: 0.0,
                    inner_cos: inner_angle.to_radians().cos(),
                    outer_cos: outer_angle.to_radians().cos(),
                },
            };
            lights.push(cl);
        });

        // Keep the brightest lights when over the GPU limit.
        lights.sort_by(|a, b| {
            b.intensity
                .partial_cmp(&a.intensity)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        lights.truncate(16);
        lights
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::ComponentStorage;
    use crate::entity::EntityId;
    use glam::{Mat4, Quat, Vec3};

    fn id(n: u64) -> EntityId {
        EntityId(n)
    }

    #[test]
    fn no_lights_returns_empty() {
        let cs = ComponentStorage::new();
        let lights = LightSystem::run(&cs, &HashMap::new());
        assert!(lights.is_empty());
    }

    #[test]
    fn directional_light_has_kind_zero() {
        let cs = ComponentStorage::new();
        cs.insert(
            id(1),
            LightComponent {
                kind: LightKind::Directional {
                    direction: [0.0, 1.0, 0.0],
                },
                color: [1.0, 1.0, 1.0],
                intensity: 1.0,
                enabled: true,
                cast_shadows: false,
            },
        );
        let lights = LightSystem::run(&cs, &HashMap::new());
        assert_eq!(lights.len(), 1);
        assert_eq!(lights[0].kind, 0);
        assert_eq!(lights[0].direction, [0.0, 1.0, 0.0]);
    }

    #[test]
    fn disabled_light_is_excluded() {
        let cs = ComponentStorage::new();
        cs.insert(
            id(1),
            LightComponent {
                kind: LightKind::Point { radius: 5.0 },
                color: [1.0, 0.0, 0.0],
                intensity: 2.0,
                enabled: false,
                cast_shadows: false,
            },
        );
        cs.insert(
            id(2),
            LightComponent {
                kind: LightKind::Point { radius: 5.0 },
                color: [0.0, 1.0, 0.0],
                intensity: 1.0,
                enabled: true,
                cast_shadows: false,
            },
        );
        let lights = LightSystem::run(&cs, &HashMap::new());
        assert_eq!(lights.len(), 1);
    }

    #[test]
    fn excess_lights_capped_at_16() {
        let cs = ComponentStorage::new();
        for i in 0..20u64 {
            cs.insert(
                id(i),
                LightComponent {
                    kind: LightKind::Point { radius: 10.0 },
                    color: [1.0, 1.0, 1.0],
                    intensity: i as f32,
                    enabled: true,
                    cast_shadows: false,
                },
            );
        }
        let lights = LightSystem::run(&cs, &HashMap::new());
        assert_eq!(lights.len(), 16);
    }

    #[test]
    fn spot_light_converts_angles_to_cosines() {
        use std::f32::consts::PI;
        let cs = ComponentStorage::new();
        cs.insert(
            id(1),
            LightComponent {
                kind: LightKind::Spot {
                    direction: [0.0, -1.0, 0.0],
                    inner_angle: 15.0,
                    outer_angle: 30.0,
                },
                color: [1.0, 1.0, 1.0],
                intensity: 1.0,
                enabled: true,
                cast_shadows: false,
            },
        );
        let lights = LightSystem::run(&cs, &HashMap::new());
        assert_eq!(lights.len(), 1);
        assert_eq!(lights[0].kind, 2);
        let expected_inner = (15.0f32 * PI / 180.0).cos();
        let expected_outer = (30.0f32 * PI / 180.0).cos();
        assert!((lights[0].inner_cos - expected_inner).abs() < 1e-5);
        assert!((lights[0].outer_cos - expected_outer).abs() < 1e-5);
    }

    #[test]
    fn point_light_uses_world_position() {
        let cs = ComponentStorage::new();
        cs.insert(
            id(1),
            LightComponent {
                kind: LightKind::Point { radius: 5.0 },
                color: [1.0, 1.0, 1.0],
                intensity: 1.0,
                enabled: true,
                cast_shadows: false,
            },
        );
        let mut wt = HashMap::new();
        wt.insert(
            id(1),
            WorldTransform {
                position: Vec3::new(3.0, 2.0, 1.0),
                rotation: Quat::IDENTITY,
                scale: Vec3::ONE,
                matrix: Mat4::IDENTITY,
            },
        );
        let lights = LightSystem::run(&cs, &wt);
        assert_eq!(lights.len(), 1);
        assert_eq!(lights[0].position, [3.0, 2.0, 1.0]);
    }

    #[test]
    fn excess_lights_brightest_are_kept() {
        let cs = ComponentStorage::new();
        for i in 0..20u64 {
            cs.insert(
                id(i),
                LightComponent {
                    kind: LightKind::Point { radius: 10.0 },
                    color: [1.0, 1.0, 1.0],
                    intensity: i as f32,
                    enabled: true,
                    cast_shadows: false,
                },
            );
        }
        let lights = LightSystem::run(&cs, &HashMap::new());
        assert_eq!(lights.len(), 16);
        // Highest intensities should be kept (19, 18, …, 4)
        assert!(lights[0].intensity >= 4.0);
        for w in lights.windows(2) {
            assert!(w[0].intensity >= w[1].intensity);
        }
    }
}

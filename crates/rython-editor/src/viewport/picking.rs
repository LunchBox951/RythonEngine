use std::sync::Arc;

use glam::{Vec3, Vec4};
use rython_ecs::{EntityId, Scene};
use rython_renderer::Camera;

/// A world-space ray.
pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
}

/// Unproject a viewport-local click position into a world-space ray.
///
/// `click_pos` — pixel position within the viewport (origin at top-left).
/// `viewport_size` — (width, height) in pixels.
pub fn ray_from_viewport_click(
    click_pos: egui::Pos2,
    viewport_rect: egui::Rect,
    camera: &Camera,
) -> Ray {
    let vw = viewport_rect.width();
    let vh = viewport_rect.height();

    // Normalize to [-1, 1] (NDC x goes left→right, y goes bottom→top)
    let local_x = click_pos.x - viewport_rect.min.x;
    let local_y = click_pos.y - viewport_rect.min.y;
    let ndc_x = (local_x / vw) * 2.0 - 1.0;
    let ndc_y = 1.0 - (local_y / vh) * 2.0;

    let inv_vp = camera.view_projection().inverse();

    // Unproject near and far plane points
    let near_ndc = Vec4::new(ndc_x, ndc_y, 0.0, 1.0);
    let far_ndc = Vec4::new(ndc_x, ndc_y, 1.0, 1.0);

    let near_world = inv_vp * near_ndc;
    let far_world = inv_vp * far_ndc;

    let near_pt = Vec3::new(
        near_world.x / near_world.w,
        near_world.y / near_world.w,
        near_world.z / near_world.w,
    );
    let far_pt = Vec3::new(
        far_world.x / far_world.w,
        far_world.y / far_world.w,
        far_world.z / far_world.w,
    );

    Ray {
        origin: near_pt,
        direction: (far_pt - near_pt).normalize(),
    }
}

/// Axis-aligned bounding box.
struct Aabb {
    min: Vec3,
    max: Vec3,
}

/// Ray-AABB slab test. Returns the distance to the nearest intersection or
/// `None` if no intersection.
fn ray_aabb(ray: &Ray, aabb: &Aabb) -> Option<f32> {
    let inv_dir = Vec3::new(
        1.0 / ray.direction.x,
        1.0 / ray.direction.y,
        1.0 / ray.direction.z,
    );
    let t1 = (aabb.min - ray.origin) * inv_dir;
    let t2 = (aabb.max - ray.origin) * inv_dir;

    let t_min = t1.min(t2);
    let t_max = t1.max(t2);

    let t_enter = t_min.x.max(t_min.y).max(t_min.z);
    let t_exit = t_max.x.min(t_max.y).min(t_max.z);

    if t_enter <= t_exit && t_exit > 0.0 {
        Some(t_enter.max(0.0))
    } else {
        None
    }
}

/// Pick the closest entity under a ray cast from the viewport click.
///
/// Iterates all entities with both `TransformComponent` and `MeshComponent`,
/// computes an axis-aligned bounding box scaled by the transform, and returns
/// the closest hit.
pub fn pick_entity(
    ray: &Ray,
    scene: &Arc<Scene>,
) -> Option<EntityId> {
    use rython_ecs::component::{MeshComponent, TransformComponent};

    let entities = scene.all_entities();
    let mut closest: Option<(EntityId, f32)> = None;

    for entity in entities {
        let Some(transform) = scene.components.get::<TransformComponent>(entity) else {
            continue;
        };
        let Some(_mesh) = scene.components.get::<MeshComponent>(entity) else {
            continue;
        };

        // Half-extents based on scale (unit cube scaled)
        let hx = transform.scale_x * 0.5;
        let hy = transform.scale_y * 0.5;
        let hz = transform.scale_z * 0.5;

        let center = Vec3::new(transform.x, transform.y, transform.z);
        let aabb = Aabb {
            min: center - Vec3::new(hx, hy, hz),
            max: center + Vec3::new(hx, hy, hz),
        };

        if let Some(dist) = ray_aabb(ray, &aabb) {
            match closest {
                Some((_, d)) if dist < d => closest = Some((entity, dist)),
                None => closest = Some((entity, dist)),
                _ => {}
            }
        }
    }

    closest.map(|(id, _)| id)
}

use glam::{Mat4, Vec3, Vec4};
use rython_ecs::component::TransformComponent;
use rython_ecs::EntityId;
use rython_renderer::Camera;

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GizmoMode {
    Translate,
    Rotate,
    Scale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GizmoSpace {
    World,
    Local,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GizmoAxis {
    X,
    Y,
    Z,
    /// Uniform scale center
    Center,
}

/// Active drag state for a gizmo interaction.
pub struct GizmoDrag {
    pub axis: GizmoAxis,
    pub entity: EntityId,
    /// The transform at drag-start, used for undo on release.
    pub initial_transform: TransformComponent,
    /// Mouse position at drag start (screen coords).
    pub start_mouse: egui::Pos2,
    /// Entity position projected to screen at drag start.
    pub origin_screen: egui::Pos2,
}

// ── Constants ─────────────────────────────────────────────────────────────────

const GIZMO_SIZE: f32 = 80.0; // pixels
const HIT_RADIUS: f32 = 10.0; // pixels

const COLOR_X: egui::Color32 = egui::Color32::from_rgb(220, 50, 50);
const COLOR_Y: egui::Color32 = egui::Color32::from_rgb(50, 200, 50);
const COLOR_Z: egui::Color32 = egui::Color32::from_rgb(50, 100, 220);
const COLOR_HOVER: egui::Color32 = egui::Color32::from_rgb(255, 230, 50);
const COLOR_CENTER: egui::Color32 = egui::Color32::from_rgb(200, 200, 200);
const ROTATE_RADIUS: f32 = 70.0; // pixels, radius of rotation circle
const ROTATE_SEGMENTS: usize = 32;

// ── World-to-screen helper ────────────────────────────────────────────────────

/// Project a world-space point through the camera VP matrix onto screen pixels.
/// Returns `None` if the point is behind the camera (w ≤ 0).
fn world_to_screen(
    world: Vec3,
    vp: Mat4,
    viewport_rect: egui::Rect,
) -> Option<egui::Pos2> {
    let clip = vp * Vec4::new(world.x, world.y, world.z, 1.0);
    if clip.w <= 0.0 {
        return None;
    }
    let ndc_x = clip.x / clip.w;
    let ndc_y = clip.y / clip.w;
    // NDC (-1..1) → viewport pixels
    let px = viewport_rect.min.x + (ndc_x + 1.0) * 0.5 * viewport_rect.width();
    let py = viewport_rect.min.y + (1.0 - (ndc_y + 1.0) * 0.5) * viewport_rect.height();
    Some(egui::Pos2::new(px, py))
}

/// Compute the screen-space axis direction (normalized 2D) for a world axis.
fn screen_axis_dir(
    origin_world: Vec3,
    axis_world: Vec3,
    vp: Mat4,
    viewport_rect: egui::Rect,
) -> Option<egui::Vec2> {
    let tip = world_to_screen(origin_world + axis_world, vp, viewport_rect)?;
    let base = world_to_screen(origin_world, vp, viewport_rect)?;
    let d = tip - base;
    let len = (d.x * d.x + d.y * d.y).sqrt();
    if len < 1e-4 {
        return None;
    }
    Some(egui::Vec2::new(d.x / len, d.y / len))
}

// ── Gizmo drawing ─────────────────────────────────────────────────────────────

/// Draw the gizmo for `entity` at `transform` into the egui Painter.
///
/// Returns the screen-space positions of the three axis tips (X, Y, Z) and
/// the origin, used later for hit-testing.
pub fn draw_gizmo(
    painter: &egui::Painter,
    mode: GizmoMode,
    entity: EntityId,
    transform: &TransformComponent,
    camera: &Camera,
    viewport_rect: egui::Rect,
    hovered_axis: Option<GizmoAxis>,
    active_drag: Option<&GizmoDrag>,
) {
    let vp = camera.view_projection();
    let entity_pos = Vec3::new(transform.x, transform.y, transform.z);

    let Some(origin_screen) = world_to_screen(entity_pos, vp, viewport_rect) else {
        return;
    };

    // Compute how far `GIZMO_SIZE` pixels should be in world units at this depth
    // by checking the scale of the axis tips relative to the screen.
    let axes: [(GizmoAxis, Vec3, egui::Color32); 3] = [
        (GizmoAxis::X, Vec3::X, COLOR_X),
        (GizmoAxis::Y, Vec3::Y, COLOR_Y),
        (GizmoAxis::Z, Vec3::Z, COLOR_Z),
    ];

    match mode {
        GizmoMode::Translate => {
            draw_translate_gizmo(
                painter, &axes, entity_pos, origin_screen, vp, viewport_rect,
                hovered_axis, active_drag,
            );
        }
        GizmoMode::Rotate => {
            draw_rotate_gizmo(
                painter, &axes, entity_pos, origin_screen, vp, viewport_rect,
                hovered_axis, active_drag,
            );
        }
        GizmoMode::Scale => {
            draw_scale_gizmo(
                painter, &axes, entity_pos, origin_screen, vp, viewport_rect,
                hovered_axis, active_drag,
            );
        }
    }

    let _ = entity;
}

fn axis_color(axis: GizmoAxis, base: egui::Color32, hovered: Option<GizmoAxis>, active: Option<&GizmoDrag>) -> egui::Color32 {
    if active.map(|d| d.axis == axis).unwrap_or(false) {
        return COLOR_HOVER;
    }
    if hovered == Some(axis) {
        return COLOR_HOVER;
    }
    base
}

fn draw_translate_gizmo(
    painter: &egui::Painter,
    axes: &[(GizmoAxis, Vec3, egui::Color32); 3],
    entity_pos: Vec3,
    origin: egui::Pos2,
    vp: Mat4,
    viewport_rect: egui::Rect,
    hovered: Option<GizmoAxis>,
    active: Option<&GizmoDrag>,
) {
    for (axis_id, axis_dir, base_color) in axes {
        // Find scale: project axis tip to screen, compute pixel distance
        let tip_world = entity_pos + *axis_dir;
        let Some(tip_screen) = world_to_screen(tip_world, vp, viewport_rect) else { continue };
        let raw_len = ((tip_screen.x - origin.x).powi(2) + (tip_screen.y - origin.y).powi(2)).sqrt();
        if raw_len < 1e-3 { continue; }
        // Scale so the arrow is exactly GIZMO_SIZE pixels
        let scale = GIZMO_SIZE / raw_len;
        let tip = egui::Pos2::new(
            origin.x + (tip_screen.x - origin.x) * scale,
            origin.y + (tip_screen.y - origin.y) * scale,
        );

        let color = axis_color(*axis_id, *base_color, hovered, active);
        let stroke = egui::Stroke::new(2.5, color);
        painter.line_segment([origin, tip], stroke);

        // Arrow head (cone tip triangle)
        let dir = egui::Vec2::new(tip.x - origin.x, tip.y - origin.y);
        let dlen = (dir.x * dir.x + dir.y * dir.y).sqrt().max(1e-4);
        let dhat = dir / dlen;
        let perp = egui::Vec2::new(-dhat.y, dhat.x) * 6.0;
        let cone_base = tip - dhat * 12.0;
        painter.add(egui::Shape::convex_polygon(
            vec![tip, cone_base + perp, cone_base - perp],
            color,
            egui::Stroke::NONE,
        ));
    }
}

fn draw_rotate_gizmo(
    painter: &egui::Painter,
    axes: &[(GizmoAxis, Vec3, egui::Color32); 3],
    entity_pos: Vec3,
    origin: egui::Pos2,
    vp: Mat4,
    viewport_rect: egui::Rect,
    hovered: Option<GizmoAxis>,
    active: Option<&GizmoDrag>,
) {
    // Draw rotation circles as polylines projected from 3D circle samples
    for (axis_id, axis_dir, base_color) in axes {
        let color = axis_color(*axis_id, *base_color, hovered, active);
        let stroke = egui::Stroke::new(2.0, color);

        // Build a tangent frame perpendicular to axis_dir
        let (t1, t2) = perpendicular_axes(*axis_dir);
        let mut pts: Vec<egui::Pos2> = Vec::with_capacity(ROTATE_SEGMENTS + 1);

        for i in 0..=ROTATE_SEGMENTS {
            let theta = (i as f32 / ROTATE_SEGMENTS as f32) * std::f32::consts::TAU;
            // Unit circle in the plane perpendicular to the axis
            let circle_world = entity_pos + (t1 * theta.cos() + t2 * theta.sin()) * 1.0;
            if let Some(sp) = world_to_screen(circle_world, vp, viewport_rect) {
                // Scale so radius ≈ ROTATE_RADIUS pixels at the current depth
                let raw_r = ((sp.x - origin.x).powi(2) + (sp.y - origin.y).powi(2)).sqrt();
                if raw_r < 1e-3 { pts.push(sp); continue; }
                // Actually just collect projected points; accept that radius varies with perspective
                pts.push(sp);
            }
        }

        // Scale all points so the average radius is ROTATE_RADIUS
        let avg_r: f32 = pts.iter().map(|p| {
            ((p.x - origin.x).powi(2) + (p.y - origin.y).powi(2)).sqrt()
        }).sum::<f32>() / pts.len() as f32;

        if avg_r > 1e-3 {
            let scale = ROTATE_RADIUS / avg_r;
            let scaled: Vec<egui::Pos2> = pts.iter().map(|p| {
                egui::Pos2::new(
                    origin.x + (p.x - origin.x) * scale,
                    origin.y + (p.y - origin.y) * scale,
                )
            }).collect();
            painter.add(egui::Shape::line(scaled, stroke));
        }
    }

    let _ = viewport_rect;
}

fn draw_scale_gizmo(
    painter: &egui::Painter,
    axes: &[(GizmoAxis, Vec3, egui::Color32); 3],
    entity_pos: Vec3,
    origin: egui::Pos2,
    vp: Mat4,
    viewport_rect: egui::Rect,
    hovered: Option<GizmoAxis>,
    active: Option<&GizmoDrag>,
) {
    for (axis_id, axis_dir, base_color) in axes {
        let tip_world = entity_pos + *axis_dir;
        let Some(tip_screen) = world_to_screen(tip_world, vp, viewport_rect) else { continue };
        let raw_len = ((tip_screen.x - origin.x).powi(2) + (tip_screen.y - origin.y).powi(2)).sqrt();
        if raw_len < 1e-3 { continue; }
        let scale = GIZMO_SIZE / raw_len;
        let tip = egui::Pos2::new(
            origin.x + (tip_screen.x - origin.x) * scale,
            origin.y + (tip_screen.y - origin.y) * scale,
        );

        let color = axis_color(*axis_id, *base_color, hovered, active);
        let stroke = egui::Stroke::new(2.5, color);
        painter.line_segment([origin, tip], stroke);

        // Cube end-cap (8x8 square)
        painter.rect_filled(
            egui::Rect::from_center_size(tip, egui::Vec2::splat(8.0)),
            0.0,
            color,
        );
    }

    // Center cube for uniform scale
    let center_color = axis_color(GizmoAxis::Center, COLOR_CENTER, hovered, active);
    painter.rect_filled(
        egui::Rect::from_center_size(origin, egui::Vec2::splat(10.0)),
        0.0,
        center_color,
    );
}

/// Build two unit vectors perpendicular to `n`.
fn perpendicular_axes(n: Vec3) -> (Vec3, Vec3) {
    let up = if n.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
    let t1 = n.cross(up).normalize();
    let t2 = n.cross(t1);
    (t1, t2)
}

// ── Hit testing ───────────────────────────────────────────────────────────────

/// Hit-test the gizmo axes against a mouse position.
///
/// Returns the closest axis within `HIT_RADIUS` pixels, or `None`.
pub fn hit_test_gizmo(
    mode: GizmoMode,
    transform: &TransformComponent,
    camera: &Camera,
    viewport_rect: egui::Rect,
    mouse: egui::Pos2,
) -> Option<GizmoAxis> {
    let vp = camera.view_projection();
    let entity_pos = Vec3::new(transform.x, transform.y, transform.z);
    let Some(origin) = world_to_screen(entity_pos, vp, viewport_rect) else {
        return None;
    };

    let axes = [(GizmoAxis::X, Vec3::X), (GizmoAxis::Y, Vec3::Y), (GizmoAxis::Z, Vec3::Z)];

    match mode {
        GizmoMode::Translate | GizmoMode::Scale => {
            let mut best: Option<(GizmoAxis, f32)> = None;
            for (axis_id, axis_dir) in &axes {
                let tip_world = entity_pos + *axis_dir;
                let Some(tip_screen) = world_to_screen(tip_world, vp, viewport_rect) else { continue };
                let raw_len = ((tip_screen.x - origin.x).powi(2) + (tip_screen.y - origin.y).powi(2)).sqrt();
                if raw_len < 1e-3 { continue; }
                let scale = GIZMO_SIZE / raw_len;
                let tip = egui::Pos2::new(
                    origin.x + (tip_screen.x - origin.x) * scale,
                    origin.y + (tip_screen.y - origin.y) * scale,
                );

                let dist = point_to_segment_dist(mouse, origin, tip);
                if dist < HIT_RADIUS {
                    match best {
                        Some((_, d)) if dist < d => best = Some((*axis_id, dist)),
                        None => best = Some((*axis_id, dist)),
                        _ => {}
                    }
                }
            }
            // For scale: also check center cube
            if mode == GizmoMode::Scale {
                let center_dist = ((mouse.x - origin.x).powi(2) + (mouse.y - origin.y).powi(2)).sqrt();
                if center_dist < HIT_RADIUS * 1.2 {
                    match best {
                        Some((_, d)) if center_dist < d => best = Some((GizmoAxis::Center, center_dist)),
                        None => best = Some((GizmoAxis::Center, center_dist)),
                        _ => {}
                    }
                }
            }
            best.map(|(a, _)| a)
        }
        GizmoMode::Rotate => {
            let mut best: Option<(GizmoAxis, f32)> = None;
            for (axis_id, axis_dir) in &axes {
                let (t1, t2) = perpendicular_axes(*axis_dir);
                // Sample the projected circle and find the closest point
                let min_dist = (0..ROTATE_SEGMENTS)
                    .filter_map(|i| {
                        let theta = (i as f32 / ROTATE_SEGMENTS as f32) * std::f32::consts::TAU;
                        let circle_world = entity_pos + (t1 * theta.cos() + t2 * theta.sin());
                        let sp = world_to_screen(circle_world, vp, viewport_rect)?;
                        // Scale to ROTATE_RADIUS
                        let raw_r = ((sp.x - origin.x).powi(2) + (sp.y - origin.y).powi(2)).sqrt();
                        if raw_r < 1e-3 { return None; }
                        let scale = ROTATE_RADIUS / raw_r;
                        let scaled = egui::Pos2::new(
                            origin.x + (sp.x - origin.x) * scale,
                            origin.y + (sp.y - origin.y) * scale,
                        );
                        Some(((mouse.x - scaled.x).powi(2) + (mouse.y - scaled.y).powi(2)).sqrt())
                    })
                    .fold(f32::INFINITY, f32::min);

                if min_dist < HIT_RADIUS {
                    match best {
                        Some((_, d)) if min_dist < d => best = Some((*axis_id, min_dist)),
                        None => best = Some((*axis_id, min_dist)),
                        _ => {}
                    }
                }
            }
            best.map(|(a, _)| a)
        }
    }
}

/// Minimum distance from point `p` to segment `[a, b]`.
fn point_to_segment_dist(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let ab = egui::Vec2::new(b.x - a.x, b.y - a.y);
    let ap = egui::Vec2::new(p.x - a.x, p.y - a.y);
    let len2 = ab.x * ab.x + ab.y * ab.y;
    if len2 < 1e-6 {
        return (ap.x * ap.x + ap.y * ap.y).sqrt();
    }
    let t = ((ap.x * ab.x + ap.y * ab.y) / len2).clamp(0.0, 1.0);
    let proj = egui::Vec2::new(ab.x * t, ab.y * t);
    let diff = egui::Vec2::new(ap.x - proj.x, ap.y - proj.y);
    (diff.x * diff.x + diff.y * diff.y).sqrt()
}

// ── Drag update helpers ───────────────────────────────────────────────────────

/// Apply a translate drag along a world axis.
///
/// Returns the new (x, y, z) position.
pub fn apply_translate_drag(
    drag: &GizmoDrag,
    current_mouse: egui::Pos2,
    camera: &Camera,
    viewport_rect: egui::Rect,
) -> (f32, f32, f32) {
    let t = &drag.initial_transform;
    let entity_pos = Vec3::new(t.x, t.y, t.z);

    let axis_world = match drag.axis {
        GizmoAxis::X => Vec3::X,
        GizmoAxis::Y => Vec3::Y,
        GizmoAxis::Z => Vec3::Z,
        GizmoAxis::Center => return (t.x, t.y, t.z),
    };

    // Project mouse delta onto the screen-space projection of the axis
    let vp = camera.view_projection();
    let Some(screen_dir) = screen_axis_dir(entity_pos, axis_world, vp, viewport_rect) else {
        return (t.x, t.y, t.z);
    };

    let delta = current_mouse - drag.start_mouse;
    let screen_dot = delta.x * screen_dir.x + delta.y * screen_dir.y;

    // Convert pixels to world units: use the projected length of a unit world vector
    let Some(origin_screen) = world_to_screen(entity_pos, vp, viewport_rect) else {
        return (t.x, t.y, t.z);
    };
    let Some(axis_tip_screen) = world_to_screen(entity_pos + axis_world, vp, viewport_rect) else {
        return (t.x, t.y, t.z);
    };
    let pix_per_unit = ((axis_tip_screen.x - origin_screen.x).powi(2)
        + (axis_tip_screen.y - origin_screen.y).powi(2))
    .sqrt();
    if pix_per_unit < 1e-3 {
        return (t.x, t.y, t.z);
    }

    let world_delta = screen_dot / pix_per_unit;
    let new_pos = entity_pos + axis_world * world_delta;
    (new_pos.x, new_pos.y, new_pos.z)
}

/// Apply a rotate drag around a world axis.
///
/// Returns the new (rot_x, rot_y, rot_z) in radians.
pub fn apply_rotate_drag(
    drag: &GizmoDrag,
    current_mouse: egui::Pos2,
    camera: &Camera,
    viewport_rect: egui::Rect,
) -> (f32, f32, f32) {
    let t = &drag.initial_transform;
    let entity_pos = Vec3::new(t.x, t.y, t.z);

    let (axis_idx, axis_world): (usize, Vec3) = match drag.axis {
        GizmoAxis::X => (0, Vec3::X),
        GizmoAxis::Y => (1, Vec3::Y),
        GizmoAxis::Z => (2, Vec3::Z),
        GizmoAxis::Center => return (t.rot_x, t.rot_y, t.rot_z),
    };

    // Compute screen-space tangent of the rotation circle at the drag start
    let vp = camera.view_projection();
    let (tan1, _) = perpendicular_axes(axis_world);
    let Some(screen_tangent) = screen_axis_dir(entity_pos, tan1, vp, viewport_rect) else {
        return (t.rot_x, t.rot_y, t.rot_z);
    };

    let delta = current_mouse - drag.start_mouse;
    let angle_delta = (delta.x * screen_tangent.x + delta.y * screen_tangent.y) * 0.01;

    let rots = [t.rot_x, t.rot_y, t.rot_z];
    let mut new_rots = rots;
    new_rots[axis_idx] = rots[axis_idx] + angle_delta;
    (new_rots[0], new_rots[1], new_rots[2])
}

/// Apply a scale drag along a world axis.
///
/// Returns the new (scale_x, scale_y, scale_z).
pub fn apply_scale_drag(
    drag: &GizmoDrag,
    current_mouse: egui::Pos2,
    camera: &Camera,
    viewport_rect: egui::Rect,
) -> (f32, f32, f32) {
    let t = &drag.initial_transform;
    let entity_pos = Vec3::new(t.x, t.y, t.z);

    let axis_world = match drag.axis {
        GizmoAxis::X | GizmoAxis::Y | GizmoAxis::Z => match drag.axis {
            GizmoAxis::X => Vec3::X,
            GizmoAxis::Y => Vec3::Y,
            _ => Vec3::Z,
        },
        GizmoAxis::Center => {
            // Uniform scale: project onto average screen direction
            let delta = current_mouse - drag.start_mouse;
            let uniform_delta = (delta.x - delta.y) * 0.005;
            let factor = 1.0 + uniform_delta;
            return (
                (t.scale_x * factor).max(0.001),
                (t.scale_y * factor).max(0.001),
                (t.scale_z * factor).max(0.001),
            );
        }
    };

    let vp = camera.view_projection();
    let Some(screen_dir) = screen_axis_dir(entity_pos, axis_world, vp, viewport_rect) else {
        return (t.scale_x, t.scale_y, t.scale_z);
    };

    let delta = current_mouse - drag.start_mouse;
    let screen_dot = delta.x * screen_dir.x + delta.y * screen_dir.y;
    let factor = 1.0 + screen_dot * 0.005;

    match drag.axis {
        GizmoAxis::X => ((t.scale_x * factor).max(0.001), t.scale_y, t.scale_z),
        GizmoAxis::Y => (t.scale_x, (t.scale_y * factor).max(0.001), t.scale_z),
        GizmoAxis::Z => (t.scale_x, t.scale_y, (t.scale_z * factor).max(0.001)),
        GizmoAxis::Center => unreachable!(),
    }
}

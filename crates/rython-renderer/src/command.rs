use rython_core::math::{Mat4, Vec2, Vec3};

/// RGBA color with 0–255 components.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Map each component from 0–255 to 0.0–1.0.
    pub fn to_linear(&self) -> [f32; 4] {
        [
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
            self.a as f32 / 255.0,
        ]
    }
}

/// Filled or bordered rectangle in normalized screen space.
#[derive(Debug, Clone)]
pub struct DrawRect {
    /// Left edge in normalized screen space [0, 1].
    pub x: f32,
    /// Top edge in normalized screen space [0, 1].
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub color: Color,
    /// Optional border color; `None` means filled with no border.
    pub border: Option<Color>,
    pub border_width: f32,
    pub z: f32,
}

/// Filled or bordered circle in normalized screen space.
#[derive(Debug, Clone)]
pub struct DrawCircle {
    pub cx: f32,
    pub cy: f32,
    pub radius: f32,
    pub color: Color,
    pub border: Option<Color>,
    pub border_width: f32,
    pub z: f32,
}

/// Line segment in normalized screen space.
#[derive(Debug, Clone)]
pub struct DrawLine {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub color: Color,
    pub width: f32,
    pub z: f32,
}

/// Textured quad from a loaded image asset.
#[derive(Debug, Clone)]
pub struct DrawImage {
    pub asset_id: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub alpha: f32,
    pub z: f32,
}

/// Text string rendered with a loaded font.
#[derive(Debug, Clone)]
pub struct DrawText {
    pub text: String,
    pub font_id: String,
    pub x: f32,
    pub y: f32,
    pub color: Color,
    pub size: u32,
    pub z: f32,
}

/// 3D mesh with material and world transform (Phase 3).
#[derive(Debug, Clone)]
pub struct DrawMesh {
    pub mesh_id: String,
    pub material_id: String,
    pub transform: Mat4,
    pub z: f32,
}

/// Camera-facing sprite in 3D space (Phase 3).
#[derive(Debug, Clone)]
pub struct DrawBillboard {
    pub asset_id: String,
    pub position: Vec3,
    pub size: Vec2,
    pub color: Color,
    pub z: f32,
}

/// All renderable command variants.
#[derive(Debug, Clone)]
pub enum DrawCommand {
    Rect(DrawRect),
    Circle(DrawCircle),
    Line(DrawLine),
    Image(DrawImage),
    Text(DrawText),
    Mesh(DrawMesh),
    Billboard(DrawBillboard),
}

impl DrawCommand {
    /// Z-value for draw ordering (painter's algorithm: lower z drawn first).
    pub fn z(&self) -> f32 {
        match self {
            DrawCommand::Rect(c) => c.z,
            DrawCommand::Circle(c) => c.z,
            DrawCommand::Line(c) => c.z,
            DrawCommand::Image(c) => c.z,
            DrawCommand::Text(c) => c.z,
            DrawCommand::Mesh(c) => c.z,
            DrawCommand::Billboard(c) => c.z,
        }
    }
}

/// Map normalized screen coordinates [0, 1] to NDC clip space [-1, 1].
///
/// Origin (0, 0) is top-left. Y is flipped: normalized Y=0 maps to clip Y=+1.
pub fn norm_to_clip(nx: f32, ny: f32) -> [f32; 2] {
    [nx * 2.0 - 1.0, -(ny * 2.0 - 1.0)]
}

/// Generate the four clip-space corner vertices for a normalized-space rect.
///
/// Returns `[top-left, top-right, bottom-left, bottom-right]`.
pub fn rect_to_clip_verts(x: f32, y: f32, w: f32, h: f32) -> [[f32; 2]; 4] {
    [
        norm_to_clip(x, y),
        norm_to_clip(x + w, y),
        norm_to_clip(x, y + h),
        norm_to_clip(x + w, y + h),
    ]
}

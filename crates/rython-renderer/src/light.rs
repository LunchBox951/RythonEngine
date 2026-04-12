//! GPU-side light types for the multi-light pipeline (§5).
//!
//! `LightBuffer` replaces `DirectionalLightUniform` at bind group 5.
//! Layout: 16 lights × 64B + 4B count + 12B ambient = 1040B (16-byte aligned).

pub const MAX_LIGHTS: usize = 16;

/// One light on the GPU — 64 bytes (4 × vec4<f32>).
///
/// Layout mirrors the WGSL `GpuLight` struct exactly:
///   position_or_dir [f32;4]: xyz=world pos (point/spot) or direction (dir); w=type (0/1/2)
///   color_intensity  [f32;4]: xyz=linear RGB, w=intensity
///   spot_params      [f32;4]: x=inner_cos, y=outer_cos, z=radius, w=enabled (1.0=on)
///   spot_dir_pad     [f32;4]: xyz=spot direction (kind==2 only), w=unused
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuLight {
    pub position_or_dir: [f32; 4],
    pub color_intensity: [f32; 4],
    pub spot_params: [f32; 4],
    pub spot_dir_pad: [f32; 4],
}

/// Full light buffer uploaded to bind group 5 each frame — 1040 bytes.
///
/// Rust repr(C) layout:
///   lights[16]:    offset    0, size 1024
///   light_count:   offset 1024, size    4
///   ambient[3]:    offset 1028, size   12
/// Total: 1040 bytes. 1040 % 16 == 0. ✓
///
/// The corresponding WGSL struct uses three separate `f32` members for
/// `ambient_r/g/b` (not `vec3`) to avoid vec3's 16-byte alignment requirement
/// and keep both layouts identical.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightBuffer {
    pub lights: [GpuLight; MAX_LIGHTS],
    pub light_count: u32,
    pub ambient: [f32; 3],
}

impl LightBuffer {
    /// All-zero buffer — no lights, black ambient. Produces a fully dark scene.
    pub fn empty() -> Self {
        bytemuck::Zeroable::zeroed()
    }

    /// Fallback scene buffer: one directional light at [0.5, 1.0, 0.5] + 0.1 ambient.
    ///
    /// Used when no `LightComponent` entities are present in the scene.
    pub fn default_scene() -> Self {
        let mut buf = Self::empty();
        buf.ambient = [0.1, 0.1, 0.1];
        buf.lights[0] = GpuLight {
            position_or_dir: [0.5, 1.0, 0.5, 0.0], // w=0 → directional
            color_intensity: [1.0, 1.0, 1.0, 1.0],
            spot_params: [0.0, 0.0, 0.0, 1.0], // w=1 → enabled
            spot_dir_pad: [0.0; 4],
        };
        buf.light_count = 1;
        buf
    }
}

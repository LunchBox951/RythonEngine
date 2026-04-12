/// Shadow mapping types and utilities — §3.
///
/// Provides `ShadowMap` (GPU depth texture), `LightMatrices` (orthographic
/// light-space VP matrix), `ShadowSettings` (runtime config), and the
/// `LightShadowUniform` GPU-side struct bound at mesh shader group(7).
use glam::{Mat4, Vec3};

/// GPU depth texture used as the shadow map.
///
/// Rendered to during the shadow pass (depth-only, from the light's POV).
/// Sampled in the mesh main pass via a comparison sampler at bind group 7.
pub struct ShadowMap {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler, // comparison sampler (LessEqual)
    pub depth_format: wgpu::TextureFormat,
    pub size: u32, // square resolution in pixels
}

impl ShadowMap {
    pub fn new(device: &wgpu::Device, size: u32) -> Self {
        let depth_format = wgpu::TextureFormat::Depth32Float;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow_map"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: depth_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow_comparison_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        Self {
            texture,
            view,
            sampler,
            depth_format,
            size,
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, size: u32) {
        *self = Self::new(device, size);
    }
}

/// Orthographic light-space view-projection matrix for directional shadow mapping.
pub struct LightMatrices {
    pub view_proj: Mat4,
    pub bias: f32,
}

impl LightMatrices {
    /// Build orthographic VP from a directional light direction.
    ///
    /// `scene_center` and `scene_radius` define the ortho frustum bounds.
    /// The light camera looks from `scene_center - direction * radius` toward `scene_center`.
    pub fn from_directional(
        direction: Vec3,
        scene_center: Vec3,
        scene_radius: f32,
        bias: f32,
    ) -> Self {
        let norm_dir = direction.normalize_or(Vec3::Y);
        let light_pos = scene_center - norm_dir * scene_radius;
        // Avoid gimbal lock when light points nearly straight up or down.
        let up = if norm_dir.y.abs() > 0.9 {
            Vec3::Z
        } else {
            Vec3::Y
        };
        let view = Mat4::look_at_rh(light_pos, scene_center, up);
        let r = scene_radius;
        let proj = Mat4::orthographic_rh(-r, r, -r, r, 0.0, r * 2.0);
        Self {
            view_proj: proj * view,
            bias,
        }
    }
}

/// Shadow rendering settings — configurable from Python via `rython.renderer`.
#[derive(Debug, Clone)]
pub struct ShadowSettings {
    /// Enable shadow casting from the primary directional light. Default: false.
    pub enabled: bool,
    /// Shadow map resolution in pixels (square). Default: 2048.
    pub map_size: u32,
    /// Depth bias to prevent shadow acne. Default: 0.005.
    pub bias: f32,
    /// PCF sample count: 1 = single sample, ≥4 = 3×3 kernel. Default: 4.
    pub pcf_samples: u32,
}

impl Default for ShadowSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            map_size: 2048,
            bias: 0.005,
            pcf_samples: 4,
        }
    }
}

/// GPU-side shadow uniform uploaded to mesh shader group(7) binding(0).
///
/// Layout — 80 bytes total, matches WGSL `LightShadowUniform`:
///   light_view_proj: mat4x4<f32>  [0–63]    64 B
///   bias:            f32           [64–67]    4 B
///   pcf_samples:     u32           [68–71]    4 B
///   shadow_enabled:  u32           [72–75]    4 B  (0 = disabled, 1 = enabled)
///   _pad:            u32           [76–79]    4 B
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightShadowUniform {
    pub light_view_proj: [[f32; 4]; 4],
    pub bias: f32,
    pub pcf_samples: u32,
    pub shadow_enabled: u32,
    pub _pad: u32,
}

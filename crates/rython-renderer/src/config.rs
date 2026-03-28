use serde::{Deserialize, Serialize};

// ── SceneSettings ─────────────────────────────────────────────────────────────

/// Runtime scene rendering settings — configurable from Python via `rython.renderer`.
///
/// These values override defaults on a per-frame basis and are separate from
/// [`RendererConfig`] (which is read from engine.json at startup).
#[derive(Debug, Clone)]
pub struct SceneSettings {
    /// Framebuffer clear color RGBA in linear [0, 1] space.
    pub clear_color: [f32; 4],
    /// World-space direction of the directional light (normalized on set).
    pub light_direction: [f32; 3],
    /// RGB color of the directional light (linear [0, 1]).
    pub light_color: [f32; 3],
    /// Scalar intensity multiplier for the directional light.
    pub light_intensity: f32,
}

impl Default for SceneSettings {
    fn default() -> Self {
        Self {
            clear_color: [0.15, 0.15, 0.15, 1.0],
            light_direction: [0.5, 1.0, 0.5],
            light_color: [1.0, 1.0, 1.0],
            light_intensity: 1.0,
        }
    }
}

// ── RendererConfig ────────────────────────────────────────────────────────────

fn default_clear_color() -> [u8; 4] {
    [0, 0, 0, 255]
}

fn default_max_draw_commands() -> usize {
    65536
}

fn default_msaa_samples() -> u32 {
    4
}

/// Renderer configuration (maps to the `renderer` section of engine.json).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RendererConfig {
    /// Framebuffer clear color in RGBA 0–255.
    #[serde(default = "default_clear_color")]
    pub clear_color: [u8; 4],
    /// Maximum draw commands per frame (pre-allocated buffer ceiling).
    #[serde(default = "default_max_draw_commands")]
    pub max_draw_commands: usize,
    /// MSAA sample count (1 = disabled).
    #[serde(default = "default_msaa_samples")]
    pub msaa_samples: u32,
    /// Enable FXAA post-processing (Phase 3).
    #[serde(default)]
    pub use_fxaa: bool,
}

impl Default for RendererConfig {
    fn default() -> Self {
        Self {
            clear_color: default_clear_color(),
            max_draw_commands: default_max_draw_commands(),
            msaa_samples: default_msaa_samples(),
            use_fxaa: false,
        }
    }
}

use serde::{Deserialize, Serialize};

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

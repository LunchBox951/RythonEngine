use serde::{Deserialize, Serialize};

fn default_fps() -> u32 {
    60
}
fn default_spin_threshold() -> u64 {
    1000
}

/// Configuration for the task scheduler (read from engine.json).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    #[serde(default = "default_fps")]
    pub target_fps: u32,
    pub parallel_threads: Option<usize>,
    #[serde(default = "default_spin_threshold")]
    pub spin_threshold_us: u64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            target_fps: default_fps(),
            parallel_threads: None,
            spin_threshold_us: default_spin_threshold(),
        }
    }
}

fn default_width() -> u32 {
    1280
}
fn default_height() -> u32 {
    720
}

/// Window configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowConfig {
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default = "default_height")]
    pub height: u32,
    #[serde(default)]
    pub fullscreen: bool,
    #[serde(default)]
    pub vsync: bool,
    #[serde(default = "default_title")]
    pub title: String,
}

fn default_title() -> String {
    "RythonEngine".to_string()
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            width: default_width(),
            height: default_height(),
            fullscreen: false,
            vsync: false,
            title: default_title(),
        }
    }
}

/// Top-level engine configuration loaded from engine.json.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EngineConfig {
    #[serde(default)]
    pub scheduler: SchedulerConfig,
    #[serde(default)]
    pub window: WindowConfig,
}

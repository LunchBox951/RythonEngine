use rython_core::EngineConfig;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default)]
pub struct ProjectConfig {
    pub name: String,
    pub version: String,
    /// Scene filename without extension.
    pub default_scene: Option<String>,
    /// Python entry point module name.
    pub entry_point: Option<String>,
    #[serde(default)]
    pub engine_config: EngineConfig,
}

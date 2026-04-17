use serde::{Deserialize, Serialize};

use crate::config::EngineConfig;

/// Records that a Python script/class is associated with a tagged entity.
/// Stored under `script_associations` in `project.json`.
#[derive(Serialize, Deserialize, Default, Clone)]
pub struct ScriptAssociation {
    /// The entity's primary tag (e.g. "player").
    pub entity_tag: String,
    /// Script filename relative to `scripts/` (e.g. "player.py").
    pub script: String,
    /// Python class name within the script (e.g. "Player").
    pub class: String,
}

/// Project metadata, read from `project.json`.
#[derive(Serialize, Deserialize, Default)]
pub struct ProjectConfig {
    pub name: String,
    pub version: String,
    /// Scene filename without extension.
    pub default_scene: Option<String>,
    /// Python entry point module name.
    ///
    /// Editor-only metadata. Release binaries bake the entry point at compile
    /// time (see `rython-cli/src/release_seal.rs`) so that a tampered
    /// `project.json` cannot redirect execution to attacker-controlled code.
    pub entry_point: Option<String>,
    #[serde(default)]
    pub engine_config: EngineConfig,
    /// Script-to-entity associations (metadata only — editor does not run Python).
    #[serde(default)]
    pub script_associations: Vec<ScriptAssociation>,
}

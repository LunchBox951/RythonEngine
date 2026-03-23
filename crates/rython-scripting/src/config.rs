use serde::{Deserialize, Serialize};

/// Configuration for the scripting system.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ScriptingConfig {
    Dev {
        script_dir: String,
        entry_point: Option<String>,
    },
    Release {
        bundle_path: String,
    },
}

impl Default for ScriptingConfig {
    fn default() -> Self {
        Self::Dev {
            script_dir: "./scripts".to_string(),
            entry_point: None,
        }
    }
}

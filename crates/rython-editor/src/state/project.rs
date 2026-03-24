use std::path::PathBuf;

use crate::project::ProjectConfig;

pub struct ProjectState {
    pub root_dir: Option<PathBuf>,
    pub config: ProjectConfig,
    pub open_scene_name: Option<String>,
    /// Set by any scene mutation; cleared on save.
    pub dirty: bool,
}

impl Default for ProjectState {
    fn default() -> Self {
        Self {
            root_dir: None,
            config: ProjectConfig::default(),
            open_scene_name: None,
            dirty: false,
        }
    }
}

impl ProjectState {
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Title-bar string showing project name + dirty indicator.
    pub fn title(&self) -> String {
        let name = if self.config.name.is_empty() {
            "Untitled"
        } else {
            &self.config.name
        };
        if self.dirty {
            format!("{}*", name)
        } else {
            name.to_string()
        }
    }
}

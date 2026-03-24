use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// ── Enums ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EditorTheme {
    Dark,
    Light,
}

impl Default for EditorTheme {
    fn default() -> Self {
        EditorTheme::Dark
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AutoSaveInterval {
    Off,
    OneMin,
    FiveMin,
    TenMin,
}

impl Default for AutoSaveInterval {
    fn default() -> Self {
        AutoSaveInterval::Off
    }
}

/// Mirrors `GizmoMode` without depending on the viewport crate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DefaultGizmoMode {
    Translate,
    Rotate,
    Scale,
}

impl Default for DefaultGizmoMode {
    fn default() -> Self {
        DefaultGizmoMode::Translate
    }
}

// ── Preferences ───────────────────────────────────────────────────────────────

/// Editor preferences, persisted to `~/.config/rython-editor/preferences.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preferences {
    #[serde(default)]
    pub theme: EditorTheme,
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    /// Viewport background colour as linear RGB [0..1].
    #[serde(default = "default_viewport_bg")]
    pub viewport_bg: [f32; 3],
    #[serde(default = "default_grid_spacing")]
    pub grid_spacing: f32,
    #[serde(default)]
    pub auto_save_interval: AutoSaveInterval,
    #[serde(default)]
    pub default_gizmo_mode: DefaultGizmoMode,
    #[serde(default)]
    pub external_editor_command: String,
}

fn default_font_size() -> f32 {
    14.0
}
fn default_viewport_bg() -> [f32; 3] {
    [0.1, 0.1, 0.1]
}
fn default_grid_spacing() -> f32 {
    1.0
}

impl Default for Preferences {
    fn default() -> Self {
        Preferences {
            theme: EditorTheme::Dark,
            font_size: default_font_size(),
            viewport_bg: default_viewport_bg(),
            grid_spacing: default_grid_spacing(),
            auto_save_interval: AutoSaveInterval::Off,
            default_gizmo_mode: DefaultGizmoMode::Translate,
            external_editor_command: String::new(),
        }
    }
}

impl Preferences {
    pub fn config_path() -> Option<PathBuf> {
        config_base().map(|b| b.join("rython-editor").join("preferences.json"))
    }

    pub fn load() -> Self {
        let path = match Self::config_path() {
            Some(p) => p,
            None => return Self::default(),
        };
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(path) = Self::config_path() else { return };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&path, json);
        }
    }
}

// ── RecentProjects ────────────────────────────────────────────────────────────

/// Persisted recent project list (`~/.config/rython-editor/recent.json`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecentProjects {
    pub recent_projects: Vec<PathBuf>,
}

impl RecentProjects {
    const MAX_ENTRIES: usize = 10;

    pub fn config_path() -> Option<PathBuf> {
        config_base().map(|b| b.join("rython-editor").join("recent.json"))
    }

    pub fn load() -> Self {
        let path = match Self::config_path() {
            Some(p) => p,
            None => return Self::default(),
        };
        let mut this: Self = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        this.prune();
        this
    }

    pub fn save(&self) {
        let Some(path) = Self::config_path() else { return };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&path, json);
        }
    }

    /// Add a project path (most recent first). Prunes stale and duplicate entries.
    pub fn push(&mut self, path: PathBuf) {
        self.recent_projects.retain(|p| p != &path);
        self.recent_projects.insert(0, path);
        self.prune();
        self.recent_projects.truncate(Self::MAX_ENTRIES);
    }

    /// Remove entries whose `project.json` no longer exists on disk.
    pub fn prune(&mut self) {
        self.recent_projects.retain(|p| p.join("project.json").exists());
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Returns the OS config base directory.
/// Respects `XDG_CONFIG_HOME` on Linux; falls back to `$HOME/.config`.
fn config_base() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg));
        }
    }
    std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".config"))
}

use std::fs;
use std::path::Path;

use rython_ecs::{EntityId, Scene};

use super::format::ProjectConfig;

/// Create directory structure + default `project.json`.
pub fn create_project(root: &Path, name: &str) -> std::io::Result<ProjectConfig> {
    fs::create_dir_all(root)?;
    fs::create_dir_all(root.join("scenes"))?;
    fs::create_dir_all(root.join("ui"))?;
    fs::create_dir_all(root.join("scripts"))?;
    fs::create_dir_all(root.join("assets/meshes"))?;
    fs::create_dir_all(root.join("assets/textures"))?;
    fs::create_dir_all(root.join("assets/sounds"))?;

    let config = ProjectConfig {
        name: name.to_string(),
        version: "0.1.0".to_string(),
        default_scene: None,
        entry_point: None,
        engine_config: Default::default(),
    };
    save_project(root, &config)?;
    Ok(config)
}

/// Read and parse `project.json`.
pub fn open_project(root: &Path) -> std::io::Result<ProjectConfig> {
    let content = fs::read_to_string(root.join("project.json"))?;
    serde_json::from_str(&content)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// Write `project.json`.
pub fn save_project(root: &Path, config: &ProjectConfig) -> std::io::Result<()> {
    let content = serde_json::to_string_pretty(config)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    fs::write(root.join("project.json"), content)
}

/// Scan `scenes/` for `.json` files, returning names without extension.
pub fn list_scenes(root: &Path) -> Vec<String> {
    let scenes_dir = root.join("scenes");
    let Ok(entries) = fs::read_dir(&scenes_dir) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| {
            let entry = e.ok()?;
            let path = entry.path();
            if path.extension()?.to_str()? == "json" {
                Some(path.file_stem()?.to_string_lossy().into_owned())
            } else {
                None
            }
        })
        .collect()
}

/// Serialize scene and write to `scenes/<name>.json`.
pub fn save_scene(root: &Path, name: &str, scene: &Scene) -> std::io::Result<()> {
    fs::create_dir_all(root.join("scenes"))?;
    let data = scene.save_json();
    let content = serde_json::to_string_pretty(&data)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    fs::write(root.join("scenes").join(format!("{name}.json")), content)
}

/// Read `scenes/<name>.json`, load into scene, then advance the entity counter
/// past all loaded IDs to prevent collisions.
pub fn load_scene(root: &Path, name: &str, scene: &Scene) -> std::io::Result<()> {
    let content = fs::read_to_string(root.join("scenes").join(format!("{name}.json")))?;
    let data: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    scene.load_json(&data);

    // Advance counter past maximum loaded ID.
    let max_id = data["entities"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|e| e["id"].as_u64())
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0);
    EntityId::ensure_counter_past(max_id);
    Ok(())
}

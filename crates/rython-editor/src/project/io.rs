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
        ..Default::default()
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static IO_TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let n = IO_TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
            let path =
                std::env::temp_dir().join(format!("rython_io_{}_{}", label, n));
            std::fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    // ── create_project ────────────────────────────────────────────────────────

    #[test]
    fn create_project_makes_expected_directories() {
        let tmp = TempDir::new("create_dirs");
        create_project(&tmp.0, "TestGame").unwrap();
        assert!(tmp.0.join("scenes").is_dir());
        assert!(tmp.0.join("ui").is_dir());
        assert!(tmp.0.join("scripts").is_dir());
        assert!(tmp.0.join("assets/meshes").is_dir());
        assert!(tmp.0.join("assets/textures").is_dir());
        assert!(tmp.0.join("assets/sounds").is_dir());
    }

    #[test]
    fn create_project_writes_project_json() {
        let tmp = TempDir::new("create_json");
        let config = create_project(&tmp.0, "TestGame").unwrap();
        assert_eq!(config.name, "TestGame");
        assert_eq!(config.version, "0.1.0");
        assert!(config.default_scene.is_none());
        assert!(config.entry_point.is_none());
        assert!(tmp.0.join("project.json").exists());
    }

    #[test]
    fn create_project_name_preserved_in_json() {
        let tmp = TempDir::new("create_name");
        create_project(&tmp.0, "SpecialName").unwrap();
        let loaded = open_project(&tmp.0).unwrap();
        assert_eq!(loaded.name, "SpecialName");
    }

    // ── open_project / save_project ───────────────────────────────────────────

    #[test]
    fn open_project_round_trips_config() {
        let tmp = TempDir::new("open_round_trip");
        create_project(&tmp.0, "RoundTrip").unwrap();
        let loaded = open_project(&tmp.0).unwrap();
        assert_eq!(loaded.name, "RoundTrip");
        assert_eq!(loaded.version, "0.1.0");
    }

    #[test]
    fn open_project_missing_file_returns_error() {
        let tmp = TempDir::new("open_missing");
        let result = open_project(&tmp.0);
        assert!(result.is_err());
    }

    #[test]
    fn save_project_updates_persisted_data() {
        let tmp = TempDir::new("save_update");
        let mut config = create_project(&tmp.0, "Original").unwrap();
        config.name = "Updated".to_string();
        config.version = "1.0.0".to_string();
        config.default_scene = Some("main_level".to_string());
        save_project(&tmp.0, &config).unwrap();
        let reloaded = open_project(&tmp.0).unwrap();
        assert_eq!(reloaded.name, "Updated");
        assert_eq!(reloaded.version, "1.0.0");
        assert_eq!(reloaded.default_scene.as_deref(), Some("main_level"));
    }

    #[test]
    fn open_project_invalid_json_returns_error() {
        let tmp = TempDir::new("open_invalid");
        std::fs::write(tmp.0.join("project.json"), "not json {{").unwrap();
        let result = open_project(&tmp.0);
        assert!(result.is_err());
    }

    // ── list_scenes ───────────────────────────────────────────────────────────

    #[test]
    fn list_scenes_returns_empty_for_missing_scenes_dir() {
        let tmp = TempDir::new("list_empty");
        let scenes = list_scenes(&tmp.0);
        assert!(scenes.is_empty());
    }

    #[test]
    fn list_scenes_returns_json_file_stems() {
        let tmp = TempDir::new("list_stems");
        let scenes_dir = tmp.0.join("scenes");
        std::fs::create_dir_all(&scenes_dir).unwrap();
        std::fs::write(scenes_dir.join("level1.json"), "{}").unwrap();
        std::fs::write(scenes_dir.join("level2.json"), "{}").unwrap();
        let mut scenes = list_scenes(&tmp.0);
        scenes.sort();
        assert_eq!(scenes, vec!["level1", "level2"]);
    }

    #[test]
    fn list_scenes_ignores_non_json_files() {
        let tmp = TempDir::new("list_filter");
        let scenes_dir = tmp.0.join("scenes");
        std::fs::create_dir_all(&scenes_dir).unwrap();
        std::fs::write(scenes_dir.join("level.json"), "{}").unwrap();
        std::fs::write(scenes_dir.join("notes.txt"), "hi").unwrap();
        std::fs::write(scenes_dir.join("thumbnail.png"), "").unwrap();
        let scenes = list_scenes(&tmp.0);
        assert_eq!(scenes.len(), 1);
        assert_eq!(scenes[0], "level");
    }

    #[test]
    fn list_scenes_returns_empty_for_empty_scenes_dir() {
        let tmp = TempDir::new("list_dir_empty");
        std::fs::create_dir_all(tmp.0.join("scenes")).unwrap();
        let scenes = list_scenes(&tmp.0);
        assert!(scenes.is_empty());
    }

    // ── save_scene / load_scene ────────────────────────────────────────────────

    #[test]
    fn save_scene_creates_json_file() {
        let tmp = TempDir::new("save_scene");
        let scene = rython_ecs::Scene::new();
        save_scene(&tmp.0, "my_level", &scene).unwrap();
        assert!(tmp.0.join("scenes").join("my_level.json").exists());
    }

    #[test]
    fn save_and_load_scene_round_trip_preserves_entities() {
        let tmp = TempDir::new("scene_round_trip");
        let scene = rython_ecs::Scene::new();
        scene.spawn_immediate(vec![]);
        scene.spawn_immediate(vec![]);
        let original_count = scene.all_entities().len();
        assert_eq!(original_count, 2);

        save_scene(&tmp.0, "test_scene", &scene).unwrap();

        let scene2 = rython_ecs::Scene::new();
        load_scene(&tmp.0, "test_scene", &scene2).unwrap();
        assert_eq!(scene2.all_entities().len(), original_count);
    }

    #[test]
    fn load_scene_missing_file_returns_error() {
        let tmp = TempDir::new("load_missing");
        std::fs::create_dir_all(tmp.0.join("scenes")).unwrap();
        let scene = rython_ecs::Scene::new();
        let result = load_scene(&tmp.0, "nonexistent", &scene);
        assert!(result.is_err());
    }

    #[test]
    fn load_scene_invalid_json_returns_error() {
        let tmp = TempDir::new("load_invalid");
        std::fs::create_dir_all(tmp.0.join("scenes")).unwrap();
        std::fs::write(tmp.0.join("scenes").join("bad.json"), "!!!").unwrap();
        let scene = rython_ecs::Scene::new();
        let result = load_scene(&tmp.0, "bad", &scene);
        assert!(result.is_err());
    }
}

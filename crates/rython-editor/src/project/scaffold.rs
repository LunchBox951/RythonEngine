use std::fs;
use std::path::Path;

/// Template kinds that the editor can scaffold for the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaffoldTemplate {
    /// Bare `main.py`-style entry point with `init()`.
    BasicModule,
    /// Class-based entity script with lifecycle hooks.
    ScriptClass,
    /// Minimal module that registers a `on_tick` recurring callback.
    PerFrameUpdate,
}

impl ScaffoldTemplate {
    pub fn label(self) -> &'static str {
        match self {
            ScaffoldTemplate::BasicModule => "Basic Module",
            ScaffoldTemplate::ScriptClass => "Script Class",
            ScaffoldTemplate::PerFrameUpdate => "Per-Frame Update",
        }
    }
}

/// Generate Python source text for `template`.
///
/// * `name` — module name (for BasicModule / PerFrameUpdate) or the raw
///   identifier that will be converted to PascalCase for ScriptClass.
/// * `project_name` — used in the BasicModule docstring.
pub fn generate_script(template: ScaffoldTemplate, name: &str, project_name: &str) -> String {
    match template {
        ScaffoldTemplate::BasicModule => format!(
            r#""""{project_name} — entry point.

Called by the engine on startup.
"""
import rython


def init():
    """Called once when the scripting module is loaded."""
    rython.camera.set_position(0.0, 5.0, -10.0)
    rython.camera.set_look_at(0.0, 0.0, 0.0)

    # Load scene entities here, or let the engine load from scene JSON.
    pass
"#,
            project_name = project_name
        ),

        ScaffoldTemplate::ScriptClass => {
            let class_name = to_pascal_case(name);
            format!(
                r#""""{class_name} — entity script.

Attach to an entity with:
    rython.scene.attach_script(entity, {class_name})
"""
import rython


class {class_name}:
    def __init__(self, entity):
        self.entity = entity

    def on_spawn(self):
        pass

    def on_despawn(self):
        pass

    def on_collision(self, other_entity, normal_vec):
        pass

    def on_trigger_enter(self, other_entity):
        pass

    def on_trigger_exit(self, other_entity):
        pass

    def on_input_action(self, action_name, value):
        pass
"#,
                class_name = class_name
            )
        }

        ScaffoldTemplate::PerFrameUpdate => format!(
            r#""""{module_name} — per-frame update logic."""
import rython


def init():
    rython.scheduler.register_recurring(on_tick)


def on_tick():
    t = rython.time.elapsed
    # Update logic here
    pass
"#,
            module_name = name
        ),
    }
}

/// Write `content` to `<root>/scripts/<filename>`.
///
/// Creates the `scripts/` directory if it does not exist.
pub fn write_script(root: &Path, filename: &str, content: &str) -> std::io::Result<()> {
    let scripts_dir = root.join("scripts");
    fs::create_dir_all(&scripts_dir)?;
    fs::write(scripts_dir.join(filename), content)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Convert `snake_case`, `kebab-case`, or space-separated words to PascalCase.
fn to_pascal_case(s: &str) -> String {
    s.split(|c: char| c == '_' || c == '-' || c == ' ')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SCAFFOLD_TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let n = SCAFFOLD_TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
            let path =
                std::env::temp_dir().join(format!("rython_scaffold_{}_{}", label, n));
            std::fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    // ── to_pascal_case ────────────────────────────────────────────────────────

    #[test]
    fn pascal_case_snake() {
        assert_eq!(to_pascal_case("my_entity"), "MyEntity");
    }

    #[test]
    fn pascal_case_kebab() {
        assert_eq!(to_pascal_case("player-controller"), "PlayerController");
    }

    #[test]
    fn pascal_case_already_pascal() {
        assert_eq!(to_pascal_case("Enemy"), "Enemy");
    }

    #[test]
    fn pascal_case_spaces() {
        assert_eq!(to_pascal_case("my entity"), "MyEntity");
    }

    #[test]
    fn pascal_case_mixed_separators() {
        assert_eq!(to_pascal_case("my_entity-name foo"), "MyEntityNameFoo");
    }

    #[test]
    fn pascal_case_empty_string() {
        assert_eq!(to_pascal_case(""), "");
    }

    #[test]
    fn pascal_case_single_word() {
        assert_eq!(to_pascal_case("player"), "Player");
    }

    #[test]
    fn pascal_case_with_numbers() {
        // Numbers are not word boundaries; digit stays attached.
        assert_eq!(to_pascal_case("player2d"), "Player2d");
    }

    #[test]
    fn pascal_case_consecutive_separators_skipped() {
        // Empty parts between consecutive separators are filtered out.
        assert_eq!(to_pascal_case("a__b--c"), "ABC");
    }

    // ── generate_script ───────────────────────────────────────────────────────

    #[test]
    fn basic_module_contains_init() {
        let src = generate_script(ScaffoldTemplate::BasicModule, "main", "MyGame");
        assert!(src.contains("def init()"));
        assert!(src.contains("MyGame"));
    }

    #[test]
    fn basic_module_imports_rython() {
        let src = generate_script(ScaffoldTemplate::BasicModule, "main", "MyGame");
        assert!(src.contains("import rython"));
    }

    #[test]
    fn basic_module_sets_up_camera() {
        let src = generate_script(ScaffoldTemplate::BasicModule, "main", "MyGame");
        assert!(src.contains("set_position"));
        assert!(src.contains("set_look_at"));
    }

    #[test]
    fn script_class_uses_pascal_name() {
        let src = generate_script(ScaffoldTemplate::ScriptClass, "player_controller", "MyGame");
        assert!(src.contains("class PlayerController"));
        assert!(src.contains("def on_spawn"));
    }

    #[test]
    fn script_class_all_lifecycle_hooks_present() {
        let src = generate_script(ScaffoldTemplate::ScriptClass, "enemy", "MyGame");
        assert!(src.contains("def on_spawn"));
        assert!(src.contains("def on_despawn"));
        assert!(src.contains("def on_collision"));
        assert!(src.contains("def on_trigger_enter"));
        assert!(src.contains("def on_trigger_exit"));
        assert!(src.contains("def on_input_action"));
    }

    #[test]
    fn script_class_imports_rython() {
        let src = generate_script(ScaffoldTemplate::ScriptClass, "enemy", "MyGame");
        assert!(src.contains("import rython"));
    }

    #[test]
    fn script_class_constructor_takes_entity() {
        let src = generate_script(ScaffoldTemplate::ScriptClass, "boss", "MyGame");
        assert!(src.contains("def __init__(self, entity)"));
    }

    #[test]
    fn per_frame_registers_recurring() {
        let src = generate_script(ScaffoldTemplate::PerFrameUpdate, "game_tick", "MyGame");
        assert!(src.contains("register_recurring(on_tick)"));
        assert!(src.contains("def on_tick"));
    }

    #[test]
    fn per_frame_imports_rython() {
        let src = generate_script(ScaffoldTemplate::PerFrameUpdate, "ticker", "MyGame");
        assert!(src.contains("import rython"));
    }

    #[test]
    fn per_frame_exposes_elapsed_time() {
        let src = generate_script(ScaffoldTemplate::PerFrameUpdate, "ticker", "MyGame");
        assert!(src.contains("rython.time.elapsed"));
    }

    #[test]
    fn per_frame_uses_provided_module_name_in_docstring() {
        let src = generate_script(ScaffoldTemplate::PerFrameUpdate, "my_ticker", "TestGame");
        assert!(src.contains("my_ticker"));
    }

    // ── write_script ──────────────────────────────────────────────────────────

    #[test]
    fn write_script_creates_scripts_directory_if_missing() {
        let tmp = TempDir::new("write_dir");
        assert!(!tmp.0.join("scripts").exists());
        write_script(&tmp.0, "mod.py", "pass").unwrap();
        assert!(tmp.0.join("scripts").is_dir());
    }

    #[test]
    fn write_script_creates_file_with_correct_content() {
        let tmp = TempDir::new("write_content");
        write_script(&tmp.0, "hello.py", "# hello\n").unwrap();
        let path = tmp.0.join("scripts").join("hello.py");
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "# hello\n");
    }

    #[test]
    fn write_script_overwrites_existing_file() {
        let tmp = TempDir::new("write_overwrite");
        write_script(&tmp.0, "mod.py", "# v1").unwrap();
        write_script(&tmp.0, "mod.py", "# v2").unwrap();
        let content =
            std::fs::read_to_string(tmp.0.join("scripts").join("mod.py")).unwrap();
        assert_eq!(content, "# v2");
    }
}

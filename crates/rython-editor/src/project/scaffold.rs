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
    fn basic_module_contains_init() {
        let src = generate_script(ScaffoldTemplate::BasicModule, "main", "MyGame");
        assert!(src.contains("def init()"));
        assert!(src.contains("MyGame"));
    }

    #[test]
    fn script_class_uses_pascal_name() {
        let src = generate_script(ScaffoldTemplate::ScriptClass, "player_controller", "MyGame");
        assert!(src.contains("class PlayerController"));
        assert!(src.contains("def on_spawn"));
    }

    #[test]
    fn per_frame_registers_recurring() {
        let src = generate_script(ScaffoldTemplate::PerFrameUpdate, "game_tick", "MyGame");
        assert!(src.contains("register_recurring(on_tick)"));
        assert!(src.contains("def on_tick"));
    }
}

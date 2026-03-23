use serde::{Deserialize, Serialize};

use rython_ecs::component::Component;

/// ECS component that attaches a Python script class to an entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptComponent {
    /// Name of the Python class to instantiate (must be registered via `register_script_class`).
    pub class_name: String,
}

impl Component for ScriptComponent {
    fn component_type_name(&self) -> &'static str {
        "ScriptComponent"
    }

    fn clone_box(&self) -> Box<dyn Component> {
        Box::new(self.clone())
    }

    fn serialize_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }
}

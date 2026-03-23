use rython_window::{GamepadAxisType, GamepadButton, KeyCode, MouseAxisType, MouseButton};
use std::collections::HashMap;

/// How a logical axis is mapped to hardware.
#[derive(Debug, Clone)]
pub enum AxisBinding {
    /// Two keyboard keys: negative and positive direction.
    KBAxis { negative: KeyCode, positive: KeyCode },
    /// Mouse movement on an axis.
    MouseAxis { axis: MouseAxisType },
    /// Gamepad analog stick or trigger.
    GamepadAxis { axis: GamepadAxisType },
}

/// How a logical button is mapped to hardware.
#[derive(Debug, Clone)]
pub enum ButtonBinding {
    Keyboard(KeyCode),
    Mouse(MouseButton),
    Gamepad(GamepadButton),
}

/// Maps logical action names to hardware bindings.
/// Multiple bindings per action are supported; highest absolute value wins for axes.
#[derive(Debug, Clone)]
pub struct InputMap {
    name: String,
    axis_bindings: HashMap<String, Vec<AxisBinding>>,
    button_bindings: HashMap<String, Vec<ButtonBinding>>,
}

impl InputMap {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            axis_bindings: HashMap::new(),
            button_bindings: HashMap::new(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn bind_axis(&mut self, action: impl Into<String>, binding: AxisBinding) {
        self.axis_bindings.entry(action.into()).or_default().push(binding);
    }

    pub fn bind_button(&mut self, action: impl Into<String>, binding: ButtonBinding) {
        self.button_bindings.entry(action.into()).or_default().push(binding);
    }

    pub fn axis_bindings(&self, action: &str) -> &[AxisBinding] {
        self.axis_bindings.get(action).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn button_bindings(&self, action: &str) -> &[ButtonBinding] {
        self.button_bindings.get(action).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn all_axis_actions(&self) -> impl Iterator<Item = &String> {
        self.axis_bindings.keys()
    }

    pub fn all_button_actions(&self) -> impl Iterator<Item = &String> {
        self.button_bindings.keys()
    }
}

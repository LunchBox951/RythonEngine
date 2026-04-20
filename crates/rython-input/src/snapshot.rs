use crate::value::ActionValue;
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
struct ButtonState {
    pressed: bool,
    held: bool,
    released: bool,
}

/// Per-frame snapshot of all logical input action states.
///
/// Derived from `PlayerController::tick`; the active snapshot is published
/// to the Python polling bridge (`rython.input.axis/pressed/held/released`).
#[derive(Debug, Clone, Default)]
pub struct InputSnapshot {
    axes: HashMap<String, f32>,
    axes2: HashMap<String, [f32; 2]>,
    axes3: HashMap<String, [f32; 3]>,
    buttons: HashMap<String, ButtonState>,
    values: HashMap<String, ActionValue>,
}

impl InputSnapshot {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_axis(&mut self, action: String, value: f32) {
        self.axes.insert(action, value);
    }

    pub fn set_axis2(&mut self, action: String, value: [f32; 2]) {
        self.axes2.insert(action, value);
    }

    pub fn set_axis3(&mut self, action: String, value: [f32; 3]) {
        self.axes3.insert(action, value);
    }

    pub fn set_button(&mut self, action: String, pressed: bool, held: bool, released: bool) {
        self.buttons.insert(
            action,
            ButtonState {
                pressed,
                held,
                released,
            },
        );
    }

    pub fn set_value(&mut self, action: String, value: ActionValue) {
        self.values.insert(action, value);
    }

    /// 1D value (or magnitude for 2D/3D actions). Returns 0.0 if unbound.
    pub fn axis(&self, action: &str) -> f32 {
        if let Some(v) = self.axes.get(action) {
            return *v;
        }
        if let Some(v) = self.axes2.get(action) {
            return (v[0] * v[0] + v[1] * v[1]).sqrt();
        }
        if let Some(v) = self.axes3.get(action) {
            return (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        }
        0.0
    }

    /// 2D value for the action. Returns [0, 0] if unbound or non-2D.
    pub fn axis2(&self, action: &str) -> [f32; 2] {
        self.axes2.get(action).copied().unwrap_or([0.0, 0.0])
    }

    /// 3D value for the action. Returns [0, 0, 0] if unbound or non-3D.
    pub fn axis3(&self, action: &str) -> [f32; 3] {
        self.axes3.get(action).copied().unwrap_or([0.0, 0.0, 0.0])
    }

    /// Raw typed value for the action. Returns `None` if unbound.
    pub fn value(&self, action: &str) -> Option<ActionValue> {
        self.values.get(action).copied()
    }

    /// True on the first frame the button is pressed.
    pub fn pressed(&self, action: &str) -> bool {
        self.buttons.get(action).map(|b| b.pressed).unwrap_or(false)
    }

    /// True every frame the button is held (including the first press frame).
    pub fn held(&self, action: &str) -> bool {
        self.buttons.get(action).map(|b| b.held).unwrap_or(false)
    }

    /// True on the first frame the button is released.
    pub fn released(&self, action: &str) -> bool {
        self.buttons
            .get(action)
            .map(|b| b.released)
            .unwrap_or(false)
    }
}

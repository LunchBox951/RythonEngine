use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
struct ButtonState {
    pressed: bool,
    held: bool,
    released: bool,
}

/// Per-frame snapshot of all logical input action states.
#[derive(Debug, Clone, Default)]
pub struct InputSnapshot {
    axes: HashMap<String, f32>,
    buttons: HashMap<String, ButtonState>,
}

impl InputSnapshot {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_axis(&mut self, action: String, value: f32) {
        self.axes.insert(action, value);
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

    /// Axis value for the action (-1.0 to 1.0). Returns 0.0 if unbound.
    pub fn axis(&self, action: &str) -> f32 {
        self.axes.get(action).copied().unwrap_or(0.0)
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

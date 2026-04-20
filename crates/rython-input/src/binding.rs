//! Hardware bindings: one raw input source plus its modifier + trigger pipeline.
//!
//! Designers compose bindings per action. A single action may hold several
//! bindings (keyboard + gamepad + alternate keybind) — at runtime their
//! post-modifier samples are accumulated component-wise (`ActionValue::accumulate`).

use crate::bitset::{GamepadButtonSet, KeyCodeSet, MouseButtonSet};
use crate::modifier::{Modifier, apply_pipeline};
use crate::trigger::Trigger;
use rython_window::{GamepadAxisType, GamepadButton, KeyCode, MouseAxisType, MouseButton};
use std::collections::HashMap;

/// The raw hardware source a binding reads from.
#[derive(Debug, Clone)]
pub enum HardwareKey {
    Key(KeyCode),
    Mouse(MouseButton),
    MouseAxis(MouseAxisType),
    Gamepad(GamepadButton),
    GamepadAxis(GamepadAxisType),

    /// Four keys that together form a 2D vector (e.g. WASD).
    Composite2D {
        up: KeyCode,
        down: KeyCode,
        left: KeyCode,
        right: KeyCode,
    },

    /// Six keys that together form a 3D vector (e.g. WASD + QE for up/down).
    Composite3D {
        up: KeyCode,
        down: KeyCode,
        left: KeyCode,
        right: KeyCode,
        forward: KeyCode,
        back: KeyCode,
    },

    /// A two-axis gamepad stick — produces a `[x, y, 0]` sample.
    GamepadStick {
        x_axis: GamepadAxisType,
        y_axis: GamepadAxisType,
    },
}

/// Bundle of all raw hardware state a binding might read. Passed to
/// `InputBinding::sample` by reference so callers don't need to plumb each
/// field individually.
pub struct InputSource<'a> {
    pub keys: &'a KeyCodeSet,
    pub mouse_buttons: &'a MouseButtonSet,
    pub mouse_delta: (f32, f32),
    pub gamepad_buttons: &'a GamepadButtonSet,
    pub gamepad_axes: &'a HashMap<GamepadAxisType, f32>,
}

/// A single binding: a hardware source plus modifier + trigger pipelines.
#[derive(Debug, Clone)]
pub struct InputBinding {
    pub key: HardwareKey,
    pub modifiers: Vec<Modifier>,
    pub triggers: Vec<Trigger>,
}

impl InputBinding {
    pub fn new(key: HardwareKey) -> Self {
        Self {
            key,
            modifiers: Vec::new(),
            triggers: Vec::new(),
        }
    }

    pub fn with_modifier(mut self, m: Modifier) -> Self {
        self.modifiers.push(m);
        self
    }

    pub fn with_trigger(mut self, t: Trigger) -> Self {
        self.triggers.push(t);
        self
    }

    /// Sample the raw hardware value, then run the modifier pipeline.
    pub fn sample(&self, src: &InputSource<'_>) -> [f32; 3] {
        let raw = sample_raw(&self.key, src);
        apply_pipeline(&self.modifiers, raw)
    }
}

fn key_axis(keys: &KeyCodeSet, neg: &KeyCode, pos: &KeyCode) -> f32 {
    let n: f32 = if keys.contains(neg) { -1.0 } else { 0.0 };
    let p: f32 = if keys.contains(pos) { 1.0 } else { 0.0 };
    (n + p).clamp(-1.0, 1.0)
}

fn sample_raw(key: &HardwareKey, src: &InputSource<'_>) -> [f32; 3] {
    match key {
        HardwareKey::Key(k) => {
            if src.keys.contains(k) {
                [1.0, 0.0, 0.0]
            } else {
                [0.0, 0.0, 0.0]
            }
        }
        HardwareKey::Mouse(m) => {
            if src.mouse_buttons.contains(m) {
                [1.0, 0.0, 0.0]
            } else {
                [0.0, 0.0, 0.0]
            }
        }
        HardwareKey::MouseAxis(axis) => match axis {
            MouseAxisType::X => [src.mouse_delta.0, 0.0, 0.0],
            MouseAxisType::Y => [0.0, src.mouse_delta.1, 0.0],
        },
        HardwareKey::Gamepad(b) => {
            if src.gamepad_buttons.contains(b) {
                [1.0, 0.0, 0.0]
            } else {
                [0.0, 0.0, 0.0]
            }
        }
        HardwareKey::GamepadAxis(axis) => {
            let v = src.gamepad_axes.get(axis).copied().unwrap_or(0.0);
            [v, 0.0, 0.0]
        }
        HardwareKey::Composite2D {
            up,
            down,
            left,
            right,
        } => [
            key_axis(src.keys, left, right),
            key_axis(src.keys, down, up),
            0.0,
        ],
        HardwareKey::Composite3D {
            up,
            down,
            left,
            right,
            forward,
            back,
        } => [
            key_axis(src.keys, left, right),
            key_axis(src.keys, down, up),
            key_axis(src.keys, back, forward),
        ],
        HardwareKey::GamepadStick { x_axis, y_axis } => [
            src.gamepad_axes.get(x_axis).copied().unwrap_or(0.0),
            src.gamepad_axes.get(y_axis).copied().unwrap_or(0.0),
            0.0,
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_src<'a>(
        keys: &'a KeyCodeSet,
        mouse: &'a MouseButtonSet,
        gpad: &'a GamepadButtonSet,
        gaxes: &'a HashMap<GamepadAxisType, f32>,
    ) -> InputSource<'a> {
        InputSource {
            keys,
            mouse_buttons: mouse,
            mouse_delta: (0.0, 0.0),
            gamepad_buttons: gpad,
            gamepad_axes: gaxes,
        }
    }

    #[test]
    fn key_binding_samples_one_or_zero() {
        let mut keys = KeyCodeSet::new();
        let mouse = MouseButtonSet::new();
        let gpad = GamepadButtonSet::new();
        let gaxes = HashMap::new();
        let b = InputBinding::new(HardwareKey::Key(KeyCode::Space));

        assert_eq!(b.sample(&empty_src(&keys, &mouse, &gpad, &gaxes)), [0.0, 0.0, 0.0]);
        keys.insert(KeyCode::Space);
        assert_eq!(b.sample(&empty_src(&keys, &mouse, &gpad, &gaxes)), [1.0, 0.0, 0.0]);
    }

    #[test]
    fn composite_2d_wasd() {
        let mut keys = KeyCodeSet::new();
        let mouse = MouseButtonSet::new();
        let gpad = GamepadButtonSet::new();
        let gaxes = HashMap::new();
        let b = InputBinding::new(HardwareKey::Composite2D {
            up: KeyCode::W,
            down: KeyCode::S,
            left: KeyCode::A,
            right: KeyCode::D,
        });

        keys.insert(KeyCode::W);
        keys.insert(KeyCode::D);
        assert_eq!(b.sample(&empty_src(&keys, &mouse, &gpad, &gaxes)), [1.0, 1.0, 0.0]);

        keys.insert(KeyCode::A); // left + right cancel
        assert_eq!(b.sample(&empty_src(&keys, &mouse, &gpad, &gaxes)), [0.0, 1.0, 0.0]);
    }

    #[test]
    fn composite_3d_six_keys() {
        let mut keys = KeyCodeSet::new();
        let mouse = MouseButtonSet::new();
        let gpad = GamepadButtonSet::new();
        let gaxes = HashMap::new();
        let b = InputBinding::new(HardwareKey::Composite3D {
            up: KeyCode::E,
            down: KeyCode::Q,
            left: KeyCode::A,
            right: KeyCode::D,
            forward: KeyCode::W,
            back: KeyCode::S,
        });

        keys.insert(KeyCode::W);
        keys.insert(KeyCode::E);
        assert_eq!(b.sample(&empty_src(&keys, &mouse, &gpad, &gaxes)), [0.0, 1.0, 1.0]);
    }

    #[test]
    fn gamepad_stick_samples_two_axes() {
        let keys = KeyCodeSet::new();
        let mouse = MouseButtonSet::new();
        let gpad = GamepadButtonSet::new();
        let mut gaxes = HashMap::new();
        gaxes.insert(GamepadAxisType::LeftStickX, 0.6);
        gaxes.insert(GamepadAxisType::LeftStickY, -0.4);
        let b = InputBinding::new(HardwareKey::GamepadStick {
            x_axis: GamepadAxisType::LeftStickX,
            y_axis: GamepadAxisType::LeftStickY,
        });

        let s = b.sample(&empty_src(&keys, &mouse, &gpad, &gaxes));
        assert!((s[0] - 0.6).abs() < 1e-5);
        assert!((s[1] - -0.4).abs() < 1e-5);
    }

    #[test]
    fn modifier_pipeline_runs_on_sample() {
        let mut keys = KeyCodeSet::new();
        let mouse = MouseButtonSet::new();
        let gpad = GamepadButtonSet::new();
        let gaxes = HashMap::new();
        let b = InputBinding::new(HardwareKey::Key(KeyCode::W))
            .with_modifier(Modifier::Scale([2.0, 1.0, 1.0]));

        keys.insert(KeyCode::W);
        let s = b.sample(&empty_src(&keys, &mouse, &gpad, &gaxes));
        assert_eq!(s, [2.0, 0.0, 0.0]);
    }
}

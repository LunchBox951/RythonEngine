//! Compact bitset wrappers for input button sets.
//!
//! Each hardware button type is mapped to a fixed bit position so that
//! "cloning" the previous-frame state is a single integer copy instead of
//! a `HashSet::clone`.

use rython_window::{GamepadButton, KeyCode, MouseButton};

// ─── KeyCode bitset (63 variants → u64) ─────────────────────────────────────

/// Maps a `KeyCode` variant to a bit index (0..62).
const fn key_index(k: KeyCode) -> u32 {
    match k {
        KeyCode::A => 0,
        KeyCode::B => 1,
        KeyCode::C => 2,
        KeyCode::D => 3,
        KeyCode::E => 4,
        KeyCode::F => 5,
        KeyCode::G => 6,
        KeyCode::H => 7,
        KeyCode::I => 8,
        KeyCode::J => 9,
        KeyCode::K => 10,
        KeyCode::L => 11,
        KeyCode::M => 12,
        KeyCode::N => 13,
        KeyCode::O => 14,
        KeyCode::P => 15,
        KeyCode::Q => 16,
        KeyCode::R => 17,
        KeyCode::S => 18,
        KeyCode::T => 19,
        KeyCode::U => 20,
        KeyCode::V => 21,
        KeyCode::W => 22,
        KeyCode::X => 23,
        KeyCode::Y => 24,
        KeyCode::Z => 25,
        KeyCode::Key0 => 26,
        KeyCode::Key1 => 27,
        KeyCode::Key2 => 28,
        KeyCode::Key3 => 29,
        KeyCode::Key4 => 30,
        KeyCode::Key5 => 31,
        KeyCode::Key6 => 32,
        KeyCode::Key7 => 33,
        KeyCode::Key8 => 34,
        KeyCode::Key9 => 35,
        KeyCode::Space => 36,
        KeyCode::Enter => 37,
        KeyCode::Escape => 38,
        KeyCode::Tab => 39,
        KeyCode::Backspace => 40,
        KeyCode::LeftShift => 41,
        KeyCode::RightShift => 42,
        KeyCode::LeftControl => 43,
        KeyCode::RightControl => 44,
        KeyCode::LeftAlt => 45,
        KeyCode::RightAlt => 46,
        KeyCode::Up => 47,
        KeyCode::Down => 48,
        KeyCode::Left => 49,
        KeyCode::Right => 50,
        KeyCode::F1 => 51,
        KeyCode::F2 => 52,
        KeyCode::F3 => 53,
        KeyCode::F4 => 54,
        KeyCode::F5 => 55,
        KeyCode::F6 => 56,
        KeyCode::F7 => 57,
        KeyCode::F8 => 58,
        KeyCode::F9 => 59,
        KeyCode::F10 => 60,
        KeyCode::F11 => 61,
        KeyCode::F12 => 62,
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct KeyCodeSet(u64);

impl KeyCodeSet {
    pub fn new() -> Self {
        Self(0)
    }

    pub fn insert(&mut self, key: KeyCode) {
        self.0 |= 1u64 << key_index(key);
    }

    pub fn remove(&mut self, key: KeyCode) {
        self.0 &= !(1u64 << key_index(key));
    }

    pub fn contains(&self, key: &KeyCode) -> bool {
        (self.0 & (1u64 << key_index(*key))) != 0
    }
}

// ─── MouseButton bitset (3 variants → u8) ───────────────────────────────────

const fn mouse_index(b: MouseButton) -> u32 {
    match b {
        MouseButton::Left => 0,
        MouseButton::Right => 1,
        MouseButton::Middle => 2,
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MouseButtonSet(u8);

impl MouseButtonSet {
    pub fn new() -> Self {
        Self(0)
    }

    pub fn insert(&mut self, btn: MouseButton) {
        self.0 |= 1u8 << mouse_index(btn);
    }

    pub fn remove(&mut self, btn: MouseButton) {
        self.0 &= !(1u8 << mouse_index(btn));
    }

    pub fn contains(&self, btn: &MouseButton) -> bool {
        (self.0 & (1u8 << mouse_index(*btn))) != 0
    }

    pub fn clear(&mut self) {
        self.0 = 0;
    }
}

// ─── GamepadButton bitset (16 variants → u16) ───────────────────────────────

const fn gamepad_index(b: GamepadButton) -> u32 {
    match b {
        GamepadButton::South => 0,
        GamepadButton::East => 1,
        GamepadButton::West => 2,
        GamepadButton::North => 3,
        GamepadButton::LeftBumper => 4,
        GamepadButton::RightBumper => 5,
        GamepadButton::LeftTriggerButton => 6,
        GamepadButton::RightTriggerButton => 7,
        GamepadButton::LeftStickPress => 8,
        GamepadButton::RightStickPress => 9,
        GamepadButton::DPadUp => 10,
        GamepadButton::DPadDown => 11,
        GamepadButton::DPadLeft => 12,
        GamepadButton::DPadRight => 13,
        GamepadButton::Start => 14,
        GamepadButton::Select => 15,
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct GamepadButtonSet(u16);

impl GamepadButtonSet {
    pub fn new() -> Self {
        Self(0)
    }

    pub fn insert(&mut self, btn: GamepadButton) {
        self.0 |= 1u16 << gamepad_index(btn);
    }

    pub fn remove(&mut self, btn: GamepadButton) {
        self.0 &= !(1u16 << gamepad_index(btn));
    }

    pub fn contains(&self, btn: &GamepadButton) -> bool {
        (self.0 & (1u16 << gamepad_index(*btn))) != 0
    }

    pub fn clear(&mut self) {
        self.0 = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_set_insert_contains_remove() {
        let mut set = KeyCodeSet::new();
        assert!(!set.contains(&KeyCode::A));
        set.insert(KeyCode::A);
        assert!(set.contains(&KeyCode::A));
        set.remove(KeyCode::A);
        assert!(!set.contains(&KeyCode::A));
    }

    #[test]
    fn key_set_multiple_keys() {
        let mut set = KeyCodeSet::new();
        set.insert(KeyCode::W);
        set.insert(KeyCode::Space);
        set.insert(KeyCode::F12);
        assert!(set.contains(&KeyCode::W));
        assert!(set.contains(&KeyCode::Space));
        assert!(set.contains(&KeyCode::F12));
        assert!(!set.contains(&KeyCode::A));
    }

    #[test]
    fn key_set_copy_is_independent() {
        let mut a = KeyCodeSet::new();
        a.insert(KeyCode::D);
        let b = a; // Copy
        assert!(b.contains(&KeyCode::D));
        // Modifying a doesn't affect b
        a.remove(KeyCode::D);
        assert!(b.contains(&KeyCode::D));
        assert!(!a.contains(&KeyCode::D));
    }

    #[test]
    fn mouse_set_basics() {
        let mut set = MouseButtonSet::new();
        set.insert(MouseButton::Left);
        assert!(set.contains(&MouseButton::Left));
        assert!(!set.contains(&MouseButton::Right));
        set.clear();
        assert!(!set.contains(&MouseButton::Left));
    }

    #[test]
    fn gamepad_set_basics() {
        let mut set = GamepadButtonSet::new();
        set.insert(GamepadButton::South);
        set.insert(GamepadButton::Select);
        assert!(set.contains(&GamepadButton::South));
        assert!(set.contains(&GamepadButton::Select));
        assert!(!set.contains(&GamepadButton::North));
        set.clear();
        assert!(!set.contains(&GamepadButton::South));
    }
}

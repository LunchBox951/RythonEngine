/// Keyboard key codes (game-relevant subset).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Key0,
    Key1,
    Key2,
    Key3,
    Key4,
    Key5,
    Key6,
    Key7,
    Key8,
    Key9,
    Space,
    Enter,
    Escape,
    Tab,
    Backspace,
    LeftShift,
    RightShift,
    LeftControl,
    RightControl,
    LeftAlt,
    RightAlt,
    Up,
    Down,
    Left,
    Right,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GamepadButton {
    South,
    East,
    West,
    North,
    LeftBumper,
    RightBumper,
    LeftTriggerButton,
    RightTriggerButton,
    LeftStickPress,
    RightStickPress,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
    Start,
    Select,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GamepadAxisType {
    LeftStickX,
    LeftStickY,
    RightStickX,
    RightStickY,
    LeftTrigger,
    RightTrigger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseAxisType {
    X,
    Y,
}

/// Raw hardware input events forwarded from the window/gamepad system.
#[derive(Debug, Clone)]
pub enum RawInputEvent {
    KeyPressed(KeyCode),
    KeyReleased(KeyCode),
    MouseMoved { dx: f64, dy: f64 },
    MouseButtonPressed(MouseButton),
    MouseButtonReleased(MouseButton),
    GamepadButtonPressed(GamepadButton),
    GamepadButtonReleased(GamepadButton),
    GamepadAxisChanged { axis: GamepadAxisType, value: f32 },
    GamepadConnected { name: String },
    GamepadDisconnected,
}

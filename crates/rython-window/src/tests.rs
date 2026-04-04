use rython_core::WindowConfig;

use crate::raw_events::{
    GamepadAxisType, GamepadButton, KeyCode, MouseAxisType, MouseButton, RawInputEvent,
};
use crate::window_module::WindowModule;

// ── T-WIN-01: WindowModule stores config ─────────────────────────────────────

#[test]
fn t_win_01_config_stored() {
    let cfg = WindowConfig {
        width: 1920,
        height: 1080,
        fullscreen: true,
        vsync: true,
        title: "TestWindow".to_string(),
    };
    let wm = WindowModule::new(cfg.clone());
    assert_eq!(wm.config().width, 1920);
    assert_eq!(wm.config().height, 1080);
    assert!(wm.config().fullscreen);
    assert!(wm.config().vsync);
    assert_eq!(wm.config().title, "TestWindow");
}

// ── T-WIN-02: WindowModule default config ────────────────────────────────────

#[test]
fn t_win_02_default_config() {
    let wm = WindowModule::new(WindowConfig::default());
    assert_eq!(wm.config().title, "RythonEngine");
    assert!(!wm.config().fullscreen);
    assert!(!wm.config().vsync);
    // Dimensions are non-zero
    assert!(wm.config().width > 0);
    assert!(wm.config().height > 0);
}

// ── T-WIN-03: push and drain events ──────────────────────────────────────────

#[test]
fn t_win_03_push_drain_events() {
    let wm = WindowModule::new(WindowConfig::default());

    wm.push_event(RawInputEvent::KeyPressed(KeyCode::Space));
    wm.push_event(RawInputEvent::KeyReleased(KeyCode::W));
    wm.push_event(RawInputEvent::MouseMoved { dx: 1.5, dy: -2.0 });

    let events = wm.drain_events();
    assert_eq!(events.len(), 3);

    assert!(matches!(
        events[0],
        RawInputEvent::KeyPressed(KeyCode::Space)
    ));
    assert!(matches!(events[1], RawInputEvent::KeyReleased(KeyCode::W)));
    assert!(matches!(events[2], RawInputEvent::MouseMoved { dx, dy } if dx == 1.5 && dy == -2.0));
}

// ── T-WIN-04: drain clears the queue ─────────────────────────────────────────

#[test]
fn t_win_04_drain_clears_queue() {
    let wm = WindowModule::new(WindowConfig::default());
    wm.push_event(RawInputEvent::KeyPressed(KeyCode::Enter));

    let first = wm.drain_events();
    assert_eq!(first.len(), 1);

    let second = wm.drain_events();
    assert_eq!(second.len(), 0, "drain must clear the queue");
}

// ── T-WIN-05: event_sender shares same queue ─────────────────────────────────

#[test]
fn t_win_05_event_sender_shares_queue() {
    let wm = WindowModule::new(WindowConfig::default());
    let sender = wm.event_sender();

    // Push via the Arc handle (simulates external platform loop)
    sender
        .lock()
        .unwrap()
        .push(RawInputEvent::MouseButtonPressed(MouseButton::Left));

    let events = wm.drain_events();
    assert_eq!(events.len(), 1);
    assert!(matches!(
        events[0],
        RawInputEvent::MouseButtonPressed(MouseButton::Left)
    ));
}

// ── T-WIN-06: multiple senders accumulate events ─────────────────────────────

#[test]
fn t_win_06_multiple_senders_accumulate() {
    let wm = WindowModule::new(WindowConfig::default());
    let s1 = wm.event_sender();
    let s2 = wm.event_sender();

    s1.lock()
        .unwrap()
        .push(RawInputEvent::GamepadButtonPressed(GamepadButton::South));
    s2.lock()
        .unwrap()
        .push(RawInputEvent::GamepadButtonReleased(GamepadButton::North));

    let events = wm.drain_events();
    assert_eq!(events.len(), 2);
}

// ── T-WIN-07: RawInputEvent variants are Debug + Clone ───────────────────────

#[test]
fn t_win_07_raw_input_event_debug_clone() {
    let events = vec![
        RawInputEvent::KeyPressed(KeyCode::A),
        RawInputEvent::KeyReleased(KeyCode::Escape),
        RawInputEvent::MouseMoved { dx: 0.0, dy: 0.0 },
        RawInputEvent::MouseButtonPressed(MouseButton::Right),
        RawInputEvent::MouseButtonReleased(MouseButton::Middle),
        RawInputEvent::GamepadButtonPressed(GamepadButton::LeftBumper),
        RawInputEvent::GamepadButtonReleased(GamepadButton::Select),
        RawInputEvent::GamepadAxisChanged {
            axis: GamepadAxisType::LeftStickX,
            value: 0.5,
        },
        RawInputEvent::GamepadConnected {
            name: "Controller".to_string(),
        },
        RawInputEvent::GamepadDisconnected,
    ];

    for e in &events {
        let cloned = e.clone();
        // Debug must not panic
        let _ = format!("{:?}", cloned);
    }
}

// ── T-WIN-08: KeyCode covers all letter keys ─────────────────────────────────

#[test]
fn t_win_08_keycode_equality() {
    assert_eq!(KeyCode::A, KeyCode::A);
    assert_ne!(KeyCode::A, KeyCode::B);
    assert_eq!(KeyCode::F12, KeyCode::F12);
    assert_ne!(KeyCode::Left, KeyCode::Right);
}

// ── T-WIN-09: MouseButton and GamepadButton equality ─────────────────────────

#[test]
fn t_win_09_button_equality() {
    assert_eq!(MouseButton::Left, MouseButton::Left);
    assert_ne!(MouseButton::Left, MouseButton::Right);

    assert_eq!(GamepadButton::DPadUp, GamepadButton::DPadUp);
    assert_ne!(GamepadButton::DPadUp, GamepadButton::DPadDown);
}

// ── T-WIN-10: GamepadAxisType and MouseAxisType equality ─────────────────────

#[test]
fn t_win_10_axis_type_equality() {
    assert_eq!(GamepadAxisType::LeftStickX, GamepadAxisType::LeftStickX);
    assert_ne!(GamepadAxisType::LeftTrigger, GamepadAxisType::RightTrigger);

    assert_eq!(MouseAxisType::X, MouseAxisType::X);
    assert_ne!(MouseAxisType::X, MouseAxisType::Y);
}

// ── T-WIN-11: push_event and event_sender point to same Arc ──────────────────

#[test]
fn t_win_11_event_sender_arc_identity() {
    let wm = WindowModule::new(WindowConfig::default());
    let s = wm.event_sender();

    // push_event via module method, then read via the Arc handle
    wm.push_event(RawInputEvent::KeyPressed(KeyCode::Up));
    let queue = s.lock().unwrap();
    assert_eq!(queue.len(), 1);
}

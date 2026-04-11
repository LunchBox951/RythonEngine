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

    assert!(matches!(events[0], RawInputEvent::KeyPressed(KeyCode::Space)));
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

// ── T-WIN-12: high throughput events ─────────────────────────────────────────

#[test]
fn t_win_12_high_throughput_events() {
    let wm = WindowModule::new(WindowConfig::default());
    for i in 0..10_000 {
        wm.push_event(RawInputEvent::GamepadAxisChanged {
            axis: GamepadAxisType::LeftStickX,
            value: i as f32,
        });
    }
    let events = wm.drain_events();
    assert_eq!(events.len(), 10_000, "all 10,000 events must be received");
    // Verify ordering is preserved
    for (i, e) in events.iter().enumerate() {
        match e {
            RawInputEvent::GamepadAxisChanged { value, .. } => {
                assert_eq!(*value, i as f32, "event {i} out of order");
            }
            _ => panic!("unexpected event variant at index {i}"),
        }
    }
}

// ── T-WIN-13: drain during push ──────────────────────────────────────────────

#[test]
fn t_win_13_drain_during_push() {
    let wm = WindowModule::new(WindowConfig::default());

    // First batch
    wm.push_event(RawInputEvent::KeyPressed(KeyCode::A));
    wm.push_event(RawInputEvent::KeyPressed(KeyCode::B));
    let first = wm.drain_events();
    assert_eq!(first.len(), 2, "first drain must return 2 events");

    // Second batch — only new events
    wm.push_event(RawInputEvent::KeyPressed(KeyCode::C));
    let second = wm.drain_events();
    assert_eq!(second.len(), 1, "second drain must return only events since last drain");
    assert!(matches!(second[0], RawInputEvent::KeyPressed(KeyCode::C)));

    // Third drain — empty
    let third = wm.drain_events();
    assert_eq!(third.len(), 0, "drain with no new events must be empty");
}

// ── T-WIN-14: all event variant push/drain ───────────────────────────────────

#[test]
fn t_win_14_all_event_variant_push_drain() {
    let wm = WindowModule::new(WindowConfig::default());

    wm.push_event(RawInputEvent::KeyPressed(KeyCode::Space));
    wm.push_event(RawInputEvent::KeyReleased(KeyCode::Enter));
    wm.push_event(RawInputEvent::MouseMoved { dx: 1.0, dy: -1.0 });
    wm.push_event(RawInputEvent::MouseButtonPressed(MouseButton::Left));
    wm.push_event(RawInputEvent::MouseButtonReleased(MouseButton::Right));
    wm.push_event(RawInputEvent::GamepadButtonPressed(GamepadButton::South));
    wm.push_event(RawInputEvent::GamepadButtonReleased(GamepadButton::North));
    wm.push_event(RawInputEvent::GamepadAxisChanged {
        axis: GamepadAxisType::RightStickY,
        value: 0.75,
    });
    wm.push_event(RawInputEvent::GamepadConnected {
        name: "TestPad".to_string(),
    });
    wm.push_event(RawInputEvent::GamepadDisconnected);

    let events = wm.drain_events();
    assert_eq!(events.len(), 10, "must recover all 10 event variants");

    assert!(matches!(events[0], RawInputEvent::KeyPressed(KeyCode::Space)));
    assert!(matches!(events[1], RawInputEvent::KeyReleased(KeyCode::Enter)));
    assert!(matches!(events[2], RawInputEvent::MouseMoved { dx, dy } if dx == 1.0 && dy == -1.0));
    assert!(matches!(events[3], RawInputEvent::MouseButtonPressed(MouseButton::Left)));
    assert!(matches!(events[4], RawInputEvent::MouseButtonReleased(MouseButton::Right)));
    assert!(matches!(events[5], RawInputEvent::GamepadButtonPressed(GamepadButton::South)));
    assert!(matches!(events[6], RawInputEvent::GamepadButtonReleased(GamepadButton::North)));
    assert!(matches!(
        events[7],
        RawInputEvent::GamepadAxisChanged { axis: GamepadAxisType::RightStickY, value }
        if (value - 0.75).abs() < f32::EPSILON
    ));
    assert!(matches!(events[8], RawInputEvent::GamepadConnected { ref name } if name == "TestPad"));
    assert!(matches!(events[9], RawInputEvent::GamepadDisconnected));
}

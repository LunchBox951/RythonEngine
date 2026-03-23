use rython_input::*;
use rython_window::*;

// ─── Helper ──────────────────────────────────────────────────────────────────

fn make_gameplay_controller() -> PlayerController {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut map = InputMap::new("gameplay");
    map.bind_axis("move_x", AxisBinding::KBAxis { negative: KeyCode::A, positive: KeyCode::D });
    map.bind_axis("move_y", AxisBinding::KBAxis { negative: KeyCode::S, positive: KeyCode::W });
    map.bind_button("jump", ButtonBinding::Keyboard(KeyCode::Space));
    map.bind_button("attack", ButtonBinding::Keyboard(KeyCode::Enter));
    map.bind_button("sprint", ButtonBinding::Keyboard(KeyCode::LeftShift));
    ctrl.register_map(map);
    ctrl
}

use rython_core::OwnerId;

// ─── T-INP-01: Keyboard Axis — Positive Key ──────────────────────────────────

#[test]
fn t_inp_01_keyboard_axis_positive_key() {
    let mut ctrl = make_gameplay_controller();
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::D)]);
    let snap = ctrl.get_snapshot(1).unwrap();
    assert_eq!(snap.axis("move_x"), 1.0);
}

// ─── T-INP-02: Keyboard Axis — Negative Key ──────────────────────────────────

#[test]
fn t_inp_02_keyboard_axis_negative_key() {
    let mut ctrl = make_gameplay_controller();
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::A)]);
    let snap = ctrl.get_snapshot(1).unwrap();
    assert_eq!(snap.axis("move_x"), -1.0);
}

// ─── T-INP-03: Keyboard Axis — Both Keys Cancel ──────────────────────────────

#[test]
fn t_inp_03_keyboard_axis_both_keys_cancel() {
    let mut ctrl = make_gameplay_controller();
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::A), RawInputEvent::KeyPressed(KeyCode::D)]);
    let snap = ctrl.get_snapshot(1).unwrap();
    assert_eq!(snap.axis("move_x"), 0.0);
}

// ─── T-INP-04: Keyboard Axis — No Keys ───────────────────────────────────────

#[test]
fn t_inp_04_keyboard_axis_no_keys() {
    let mut ctrl = make_gameplay_controller();
    ctrl.tick(&[]);
    let snap = ctrl.get_snapshot(1).unwrap();
    assert_eq!(snap.axis("move_x"), 0.0);
}

// ─── T-INP-05: Button Press / Hold / Release Lifecycle ───────────────────────

#[test]
fn t_inp_05_button_press_hold_release_lifecycle() {
    let mut ctrl = make_gameplay_controller();

    // Frame 1: press
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::Space)]);
    let s = ctrl.get_snapshot(1).unwrap();
    assert!(s.pressed("jump"), "frame 1: pressed");
    assert!(s.held("jump"), "frame 1: held");
    assert!(!s.released("jump"), "frame 1: not released");

    // Frame 2: hold (no new events; key remains down)
    ctrl.tick(&[]);
    let s = ctrl.get_snapshot(1).unwrap();
    assert!(!s.pressed("jump"), "frame 2: not pressed");
    assert!(s.held("jump"), "frame 2: held");
    assert!(!s.released("jump"), "frame 2: not released");

    // Frame 3: release
    ctrl.tick(&[RawInputEvent::KeyReleased(KeyCode::Space)]);
    let s = ctrl.get_snapshot(1).unwrap();
    assert!(!s.pressed("jump"), "frame 3: not pressed");
    assert!(!s.held("jump"), "frame 3: not held");
    assert!(s.released("jump"), "frame 3: released");

    // Frame 4: nothing
    ctrl.tick(&[]);
    let s = ctrl.get_snapshot(1).unwrap();
    assert!(!s.pressed("jump"), "frame 4: not pressed");
    assert!(!s.held("jump"), "frame 4: not held");
    assert!(!s.released("jump"), "frame 4: not released");
}

// ─── T-INP-06: Input Map Switching ───────────────────────────────────────────

#[test]
fn t_inp_06_input_map_switching() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);

    let mut gameplay = InputMap::new("gameplay");
    gameplay.bind_button("jump", ButtonBinding::Keyboard(KeyCode::Space));
    ctrl.register_map(gameplay);

    let mut menu = InputMap::new("menu");
    menu.bind_button("confirm", ButtonBinding::Keyboard(KeyCode::Enter));
    ctrl.register_map(menu);

    // Gameplay map is active (first registered)
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::Space)]);
    let s = ctrl.get_snapshot(owner).unwrap();
    assert!(s.pressed("jump"), "gameplay: jump pressed with Space");
    assert!(!s.pressed("confirm"), "gameplay: confirm not in map → false");

    // Switch to menu
    ctrl.set_active_map("menu", owner).unwrap();
    ctrl.tick(&[
        RawInputEvent::KeyReleased(KeyCode::Space),
        RawInputEvent::KeyPressed(KeyCode::Enter),
    ]);
    let s = ctrl.get_snapshot(owner).unwrap();
    assert!(s.pressed("confirm"), "menu: confirm pressed with Enter");
    assert!(!s.pressed("jump"), "menu: jump not in map → false");
}

// ─── T-INP-07: Unbound Action Returns Default ────────────────────────────────

#[test]
fn t_inp_07_unbound_action_returns_default() {
    let mut ctrl = make_gameplay_controller();
    ctrl.tick(&[]);
    let s = ctrl.get_snapshot(1).unwrap();
    assert_eq!(s.axis("nonexistent_action"), 0.0);
    assert!(!s.pressed("nonexistent"));
    assert!(!s.held("nonexistent"));
    assert!(!s.released("nonexistent"));
}

// ─── T-INP-08: Input Locking — Events Suppressed ─────────────────────────────

#[test]
fn t_inp_08_input_locking_events_suppressed() {
    let mut ctrl = make_gameplay_controller();
    ctrl.lock();
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::Space)]);
    let s = ctrl.get_snapshot(1).unwrap();
    assert!(!s.pressed("jump"), "locked: pressed should be false");
    assert!(ctrl.pending_events().lock().unwrap().is_empty(), "locked: no events emitted");
}

// ─── T-INP-09: Input Locking — Unlock Restores ───────────────────────────────

#[test]
fn t_inp_09_input_locking_unlock_restores() {
    let mut ctrl = make_gameplay_controller();
    ctrl.lock();
    ctrl.unlock();
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::Space)]);
    let s = ctrl.get_snapshot(1).unwrap();
    assert!(s.pressed("jump"), "unlocked: pressed should be true");
    assert!(!ctrl.pending_events().lock().unwrap().is_empty(), "unlocked: event emitted");
}

// ─── T-INP-10: Event-Driven Input Fires Events ───────────────────────────────

#[test]
fn t_inp_10_event_driven_input_fires_events() {
    let mut ctrl = make_gameplay_controller();
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::Space)]);
    let events = ctrl.pending_events();
    let evts = events.lock().unwrap();
    assert_eq!(evts.len(), 1, "exactly one event on press");
    assert_eq!(evts[0].action, "jump");
    assert_eq!(evts[0].value, 1.0);
}

// ─── T-INP-11: Ownership — Non-Owner Rejected ────────────────────────────────

#[test]
fn t_inp_11_ownership_non_owner_rejected() {
    let mut ctrl = make_gameplay_controller();
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::Space)]);
    assert!(ctrl.get_snapshot(1).is_ok(), "owner succeeds");
    assert!(ctrl.get_snapshot(2).is_err(), "non-owner rejected");
}

// ─── T-INP-12: Ownership Transfer Succeeds ───────────────────────────────────

#[test]
fn t_inp_12_ownership_transfer_succeeds() {
    let mut ctrl = make_gameplay_controller();

    // Transfer from owner 1 to owner 2
    ctrl.set_owner(2);
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::Space)]);

    assert!(ctrl.get_snapshot(2).is_ok(), "new owner succeeds");
    assert!(ctrl.get_snapshot(1).is_err(), "old owner rejected");
    assert!(ctrl.set_active_map("gameplay", 2).is_ok(), "new owner can switch map");
    assert!(ctrl.set_active_map("gameplay", 1).is_err(), "old owner cannot switch map");
}

// ─── T-INP-13: Gamepad Axis Range ────────────────────────────────────────────

#[test]
fn t_inp_13_gamepad_axis_range() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut map = InputMap::new("gameplay");
    map.bind_axis(
        "move_x",
        AxisBinding::GamepadAxis { axis: GamepadAxisType::LeftStickX },
    );
    ctrl.register_map(map);

    // Full positive deflection
    ctrl.tick(&[
        RawInputEvent::GamepadConnected { name: "TestPad".into() },
        RawInputEvent::GamepadAxisChanged { axis: GamepadAxisType::LeftStickX, value: 1.0 },
    ]);
    assert_eq!(ctrl.get_snapshot(owner).unwrap().axis("move_x"), 1.0);

    // Full negative deflection
    ctrl.tick(&[RawInputEvent::GamepadAxisChanged {
        axis: GamepadAxisType::LeftStickX,
        value: -1.0,
    }]);
    assert_eq!(ctrl.get_snapshot(owner).unwrap().axis("move_x"), -1.0);

    // At rest
    ctrl.tick(&[RawInputEvent::GamepadAxisChanged {
        axis: GamepadAxisType::LeftStickX,
        value: 0.0,
    }]);
    assert_eq!(ctrl.get_snapshot(owner).unwrap().axis("move_x"), 0.0);
}

// ─── T-INP-14: Multiple Bindings Same Action ─────────────────────────────────

#[test]
fn t_inp_14_multiple_bindings_same_action() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut map = InputMap::new("gameplay");
    map.bind_axis("move_x", AxisBinding::KBAxis { negative: KeyCode::A, positive: KeyCode::D });
    map.bind_axis(
        "move_x",
        AxisBinding::GamepadAxis { axis: GamepadAxisType::LeftStickX },
    );
    ctrl.register_map(map);

    // Keyboard only: D pressed → 1.0
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::D)]);
    assert_eq!(ctrl.get_snapshot(owner).unwrap().axis("move_x"), 1.0);

    // Gamepad higher absolute value wins over keyboard (keyboard released, gamepad at -0.75)
    ctrl.tick(&[
        RawInputEvent::KeyReleased(KeyCode::D),
        RawInputEvent::GamepadAxisChanged { axis: GamepadAxisType::LeftStickX, value: -0.75 },
    ]);
    assert_eq!(ctrl.get_snapshot(owner).unwrap().axis("move_x"), -0.75);
}

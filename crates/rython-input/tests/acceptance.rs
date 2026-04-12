use rython_input::*;
use rython_window::*;

// ─── Helper ──────────────────────────────────────────────────────────────────

fn make_gameplay_controller() -> PlayerController {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut map = InputMap::new("gameplay");
    map.bind_axis(
        "move_x",
        AxisBinding::KBAxis {
            negative: KeyCode::A,
            positive: KeyCode::D,
        },
    );
    map.bind_axis(
        "move_y",
        AxisBinding::KBAxis {
            negative: KeyCode::S,
            positive: KeyCode::W,
        },
    );
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
    ctrl.tick(&[
        RawInputEvent::KeyPressed(KeyCode::A),
        RawInputEvent::KeyPressed(KeyCode::D),
    ]);
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
    assert!(
        !s.pressed("confirm"),
        "gameplay: confirm not in map → false"
    );

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
    assert!(
        ctrl.pending_events().lock().unwrap().is_empty(),
        "locked: no events emitted"
    );
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
    assert!(
        !ctrl.pending_events().lock().unwrap().is_empty(),
        "unlocked: event emitted"
    );
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
    assert!(
        ctrl.set_active_map("gameplay", 2).is_ok(),
        "new owner can switch map"
    );
    assert!(
        ctrl.set_active_map("gameplay", 1).is_err(),
        "old owner cannot switch map"
    );
}

// ─── T-INP-13: Gamepad Axis Range ────────────────────────────────────────────

#[test]
fn t_inp_13_gamepad_axis_range() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut map = InputMap::new("gameplay");
    map.bind_axis(
        "move_x",
        AxisBinding::GamepadAxis {
            axis: GamepadAxisType::LeftStickX,
        },
    );
    ctrl.register_map(map);

    // Full positive deflection
    ctrl.tick(&[
        RawInputEvent::GamepadConnected {
            name: "TestPad".into(),
        },
        RawInputEvent::GamepadAxisChanged {
            axis: GamepadAxisType::LeftStickX,
            value: 1.0,
        },
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
    map.bind_axis(
        "move_x",
        AxisBinding::KBAxis {
            negative: KeyCode::A,
            positive: KeyCode::D,
        },
    );
    map.bind_axis(
        "move_x",
        AxisBinding::GamepadAxis {
            axis: GamepadAxisType::LeftStickX,
        },
    );
    ctrl.register_map(map);

    // Keyboard only: D pressed → 1.0
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::D)]);
    assert_eq!(ctrl.get_snapshot(owner).unwrap().axis("move_x"), 1.0);

    // Gamepad higher absolute value wins over keyboard (keyboard released, gamepad at -0.75)
    ctrl.tick(&[
        RawInputEvent::KeyReleased(KeyCode::D),
        RawInputEvent::GamepadAxisChanged {
            axis: GamepadAxisType::LeftStickX,
            value: -0.75,
        },
    ]);
    assert_eq!(ctrl.get_snapshot(owner).unwrap().axis("move_x"), -0.75);
}

// ─── T-INP-15: Mouse Axis X Binding ─────────────────────────────────────────

#[test]
fn t_inp_15_mouse_axis_x_binding() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut map = InputMap::new("gameplay");
    map.bind_axis(
        "look_x",
        AxisBinding::MouseAxis {
            axis: MouseAxisType::X,
        },
    );
    ctrl.register_map(map);

    ctrl.tick(&[RawInputEvent::MouseMoved { dx: 5.0, dy: 0.0 }]);
    assert_eq!(ctrl.get_snapshot(owner).unwrap().axis("look_x"), 5.0);
}

// ─── T-INP-16: Mouse Axis Y Binding ─────────────────────────────────────────

#[test]
fn t_inp_16_mouse_axis_y_binding() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut map = InputMap::new("gameplay");
    map.bind_axis(
        "look_y",
        AxisBinding::MouseAxis {
            axis: MouseAxisType::Y,
        },
    );
    ctrl.register_map(map);

    ctrl.tick(&[RawInputEvent::MouseMoved { dx: 0.0, dy: -3.0 }]);
    assert_eq!(ctrl.get_snapshot(owner).unwrap().axis("look_y"), -3.0);
}

// ─── T-INP-17: Mouse Delta Accumulates Within One Tick ───────────────────────

#[test]
fn t_inp_17_mouse_delta_accumulates_within_tick() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut map = InputMap::new("gameplay");
    map.bind_axis(
        "look_x",
        AxisBinding::MouseAxis {
            axis: MouseAxisType::X,
        },
    );
    ctrl.register_map(map);

    // Two mouse-moved events in the same tick — deltas must be summed.
    ctrl.tick(&[
        RawInputEvent::MouseMoved { dx: 2.0, dy: 0.0 },
        RawInputEvent::MouseMoved { dx: 3.0, dy: 0.0 },
    ]);
    assert_eq!(ctrl.get_snapshot(owner).unwrap().axis("look_x"), 5.0);
}

// ─── T-INP-18: Mouse Delta Resets To Zero Each Tick ──────────────────────────

#[test]
fn t_inp_18_mouse_delta_resets_each_tick() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut map = InputMap::new("gameplay");
    map.bind_axis(
        "look_x",
        AxisBinding::MouseAxis {
            axis: MouseAxisType::X,
        },
    );
    ctrl.register_map(map);

    // Tick 1: mouse moves.
    ctrl.tick(&[RawInputEvent::MouseMoved { dx: 5.0, dy: 0.0 }]);
    assert_eq!(
        ctrl.get_snapshot(owner).unwrap().axis("look_x"),
        5.0,
        "tick 1: delta present"
    );

    // Tick 2: no events — delta must be 0, not carried over.
    ctrl.tick(&[]);
    assert_eq!(
        ctrl.get_snapshot(owner).unwrap().axis("look_x"),
        0.0,
        "tick 2: delta cleared"
    );
}

// ─── T-INP-19: Mouse Button Binding — Press / Hold / Release ─────────────────

#[test]
fn t_inp_19_mouse_button_binding_lifecycle() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut map = InputMap::new("gameplay");
    map.bind_button("fire", ButtonBinding::Mouse(MouseButton::Left));
    ctrl.register_map(map);

    // Frame 1: press
    ctrl.tick(&[RawInputEvent::MouseButtonPressed(MouseButton::Left)]);
    let s = ctrl.get_snapshot(owner).unwrap();
    assert!(s.pressed("fire"), "frame 1: pressed");
    assert!(s.held("fire"), "frame 1: held");
    assert!(!s.released("fire"), "frame 1: not released");

    // Frame 2: hold
    ctrl.tick(&[]);
    let s = ctrl.get_snapshot(owner).unwrap();
    assert!(!s.pressed("fire"), "frame 2: not pressed again");
    assert!(s.held("fire"), "frame 2: still held");

    // Frame 3: release
    ctrl.tick(&[RawInputEvent::MouseButtonReleased(MouseButton::Left)]);
    let s = ctrl.get_snapshot(owner).unwrap();
    assert!(!s.pressed("fire"), "frame 3: not pressed");
    assert!(!s.held("fire"), "frame 3: not held");
    assert!(s.released("fire"), "frame 3: released");
}

// ─── T-INP-20: Gamepad Button Binding — Press / Hold / Release ───────────────

#[test]
fn t_inp_20_gamepad_button_binding_lifecycle() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut map = InputMap::new("gameplay");
    map.bind_button("jump", ButtonBinding::Gamepad(GamepadButton::South));
    ctrl.register_map(map);

    // Frame 1: press
    ctrl.tick(&[
        RawInputEvent::GamepadConnected {
            name: "TestPad".into(),
        },
        RawInputEvent::GamepadButtonPressed(GamepadButton::South),
    ]);
    let s = ctrl.get_snapshot(owner).unwrap();
    assert!(s.pressed("jump"), "frame 1: pressed");
    assert!(s.held("jump"), "frame 1: held");
    assert!(!s.released("jump"), "frame 1: not released");

    // Frame 2: hold
    ctrl.tick(&[]);
    let s = ctrl.get_snapshot(owner).unwrap();
    assert!(!s.pressed("jump"), "frame 2: not pressed");
    assert!(s.held("jump"), "frame 2: held");

    // Frame 3: release
    ctrl.tick(&[RawInputEvent::GamepadButtonReleased(GamepadButton::South)]);
    let s = ctrl.get_snapshot(owner).unwrap();
    assert!(!s.held("jump"), "frame 3: not held");
    assert!(s.released("jump"), "frame 3: released");
}

// ─── T-INP-21: Gamepad Disconnected Clears State ─────────────────────────────

#[test]
fn t_inp_21_gamepad_disconnected_clears_state() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut map = InputMap::new("gameplay");
    map.bind_axis(
        "move_x",
        AxisBinding::GamepadAxis {
            axis: GamepadAxisType::LeftStickX,
        },
    );
    map.bind_button("jump", ButtonBinding::Gamepad(GamepadButton::South));
    ctrl.register_map(map);

    // Connect and push a value.
    ctrl.tick(&[
        RawInputEvent::GamepadConnected {
            name: "TestPad".into(),
        },
        RawInputEvent::GamepadAxisChanged {
            axis: GamepadAxisType::LeftStickX,
            value: 0.8,
        },
        RawInputEvent::GamepadButtonPressed(GamepadButton::South),
    ]);
    assert_eq!(ctrl.get_snapshot(owner).unwrap().axis("move_x"), 0.8);
    assert_eq!(ctrl.active_backend(), "gamepad");

    // Disconnect: axes and buttons must be cleared.
    ctrl.tick(&[RawInputEvent::GamepadDisconnected]);
    let s = ctrl.get_snapshot(owner).unwrap();
    assert_eq!(s.axis("move_x"), 0.0, "axis cleared after disconnect");
    assert!(!s.held("jump"), "button cleared after disconnect");
    assert_eq!(ctrl.active_backend(), "keyboard_mouse");
    assert!(ctrl.gamepad_info().is_none(), "no name after disconnect");
}

// ─── T-INP-22: Gamepad Info Tracks Connect / Disconnect ──────────────────────

#[test]
fn t_inp_22_gamepad_info_tracks_connect_disconnect() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut map = InputMap::new("gameplay");
    map.bind_button("jump", ButtonBinding::Gamepad(GamepadButton::South));
    ctrl.register_map(map);

    assert_eq!(ctrl.active_backend(), "keyboard_mouse");
    assert!(ctrl.gamepad_info().is_none());

    ctrl.tick(&[RawInputEvent::GamepadConnected {
        name: "Xbox Controller".into(),
    }]);
    assert_eq!(ctrl.active_backend(), "gamepad");
    assert_eq!(ctrl.gamepad_info(), Some("Xbox Controller"));

    ctrl.tick(&[RawInputEvent::GamepadDisconnected]);
    assert_eq!(ctrl.active_backend(), "keyboard_mouse");
    assert!(ctrl.gamepad_info().is_none());
}

// ─── T-INP-23: Axis Below Deadzone Emits No Event ────────────────────────────

#[test]
fn t_inp_23_axis_below_deadzone_emits_no_event() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut map = InputMap::new("gameplay");
    map.bind_axis(
        "move_x",
        AxisBinding::GamepadAxis {
            axis: GamepadAxisType::LeftStickX,
        },
    );
    ctrl.register_map(map);

    ctrl.tick(&[
        RawInputEvent::GamepadConnected {
            name: "TestPad".into(),
        },
        RawInputEvent::GamepadAxisChanged {
            axis: GamepadAxisType::LeftStickX,
            value: 0.05,
        },
    ]);
    // Snapshot value reflects raw input.
    assert_eq!(ctrl.get_snapshot(owner).unwrap().axis("move_x"), 0.05);
    // But no axis event should have been emitted (both prev and curr below deadzone 0.1).
    let evts = ctrl.pending_events();
    let evts = evts.lock().unwrap();
    assert!(
        evts.iter().all(|e| !e.action.starts_with("axis:")),
        "no axis event below deadzone; got: {:?}",
        *evts
    );
}

// ─── T-INP-24: Axis Crossing Deadzone Emits Event ────────────────────────────

#[test]
fn t_inp_24_axis_crossing_deadzone_emits_event() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut map = InputMap::new("gameplay");
    map.bind_axis(
        "move_x",
        AxisBinding::GamepadAxis {
            axis: GamepadAxisType::LeftStickX,
        },
    );
    ctrl.register_map(map);

    // Start at rest (no event).
    ctrl.tick(&[RawInputEvent::GamepadConnected {
        name: "TestPad".into(),
    }]);

    // Cross the deadzone threshold (0.1) — an axis event must fire.
    ctrl.tick(&[RawInputEvent::GamepadAxisChanged {
        axis: GamepadAxisType::LeftStickX,
        value: 0.5,
    }]);
    let evts = ctrl.pending_events();
    let evts = evts.lock().unwrap();
    let axis_evts: Vec<_> = evts.iter().filter(|e| e.action == "axis:move_x").collect();
    assert_eq!(axis_evts.len(), 1, "one axis event after deadzone crossing");
    assert_eq!(axis_evts[0].value, 0.5);
}

// ─── T-INP-25: Button Release Emits Event With Value 0.0 ─────────────────────

#[test]
fn t_inp_25_button_release_emits_event_value_zero() {
    let mut ctrl = make_gameplay_controller();

    // Press → release.
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::Space)]);
    ctrl.tick(&[RawInputEvent::KeyReleased(KeyCode::Space)]);

    let evts = ctrl.pending_events();
    let evts = evts.lock().unwrap();
    // Two events: press (1.0) then release (0.0).
    assert_eq!(evts.len(), 2, "expect press + release events");
    assert_eq!(evts[0].action, "jump");
    assert_eq!(evts[0].value, 1.0, "press value");
    assert_eq!(evts[1].action, "jump");
    assert_eq!(evts[1].value, 0.0, "release value");
}

// ─── T-INP-26: Pending Events Accumulate Across Ticks ────────────────────────

#[test]
fn t_inp_26_pending_events_accumulate_across_ticks() {
    let mut ctrl = make_gameplay_controller();

    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::Space)]); // jump press
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::Enter)]); // attack press (jump still held)
    ctrl.tick(&[RawInputEvent::KeyReleased(KeyCode::Space)]); // jump release

    let evts = ctrl.pending_events();
    let evts = evts.lock().unwrap();
    // jump:press, attack:press, jump:release = 3 events total (not auto-cleared).
    assert_eq!(evts.len(), 3, "events accumulate: {:?}", *evts);
}

// ─── T-INP-27: No Map Registered — Snapshot Returns Defaults ─────────────────

#[test]
fn t_inp_27_no_map_registered_snapshot_defaults() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::Space)]);
    let s = ctrl.get_snapshot(owner).unwrap();
    assert_eq!(s.axis("anything"), 0.0);
    assert!(!s.pressed("anything"));
    assert!(!s.held("anything"));
    assert!(!s.released("anything"));
    assert!(ctrl.pending_events().lock().unwrap().is_empty());
}

// ─── T-INP-28: Set Active Map To Nonexistent Returns Error ───────────────────

#[test]
fn t_inp_28_set_active_map_nonexistent_returns_error() {
    let mut ctrl = make_gameplay_controller();
    let result = ctrl.set_active_map("doesnotexist", 1);
    assert!(result.is_err(), "expected error for nonexistent map name");
}

// ─── T-INP-29: Key Pressed While Locked Appears As Held After Unlock ──────────

#[test]
fn t_inp_29_key_pressed_while_locked_appears_as_held_after_unlock() {
    let mut ctrl = make_gameplay_controller();

    // Lock and press a key.
    ctrl.lock();
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::Space)]);
    // Locked: snapshot must be empty.
    let s = ctrl.get_snapshot(1).unwrap();
    assert!(!s.pressed("jump"), "locked: no press");
    assert!(!s.held("jump"), "locked: no held");

    // Unlock and tick with no new events: key is still down.
    // previous_keys == current_keys (both have Space), so pressed=false, held=true.
    ctrl.unlock();
    ctrl.tick(&[]);
    let s = ctrl.get_snapshot(1).unwrap();
    assert!(
        !s.pressed("jump"),
        "after unlock: not pressed (was already down)"
    );
    assert!(
        s.held("jump"),
        "after unlock: held because key is still physically down"
    );
}

// ─── T-INP-30: Simultaneous Conflicting KB + Gamepad ─────────────────────────

#[test]
fn t_inp_30_simultaneous_conflicting_kb_gamepad() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut map = InputMap::new("gameplay");
    map.bind_axis(
        "move_x",
        AxisBinding::KBAxis {
            negative: KeyCode::A,
            positive: KeyCode::D,
        },
    );
    map.bind_axis(
        "move_x",
        AxisBinding::GamepadAxis {
            axis: GamepadAxisType::LeftStickX,
        },
    );
    ctrl.register_map(map);

    // Press A (keyboard negative = -1.0) AND send gamepad positive (+1.0) in the same tick.
    ctrl.tick(&[
        RawInputEvent::GamepadConnected {
            name: "TestPad".into(),
        },
        RawInputEvent::KeyPressed(KeyCode::A),
        RawInputEvent::GamepadAxisChanged {
            axis: GamepadAxisType::LeftStickX,
            value: 1.0,
        },
    ]);
    let val = ctrl.get_snapshot(owner).unwrap().axis("move_x");

    // Behavior: the binding with the highest absolute value wins (see T-INP-14).
    // Both are |1.0|, so the result should be one of them. Gamepad was processed
    // second and has equal abs value, so it wins as the "higher absolute value"
    // tiebreaker.  Document: engine uses "highest absolute value wins" strategy.
    assert!(
        (val - 1.0).abs() < 1e-5 || (val - (-1.0)).abs() < 1e-5,
        "simultaneous KB+gamepad: axis must resolve to +1.0 or -1.0, got {}",
        val
    );
}

// ─── T-INP-31: Dead Zone — Gamepad Axis ──────────────────────────────────────
//
// The engine uses a hardcoded AXIS_DEADZONE = 0.1 for event emission only.
// The snapshot axis value reflects raw input regardless of the dead zone.
// There is no configurable set_dead_zone API — this is a potential enhancement.

#[test]
fn t_inp_31_dead_zone_gamepad_axis() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut map = InputMap::new("gameplay");
    map.bind_axis(
        "move_x",
        AxisBinding::GamepadAxis {
            axis: GamepadAxisType::LeftStickX,
        },
    );
    ctrl.register_map(map);

    // No configurable dead zone API exists; the hardcoded AXIS_DEADZONE (0.1) only
    // affects event emission, not the snapshot value. Small values pass through unchanged.

    // Value below dead zone (0.05 < 0.1): snapshot still reflects raw value
    ctrl.tick(&[
        RawInputEvent::GamepadConnected {
            name: "TestPad".into(),
        },
        RawInputEvent::GamepadAxisChanged {
            axis: GamepadAxisType::LeftStickX,
            value: 0.05,
        },
    ]);
    assert_eq!(
        ctrl.get_snapshot(owner).unwrap().axis("move_x"),
        0.05,
        "below dead zone: raw value passes through to snapshot"
    );

    // Value above dead zone (0.2 > 0.1): snapshot reflects raw value
    ctrl.tick(&[RawInputEvent::GamepadAxisChanged {
        axis: GamepadAxisType::LeftStickX,
        value: 0.2,
    }]);
    assert_eq!(
        ctrl.get_snapshot(owner).unwrap().axis("move_x"),
        0.2,
        "above dead zone: raw value passes through to snapshot"
    );
}

// ─── T-INP-32: Input Map Switch Mid-Frame ────────────────────────────────────

#[test]
fn t_inp_32_input_map_switch_mid_frame() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);

    let mut gameplay = InputMap::new("gameplay");
    gameplay.bind_button("jump", ButtonBinding::Keyboard(KeyCode::Space));
    ctrl.register_map(gameplay);

    let mut menu = InputMap::new("menu");
    menu.bind_button("confirm", ButtonBinding::Keyboard(KeyCode::Enter));
    ctrl.register_map(menu);

    // Switch to "menu" before ticking
    ctrl.set_active_map("menu", owner).unwrap();

    // Tick with Space (bound only in "gameplay") — should NOT be active
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::Space)]);
    let s = ctrl.get_snapshot(owner).unwrap();
    assert!(
        !s.pressed("jump"),
        "menu map active: jump must not be pressed"
    );
    assert!(!s.held("jump"), "menu map active: jump must not be held");

    // Enter (bound in "menu") should work
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::Enter)]);
    let s = ctrl.get_snapshot(owner).unwrap();
    assert!(
        s.pressed("confirm"),
        "menu map active: confirm must be pressed"
    );
}

// ─── T-INP-33: Unbound Axis Returns Zero ─────────────────────────────────────

#[test]
fn t_inp_33_unbound_axis_returns_zero() {
    let mut ctrl = make_gameplay_controller();
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::D)]);
    let s = ctrl.get_snapshot(1).unwrap();

    // "nonexistent_axis" is not bound in any map
    assert_eq!(
        s.axis("nonexistent_axis"),
        0.0,
        "unbound axis must return 0.0"
    );
}

// ─── T-INP-34: Unbound Button Returns False ──────────────────────────────────

#[test]
fn t_inp_34_unbound_button_returns_false() {
    let mut ctrl = make_gameplay_controller();
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::Space)]);
    let s = ctrl.get_snapshot(1).unwrap();

    // "nonexistent_button" is not bound in any map
    assert!(
        !s.pressed("nonexistent_button"),
        "unbound button: pressed must be false"
    );
    assert!(
        !s.held("nonexistent_button"),
        "unbound button: held must be false"
    );
    assert!(
        !s.released("nonexistent_button"),
        "unbound button: released must be false"
    );
}

// ─── T-INP-35: Multiple Bindings Same Action — KB + Gamepad ──────────────────

#[test]
fn t_inp_35_multiple_bindings_same_action() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut map = InputMap::new("gameplay");
    map.bind_button("jump", ButtonBinding::Keyboard(KeyCode::Space));
    map.bind_button("jump", ButtonBinding::Gamepad(GamepadButton::South));
    ctrl.register_map(map);

    // Frame 1: Press Space → jump pressed via keyboard
    ctrl.tick(&[
        RawInputEvent::GamepadConnected {
            name: "TestPad".into(),
        },
        RawInputEvent::KeyPressed(KeyCode::Space),
    ]);
    let s = ctrl.get_snapshot(owner).unwrap();
    assert!(s.pressed("jump"), "frame 1: jump pressed via Space");
    assert!(s.held("jump"), "frame 1: jump held via Space");

    // Frame 2: Release Space
    ctrl.tick(&[RawInputEvent::KeyReleased(KeyCode::Space)]);
    let s = ctrl.get_snapshot(owner).unwrap();
    assert!(
        !s.held("jump"),
        "frame 2: jump not held after Space release"
    );
    assert!(s.released("jump"), "frame 2: jump released");

    // Frame 3: Press GamepadButton::South → jump pressed via gamepad
    ctrl.tick(&[RawInputEvent::GamepadButtonPressed(GamepadButton::South)]);
    let s = ctrl.get_snapshot(owner).unwrap();
    assert!(
        s.pressed("jump"),
        "frame 3: jump pressed via GamepadButton::South"
    );
    assert!(
        s.held("jump"),
        "frame 3: jump held via GamepadButton::South"
    );
}

// ── T-INP-36: reset_keys clears held state (focus-lost scenario) ─────────────
//
// Regression: if the window loses focus while a key is held, the OS never
// delivers `KeyReleased`, so `held` would report true forever.
// `reset_keys()` lets the window-event handler drop all held state.
#[test]
fn t_inp_36_reset_keys_clears_held_state() {
    let mut ctrl = make_gameplay_controller();
    let owner: OwnerId = 1;

    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::Space)]);
    let s = ctrl.get_snapshot(owner).unwrap();
    assert!(s.held("jump"));

    // Simulate a focus-loss: reset without receiving a KeyReleased event.
    ctrl.reset_keys();
    ctrl.tick(&[]);
    let s = ctrl.get_snapshot(owner).unwrap();
    assert!(
        !s.held("jump"),
        "jump must not be held after reset_keys()"
    );
    assert!(
        !s.pressed("jump"),
        "jump must not be pressed after reset_keys()"
    );
}

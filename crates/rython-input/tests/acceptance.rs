//! Acceptance tests for the customizable-input pipeline. Exercises the new
//! `InputMappingContext` stack on `PlayerController` end-to-end.

use rython_core::OwnerId;
use rython_input::*;
use rython_window::*;

const DT: f32 = 0.016;

// ─── Helpers ────────────────────────────────────────────────────────────────

fn make_gameplay_controller() -> PlayerController {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut ctx = InputMappingContext::new("gameplay", 0);

    ctx.add_action(InputAction::new("move_x", ValueKind::Axis1D));
    ctx.add_binding(
        "move_x",
        InputBinding::new(HardwareKey::Composite2D {
            // Composite2D is a handy way to express a paired +/- key binding;
            // we use X for move_x and ignore Y by projecting through axis().
            up: KeyCode::W,
            down: KeyCode::S,
            left: KeyCode::A,
            right: KeyCode::D,
        }),
    );

    ctx.add_action(InputAction::new("move_y", ValueKind::Axis1D));
    ctx.add_binding(
        "move_y",
        InputBinding::new(HardwareKey::Composite2D {
            up: KeyCode::W,
            down: KeyCode::S,
            left: KeyCode::A,
            right: KeyCode::D,
        })
        .with_modifier(Modifier::Swizzle(SwizzleOrder::YXZ)),
    );

    ctx.add_action(InputAction::new("jump", ValueKind::Button));
    ctx.add_binding(
        "jump",
        InputBinding::new(HardwareKey::Key(KeyCode::Space)),
    );

    ctx.add_action(InputAction::new("attack", ValueKind::Button));
    ctx.add_binding(
        "attack",
        InputBinding::new(HardwareKey::Key(KeyCode::Enter)),
    );

    ctrl.push_context(ctx);
    ctrl
}

fn drain(ctrl: &mut PlayerController) -> Vec<InputActionEvent> {
    let arc = ctrl.pending_events();
    let mut guard = arc.lock().unwrap();
    std::mem::take(&mut *guard)
}

// ─── Axis values ────────────────────────────────────────────────────────────

#[test]
fn t_inp_01_composite_axis_positive_key() {
    let mut ctrl = make_gameplay_controller();
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::D)], DT);
    let snap = ctrl.get_snapshot(1).unwrap();
    assert_eq!(snap.axis("move_x"), 1.0);
}

#[test]
fn t_inp_02_composite_axis_negative_key() {
    let mut ctrl = make_gameplay_controller();
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::A)], DT);
    let snap = ctrl.get_snapshot(1).unwrap();
    assert_eq!(snap.axis("move_x"), -1.0);
}

#[test]
fn t_inp_03_composite_axis_both_keys_cancel() {
    let mut ctrl = make_gameplay_controller();
    ctrl.tick(
        &[
            RawInputEvent::KeyPressed(KeyCode::A),
            RawInputEvent::KeyPressed(KeyCode::D),
        ],
        DT,
    );
    let snap = ctrl.get_snapshot(1).unwrap();
    assert_eq!(snap.axis("move_x"), 0.0);
}

#[test]
fn t_inp_04_no_keys_axis_zero() {
    let mut ctrl = make_gameplay_controller();
    ctrl.tick(&[], DT);
    let snap = ctrl.get_snapshot(1).unwrap();
    assert_eq!(snap.axis("move_x"), 0.0);
}

#[test]
fn t_inp_05_axis2d_composite_vector() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut ctx = InputMappingContext::new("gameplay", 0);
    ctx.add_action(InputAction::new("move", ValueKind::Axis2D));
    ctx.add_binding(
        "move",
        InputBinding::new(HardwareKey::Composite2D {
            up: KeyCode::W,
            down: KeyCode::S,
            left: KeyCode::A,
            right: KeyCode::D,
        }),
    );
    ctrl.push_context(ctx);

    ctrl.tick(
        &[
            RawInputEvent::KeyPressed(KeyCode::W),
            RawInputEvent::KeyPressed(KeyCode::D),
        ],
        DT,
    );
    let snap = ctrl.get_snapshot(owner).unwrap();
    assert_eq!(snap.axis2("move"), [1.0, 1.0]);
}

#[test]
fn t_inp_06_axis3d_composite_vector() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut ctx = InputMappingContext::new("gameplay", 0);
    ctx.add_action(InputAction::new("fly", ValueKind::Axis3D));
    ctx.add_binding(
        "fly",
        InputBinding::new(HardwareKey::Composite3D {
            up: KeyCode::E,
            down: KeyCode::Q,
            left: KeyCode::A,
            right: KeyCode::D,
            forward: KeyCode::W,
            back: KeyCode::S,
        }),
    );
    ctrl.push_context(ctx);

    ctrl.tick(
        &[
            RawInputEvent::KeyPressed(KeyCode::W),
            RawInputEvent::KeyPressed(KeyCode::E),
        ],
        DT,
    );
    let snap = ctrl.get_snapshot(owner).unwrap();
    assert_eq!(snap.axis3("fly"), [0.0, 1.0, 1.0]);
}

// ─── Button lifecycle ──────────────────────────────────────────────────────

#[test]
fn t_inp_07_button_press_hold_release_lifecycle() {
    let mut ctrl = make_gameplay_controller();

    // Frame 1: press
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::Space)], DT);
    let s = ctrl.get_snapshot(1).unwrap();
    assert!(s.pressed("jump"), "frame 1: pressed");
    assert!(s.held("jump"), "frame 1: held");
    assert!(!s.released("jump"), "frame 1: not released");

    // Frame 2: hold (still down)
    ctrl.tick(&[], DT);
    let s = ctrl.get_snapshot(1).unwrap();
    assert!(!s.pressed("jump"), "frame 2: not pressed");
    assert!(s.held("jump"), "frame 2: held");
    assert!(!s.released("jump"), "frame 2: not released");

    // Frame 3: release
    ctrl.tick(&[RawInputEvent::KeyReleased(KeyCode::Space)], DT);
    let s = ctrl.get_snapshot(1).unwrap();
    assert!(!s.pressed("jump"), "frame 3: not pressed");
    assert!(!s.held("jump"), "frame 3: not held");
    assert!(s.released("jump"), "frame 3: released");

    // Frame 4: idle
    ctrl.tick(&[], DT);
    let s = ctrl.get_snapshot(1).unwrap();
    assert!(!s.pressed("jump"), "frame 4: idle");
    assert!(!s.held("jump"), "frame 4: idle");
    assert!(!s.released("jump"), "frame 4: idle");
}

#[test]
fn t_inp_08_button_fires_started_and_completed_events() {
    let mut ctrl = make_gameplay_controller();
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::Space)], DT);
    let evs = drain(&mut ctrl);
    let jump: Vec<_> = evs.iter().filter(|e| e.action == "jump").collect();
    assert_eq!(jump.len(), 1);
    assert_eq!(jump[0].phase, EventPhase::Started);
    assert_eq!(jump[0].value, ActionValue::Button(true));

    // Still held → Triggered.
    ctrl.tick(&[], DT);
    let evs = drain(&mut ctrl);
    let jump: Vec<_> = evs.iter().filter(|e| e.action == "jump").collect();
    assert_eq!(jump.len(), 1);
    assert_eq!(jump[0].phase, EventPhase::Triggered);

    // Release → Completed.
    ctrl.tick(&[RawInputEvent::KeyReleased(KeyCode::Space)], DT);
    let evs = drain(&mut ctrl);
    let jump: Vec<_> = evs.iter().filter(|e| e.action == "jump").collect();
    assert_eq!(jump.len(), 1);
    assert_eq!(jump[0].phase, EventPhase::Completed);
}

// ─── Unbound + locking ─────────────────────────────────────────────────────

#[test]
fn t_inp_09_unbound_action_returns_default() {
    let mut ctrl = make_gameplay_controller();
    ctrl.tick(&[], DT);
    let s = ctrl.get_snapshot(1).unwrap();
    assert_eq!(s.axis("nonexistent"), 0.0);
    assert!(!s.pressed("nonexistent"));
    assert!(!s.held("nonexistent"));
    assert!(!s.released("nonexistent"));
}

#[test]
fn t_inp_10_locking_suppresses_events_and_snapshot() {
    let mut ctrl = make_gameplay_controller();
    ctrl.lock();
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::Space)], DT);
    let s = ctrl.get_snapshot(1).unwrap();
    assert!(!s.pressed("jump"));
    assert!(drain(&mut ctrl).is_empty());
}

#[test]
fn t_inp_11_unlock_restores_input() {
    let mut ctrl = make_gameplay_controller();
    ctrl.lock();
    ctrl.unlock();
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::Space)], DT);
    let s = ctrl.get_snapshot(1).unwrap();
    assert!(s.pressed("jump"));
    assert!(!drain(&mut ctrl).is_empty());
}

// ─── Ownership ─────────────────────────────────────────────────────────────

#[test]
fn t_inp_12_ownership_non_owner_rejected() {
    let mut ctrl = make_gameplay_controller();
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::Space)], DT);
    assert!(ctrl.get_snapshot(1).is_ok());
    assert!(ctrl.get_snapshot(2).is_err());
}

#[test]
fn t_inp_13_ownership_transfer() {
    let mut ctrl = make_gameplay_controller();
    ctrl.set_owner(2);
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::Space)], DT);
    assert!(ctrl.get_snapshot(2).is_ok());
    assert!(ctrl.get_snapshot(1).is_err());
}

// ─── Gamepad ───────────────────────────────────────────────────────────────

#[test]
fn t_inp_14_gamepad_axis_range() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut ctx = InputMappingContext::new("gameplay", 0);
    ctx.add_action(InputAction::new("move_x", ValueKind::Axis1D));
    ctx.add_binding(
        "move_x",
        InputBinding::new(HardwareKey::GamepadAxis(GamepadAxisType::LeftStickX)),
    );
    ctrl.push_context(ctx);

    ctrl.tick(
        &[
            RawInputEvent::GamepadConnected {
                name: "TestPad".into(),
            },
            RawInputEvent::GamepadAxisChanged {
                axis: GamepadAxisType::LeftStickX,
                value: 1.0,
            },
        ],
        DT,
    );
    assert_eq!(ctrl.get_snapshot(owner).unwrap().axis("move_x"), 1.0);

    ctrl.tick(
        &[RawInputEvent::GamepadAxisChanged {
            axis: GamepadAxisType::LeftStickX,
            value: -1.0,
        }],
        DT,
    );
    assert_eq!(ctrl.get_snapshot(owner).unwrap().axis("move_x"), -1.0);

    ctrl.tick(
        &[RawInputEvent::GamepadAxisChanged {
            axis: GamepadAxisType::LeftStickX,
            value: 0.0,
        }],
        DT,
    );
    assert_eq!(ctrl.get_snapshot(owner).unwrap().axis("move_x"), 0.0);
}

#[test]
fn t_inp_15_gamepad_stick_2d() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut ctx = InputMappingContext::new("gameplay", 0);
    ctx.add_action(InputAction::new("move", ValueKind::Axis2D));
    ctx.add_binding(
        "move",
        InputBinding::new(HardwareKey::GamepadStick {
            x_axis: GamepadAxisType::LeftStickX,
            y_axis: GamepadAxisType::LeftStickY,
        }),
    );
    ctrl.push_context(ctx);

    ctrl.tick(
        &[
            RawInputEvent::GamepadConnected {
                name: "Pad".into(),
            },
            RawInputEvent::GamepadAxisChanged {
                axis: GamepadAxisType::LeftStickX,
                value: 0.6,
            },
            RawInputEvent::GamepadAxisChanged {
                axis: GamepadAxisType::LeftStickY,
                value: -0.4,
            },
        ],
        DT,
    );
    let v = ctrl.get_snapshot(owner).unwrap().axis2("move");
    assert!((v[0] - 0.6).abs() < 1e-5);
    assert!((v[1] - -0.4).abs() < 1e-5);
}

#[test]
fn t_inp_16_gamepad_button_lifecycle() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut ctx = InputMappingContext::new("gameplay", 0);
    ctx.add_action(InputAction::new("jump", ValueKind::Button));
    ctx.add_binding(
        "jump",
        InputBinding::new(HardwareKey::Gamepad(GamepadButton::South)),
    );
    ctrl.push_context(ctx);

    ctrl.tick(
        &[
            RawInputEvent::GamepadConnected {
                name: "Pad".into(),
            },
            RawInputEvent::GamepadButtonPressed(GamepadButton::South),
        ],
        DT,
    );
    let s = ctrl.get_snapshot(owner).unwrap();
    assert!(s.pressed("jump"));
    assert!(s.held("jump"));
    assert!(!s.released("jump"));
}

#[test]
fn t_inp_17_gamepad_disconnected_clears_state() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut ctx = InputMappingContext::new("gameplay", 0);
    ctx.add_action(InputAction::new("jump", ValueKind::Button));
    ctx.add_binding(
        "jump",
        InputBinding::new(HardwareKey::Gamepad(GamepadButton::South)),
    );
    ctrl.push_context(ctx);

    ctrl.tick(
        &[
            RawInputEvent::GamepadConnected {
                name: "Pad".into(),
            },
            RawInputEvent::GamepadButtonPressed(GamepadButton::South),
        ],
        DT,
    );
    ctrl.tick(&[RawInputEvent::GamepadDisconnected], DT);
    // After disconnect + one frame of idle, the button should be released.
    ctrl.tick(&[], DT);
    let s = ctrl.get_snapshot(owner).unwrap();
    assert!(!s.held("jump"));
    assert!(ctrl.gamepad_info().is_none());
}

// ─── Mouse ─────────────────────────────────────────────────────────────────

#[test]
fn t_inp_18_mouse_button_binding_lifecycle() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut ctx = InputMappingContext::new("gameplay", 0);
    ctx.add_action(InputAction::new("fire", ValueKind::Button));
    ctx.add_binding(
        "fire",
        InputBinding::new(HardwareKey::Mouse(MouseButton::Left)),
    );
    ctrl.push_context(ctx);

    ctrl.tick(
        &[RawInputEvent::MouseButtonPressed(MouseButton::Left)],
        DT,
    );
    assert!(ctrl.get_snapshot(owner).unwrap().pressed("fire"));
    ctrl.tick(
        &[RawInputEvent::MouseButtonReleased(MouseButton::Left)],
        DT,
    );
    assert!(ctrl.get_snapshot(owner).unwrap().released("fire"));
}

#[test]
fn t_inp_19_mouse_axis_delta() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut ctx = InputMappingContext::new("gameplay", 0);
    ctx.add_action(InputAction::new("look_x", ValueKind::Axis1D));
    ctx.add_binding(
        "look_x",
        InputBinding::new(HardwareKey::MouseAxis(MouseAxisType::X)),
    );
    ctrl.push_context(ctx);

    ctrl.tick(
        &[
            RawInputEvent::MouseMoved { dx: 3.0, dy: 0.0 },
            RawInputEvent::MouseMoved { dx: 2.0, dy: 0.0 },
        ],
        DT,
    );
    assert_eq!(ctrl.get_snapshot(owner).unwrap().axis("look_x"), 5.0);
}

#[test]
fn t_inp_20_mouse_delta_resets_each_tick() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut ctx = InputMappingContext::new("gameplay", 0);
    ctx.add_action(InputAction::new("look_x", ValueKind::Axis1D));
    ctx.add_binding(
        "look_x",
        InputBinding::new(HardwareKey::MouseAxis(MouseAxisType::X)),
    );
    ctrl.push_context(ctx);

    ctrl.tick(&[RawInputEvent::MouseMoved { dx: 3.0, dy: 0.0 }], DT);
    assert_eq!(ctrl.get_snapshot(owner).unwrap().axis("look_x"), 3.0);

    ctrl.tick(&[], DT);
    assert_eq!(ctrl.get_snapshot(owner).unwrap().axis("look_x"), 0.0);
}

// ─── Modifiers ─────────────────────────────────────────────────────────────

#[test]
fn t_inp_21_deadzone_modifier_on_gamepad_axis() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut ctx = InputMappingContext::new("gameplay", 0);
    ctx.add_action(InputAction::new("move_x", ValueKind::Axis1D));
    ctx.add_binding(
        "move_x",
        InputBinding::new(HardwareKey::GamepadAxis(GamepadAxisType::LeftStickX))
            .with_modifier(Modifier::DeadZone {
                lower: 0.2,
                upper: 1.0,
                radial: false,
            }),
    );
    ctrl.push_context(ctx);

    // Below deadzone → zero.
    ctrl.tick(
        &[RawInputEvent::GamepadAxisChanged {
            axis: GamepadAxisType::LeftStickX,
            value: 0.1,
        }],
        DT,
    );
    assert_eq!(ctrl.get_snapshot(owner).unwrap().axis("move_x"), 0.0);
}

#[test]
fn t_inp_22_scale_modifier_multiplies_sample() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut ctx = InputMappingContext::new("gameplay", 0);
    ctx.add_action(InputAction::new("throttle", ValueKind::Axis1D));
    ctx.add_binding(
        "throttle",
        InputBinding::new(HardwareKey::GamepadAxis(GamepadAxisType::LeftStickX))
            .with_modifier(Modifier::Scale([2.0, 1.0, 1.0])),
    );
    ctrl.push_context(ctx);

    ctrl.tick(
        &[RawInputEvent::GamepadAxisChanged {
            axis: GamepadAxisType::LeftStickX,
            value: 0.5,
        }],
        DT,
    );
    assert_eq!(ctrl.get_snapshot(owner).unwrap().axis("throttle"), 1.0);
}

// ─── Triggers ──────────────────────────────────────────────────────────────

#[test]
fn t_inp_23_pressed_trigger_rising_edge_only() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut ctx = InputMappingContext::new("gameplay", 0);
    ctx.add_action(InputAction::new("jump", ValueKind::Button));
    ctx.add_binding(
        "jump",
        InputBinding::new(HardwareKey::Key(KeyCode::Space)).with_trigger(Trigger::pressed()),
    );
    ctrl.push_context(ctx);

    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::Space)], DT);
    let evs = drain(&mut ctrl);
    assert_eq!(evs.len(), 1);
    assert_eq!(evs[0].phase, EventPhase::Started);

    // Held frame: Pressed trigger doesn't fire; action is in-progress with no state.
    ctrl.tick(&[], DT);
    let evs = drain(&mut ctrl);
    // Pressed returns None while held, so binding state is None → previous was
    // Started, so we emit Completed.
    assert_eq!(evs.len(), 1);
    assert_eq!(evs[0].phase, EventPhase::Completed);
}

#[test]
fn t_inp_24_hold_trigger_fires_after_threshold() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut ctx = InputMappingContext::new("gameplay", 0);
    ctx.add_action(InputAction::new("charge", ValueKind::Button));
    ctx.add_binding(
        "charge",
        InputBinding::new(HardwareKey::Key(KeyCode::F)).with_trigger(Trigger::hold(0.3)),
    );
    ctrl.push_context(ctx);

    // Press F — Ongoing only.
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::F)], 0.1);
    let evs = drain(&mut ctrl);
    assert_eq!(evs.last().map(|e| e.phase), Some(EventPhase::Started));

    ctrl.tick(&[], 0.1);
    let evs = drain(&mut ctrl);
    assert_eq!(evs.last().map(|e| e.phase), Some(EventPhase::Ongoing));

    // Total 0.3s → Triggered.
    ctrl.tick(&[], 0.1);
    let evs = drain(&mut ctrl);
    assert!(evs
        .iter()
        .any(|e| e.action == "charge" && e.phase == EventPhase::Triggered));
}

#[test]
fn t_inp_25_tap_trigger_canceled_on_long_hold() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut ctx = InputMappingContext::new("gameplay", 0);
    ctx.add_action(InputAction::new("tap", ValueKind::Button));
    ctx.add_binding(
        "tap",
        InputBinding::new(HardwareKey::Key(KeyCode::T)).with_trigger(Trigger::tap(0.2)),
    );
    ctrl.push_context(ctx);

    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::T)], 0.1);
    drain(&mut ctrl);
    ctrl.tick(&[], 0.1);
    drain(&mut ctrl);
    ctrl.tick(&[], 0.1); // now elapsed > 0.2 → Canceled
    let evs = drain(&mut ctrl);
    assert!(evs
        .iter()
        .any(|e| e.action == "tap" && e.phase == EventPhase::Canceled));
}

// ─── Multiple contexts + priority + shadowing ──────────────────────────────

#[test]
fn t_inp_26_higher_priority_shadows_lower() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);

    let mut gameplay = InputMappingContext::new("gameplay", 0);
    gameplay.add_action(InputAction::new("jump", ValueKind::Button));
    gameplay.add_binding(
        "jump",
        InputBinding::new(HardwareKey::Key(KeyCode::Space)),
    );
    ctrl.push_context(gameplay);

    let mut menu = InputMappingContext::new("menu", 100);
    menu.add_action(InputAction::new("jump", ValueKind::Button));
    menu.add_binding(
        "jump",
        InputBinding::new(HardwareKey::Key(KeyCode::Space)),
    );
    ctrl.push_context(menu);

    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::Space)], DT);
    let evs = drain(&mut ctrl);
    // Only one jump event — from the higher-priority context.
    let jump_evs: Vec<_> = evs.iter().filter(|e| e.action == "jump").collect();
    assert_eq!(jump_evs.len(), 1);
    assert_eq!(ctrl.active_contexts(), vec!["menu", "gameplay"]);
}

#[test]
fn t_inp_27_pop_context_restores_lower() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);

    let mut gameplay = InputMappingContext::new("gameplay", 0);
    gameplay.add_action(InputAction::new("jump", ValueKind::Button));
    gameplay.add_binding(
        "jump",
        InputBinding::new(HardwareKey::Key(KeyCode::Space)),
    );
    ctrl.push_context(gameplay);

    let mut menu = InputMappingContext::new("menu", 100);
    menu.add_action(InputAction::new("jump", ValueKind::Button));
    menu.add_binding(
        "jump",
        InputBinding::new(HardwareKey::Key(KeyCode::Space)),
    );
    ctrl.push_context(menu);

    assert!(ctrl.pop_context("menu").is_some());
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::Space)], DT);
    assert!(ctrl.get_snapshot(owner).unwrap().pressed("jump"));
}

#[test]
fn t_inp_28_clear_contexts_empties_stack() {
    let mut ctrl = make_gameplay_controller();
    ctrl.clear_contexts();
    assert!(ctrl.active_contexts().is_empty());
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::Space)], DT);
    assert!(!ctrl.get_snapshot(1).unwrap().pressed("jump"));
}

// ─── Multiple bindings per action ──────────────────────────────────────────

#[test]
fn t_inp_29_multiple_bindings_accumulate() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut ctx = InputMappingContext::new("gameplay", 0);
    ctx.add_action(InputAction::new("jump", ValueKind::Button));
    ctx.add_binding(
        "jump",
        InputBinding::new(HardwareKey::Key(KeyCode::Space)),
    );
    ctx.add_binding(
        "jump",
        InputBinding::new(HardwareKey::Key(KeyCode::Enter)),
    );
    ctrl.push_context(ctx);

    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::Enter)], DT);
    assert!(ctrl.get_snapshot(owner).unwrap().pressed("jump"));
}

// ─── reset_keys ────────────────────────────────────────────────────────────

#[test]
fn t_inp_30_reset_keys_clears_held_state() {
    let mut ctrl = make_gameplay_controller();
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::Space)], DT);
    assert!(ctrl.get_snapshot(1).unwrap().held("jump"));
    ctrl.reset_keys();
    ctrl.tick(&[], DT);
    assert!(!ctrl.get_snapshot(1).unwrap().held("jump"));
}

#[test]
fn t_inp_31_event_elapsed_counts_up_during_ongoing() {
    let owner: OwnerId = 1;
    let mut ctrl = PlayerController::new(owner);
    let mut ctx = InputMappingContext::new("gameplay", 0);
    ctx.add_action(InputAction::new("charge", ValueKind::Button));
    ctx.add_binding(
        "charge",
        InputBinding::new(HardwareKey::Key(KeyCode::F)).with_trigger(Trigger::hold(10.0)),
    );
    ctrl.push_context(ctx);

    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::F)], 0.5);
    ctrl.tick(&[], 0.5);
    ctrl.tick(&[], 0.5);
    let evs = drain(&mut ctrl);
    let last = evs
        .iter()
        .rev()
        .find(|e| e.action == "charge")
        .expect("event");
    // After Started (elapsed=0) we spend two Ongoing frames accumulating dt.
    assert!(
        last.elapsed_seconds >= 1.0,
        "elapsed should be at least 1s after 3 frames of 0.5s hold, got {}",
        last.elapsed_seconds
    );
}

// ─── Context rebind support ────────────────────────────────────────────────

#[test]
fn t_inp_32_binding_mut_supports_runtime_rebind() {
    let mut ctrl = make_gameplay_controller();
    // Re-point jump from Space → Enter.
    {
        let ctx = ctrl.context_mut("gameplay").expect("gameplay context exists");
        let binding = ctx.binding_mut("jump", 0).expect("jump binding 0");
        binding.key = HardwareKey::Key(KeyCode::Enter);
    }
    ctrl.tick(&[RawInputEvent::KeyPressed(KeyCode::Enter)], DT);
    assert!(ctrl.get_snapshot(1).unwrap().pressed("jump"));
    // Release Enter, then press Space — after rebind, Space is unbound for jump.
    ctrl.tick(
        &[
            RawInputEvent::KeyReleased(KeyCode::Enter),
            RawInputEvent::KeyPressed(KeyCode::Space),
        ],
        DT,
    );
    ctrl.tick(&[], DT);
    assert!(!ctrl.get_snapshot(1).unwrap().held("jump"));
}

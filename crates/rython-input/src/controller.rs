use crate::{AxisBinding, ButtonBinding, InputActionEvent, InputMap, InputSnapshot};
use rython_core::{EngineError, OwnerId, SchedulerHandle};
use rython_modules::Module;
use rython_window::{GamepadAxisType, GamepadButton, KeyCode, MouseAxisType, MouseButton, RawInputEvent};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

// ─── Free functions (avoid borrow-checker conflicts inside tick) ─────────────

fn eval_axis(
    binding: &AxisBinding,
    keys: &HashSet<KeyCode>,
    mouse_delta: &(f64, f64),
    gpad_axes: &HashMap<GamepadAxisType, f32>,
) -> f32 {
    match binding {
        AxisBinding::KBAxis { negative, positive } => {
            let neg = if keys.contains(negative) { -1.0_f32 } else { 0.0 };
            let pos = if keys.contains(positive) { 1.0_f32 } else { 0.0 };
            (neg + pos).clamp(-1.0, 1.0)
        }
        AxisBinding::MouseAxis { axis } => match axis {
            MouseAxisType::X => mouse_delta.0 as f32,
            MouseAxisType::Y => mouse_delta.1 as f32,
        },
        AxisBinding::GamepadAxis { axis } => gpad_axes.get(axis).copied().unwrap_or(0.0),
    }
}

fn is_btn_active(
    binding: &ButtonBinding,
    keys: &HashSet<KeyCode>,
    mouse_buttons: &HashSet<MouseButton>,
    gpad_buttons: &HashSet<GamepadButton>,
) -> bool {
    match binding {
        ButtonBinding::Keyboard(key) => keys.contains(key),
        ButtonBinding::Mouse(btn) => mouse_buttons.contains(btn),
        ButtonBinding::Gamepad(btn) => gpad_buttons.contains(btn),
    }
}

const AXIS_DEADZONE: f32 = 0.1;

// ─── PlayerController ────────────────────────────────────────────────────────

/// Module that processes raw input events each frame and produces an InputSnapshot.
/// This is an exclusive module — only the current owner may read or reconfigure it.
pub struct PlayerController {
    maps: HashMap<String, InputMap>,
    active_map: Option<String>,
    locked: bool,
    owner: OwnerId,

    // Per-frame hardware state
    current_keys: HashSet<KeyCode>,
    previous_keys: HashSet<KeyCode>,
    current_mouse_buttons: HashSet<MouseButton>,
    previous_mouse_buttons: HashSet<MouseButton>,
    mouse_delta: (f64, f64),
    current_gamepad_buttons: HashSet<GamepadButton>,
    previous_gamepad_buttons: HashSet<GamepadButton>,
    gamepad_axes: HashMap<GamepadAxisType, f32>,
    gamepad_connected: bool,
    gamepad_name: Option<String>,

    // Output
    snapshot: InputSnapshot,
    /// Pending input action events; callers may drain this after tick().
    pending_events: Arc<Mutex<Vec<InputActionEvent>>>,
    /// Tracks the previous quantized axis value per action for change detection.
    previous_axis_values: HashMap<String, f32>,
}

impl PlayerController {
    pub fn new(owner: OwnerId) -> Self {
        Self {
            maps: HashMap::new(),
            active_map: None,
            locked: false,
            owner,
            current_keys: HashSet::new(),
            previous_keys: HashSet::new(),
            current_mouse_buttons: HashSet::new(),
            previous_mouse_buttons: HashSet::new(),
            mouse_delta: (0.0, 0.0),
            current_gamepad_buttons: HashSet::new(),
            previous_gamepad_buttons: HashSet::new(),
            gamepad_axes: HashMap::new(),
            gamepad_connected: false,
            gamepad_name: None,
            snapshot: InputSnapshot::new(),
            pending_events: Arc::new(Mutex::new(Vec::new())),
            previous_axis_values: HashMap::new(),
        }
    }

    /// Register a map. The first registered map becomes active automatically.
    pub fn register_map(&mut self, map: InputMap) {
        let name = map.name().to_owned();
        if self.active_map.is_none() {
            self.active_map = Some(name.clone());
        }
        self.maps.insert(name, map);
    }

    pub fn set_active_map(&mut self, name: &str, caller: OwnerId) -> Result<(), EngineError> {
        if caller != self.owner {
            return Err(EngineError::Module {
                module: "PlayerController".into(),
                message: "caller is not the owner".into(),
            });
        }
        if self.maps.contains_key(name) {
            self.active_map = Some(name.to_owned());
            Ok(())
        } else {
            Err(EngineError::Module {
                module: "PlayerController".into(),
                message: format!("map '{}' not found", name),
            })
        }
    }

    pub fn lock(&mut self) {
        self.locked = true;
    }

    pub fn unlock(&mut self) {
        self.locked = false;
    }

    pub fn is_locked(&self) -> bool {
        self.locked
    }

    pub fn owner(&self) -> OwnerId {
        self.owner
    }

    /// Transfer ownership. Only the current owner may call this; subsequent
    /// calls from the old owner will be rejected.
    pub fn set_owner(&mut self, new_owner: OwnerId) {
        self.owner = new_owner;
    }

    /// Returns the current snapshot if the caller is the owner.
    pub fn get_snapshot(&self, caller: OwnerId) -> Result<&InputSnapshot, EngineError> {
        if caller != self.owner {
            return Err(EngineError::Module {
                module: "PlayerController".into(),
                message: "caller is not the owner".into(),
            });
        }
        Ok(&self.snapshot)
    }

    /// Shared handle to the pending-events queue. Callers may drain or observe it.
    pub fn pending_events(&self) -> Arc<Mutex<Vec<InputActionEvent>>> {
        Arc::clone(&self.pending_events)
    }

    pub fn active_backend(&self) -> &str {
        if self.gamepad_connected { "gamepad" } else { "keyboard_mouse" }
    }

    pub fn gamepad_info(&self) -> Option<&str> {
        self.gamepad_name.as_deref()
    }

    /// Process raw input events for one frame and rebuild the snapshot.
    ///
    /// Call this once per frame (or directly in tests) before reading the snapshot.
    pub fn tick(&mut self, events: &[RawInputEvent]) {
        // ── Step 1: save previous state ──────────────────────────────────────
        self.previous_keys = self.current_keys.clone();
        self.previous_mouse_buttons = self.current_mouse_buttons.clone();
        self.previous_gamepad_buttons = self.current_gamepad_buttons.clone();
        self.mouse_delta = (0.0, 0.0);

        // ── Step 2: apply raw events ──────────────────────────────────────────
        for event in events {
            match event {
                RawInputEvent::KeyPressed(key) => { self.current_keys.insert(*key); }
                RawInputEvent::KeyReleased(key) => { self.current_keys.remove(key); }
                RawInputEvent::MouseMoved { dx, dy } => {
                    self.mouse_delta.0 += dx;
                    self.mouse_delta.1 += dy;
                }
                RawInputEvent::MouseButtonPressed(btn) => { self.current_mouse_buttons.insert(*btn); }
                RawInputEvent::MouseButtonReleased(btn) => { self.current_mouse_buttons.remove(btn); }
                RawInputEvent::GamepadButtonPressed(btn) => { self.current_gamepad_buttons.insert(*btn); }
                RawInputEvent::GamepadButtonReleased(btn) => { self.current_gamepad_buttons.remove(btn); }
                RawInputEvent::GamepadAxisChanged { axis, value } => {
                    self.gamepad_axes.insert(*axis, *value);
                }
                RawInputEvent::GamepadConnected { name } => {
                    self.gamepad_connected = true;
                    self.gamepad_name = Some(name.clone());
                }
                RawInputEvent::GamepadDisconnected => {
                    self.gamepad_connected = false;
                    self.gamepad_name = None;
                    self.gamepad_axes.clear();
                    self.current_gamepad_buttons.clear();
                }
            }
        }

        // ── Step 3: build snapshot ────────────────────────────────────────────
        // Take local copies of raw state refs so we can borrow maps separately.
        let cur_keys = &self.current_keys;
        let prev_keys = &self.previous_keys;
        let cur_mouse = &self.current_mouse_buttons;
        let prev_mouse = &self.previous_mouse_buttons;
        let cur_gpad = &self.current_gamepad_buttons;
        let prev_gpad = &self.previous_gamepad_buttons;
        let mouse_delta = &self.mouse_delta;
        let gpad_axes = &self.gamepad_axes;
        let locked = self.locked;

        let mut new_snapshot = InputSnapshot::new();
        let mut new_events: Vec<InputActionEvent> = Vec::new();

        let map_name = self.active_map.clone();
        if let Some(ref map_name) = map_name {
            if let Some(map) = self.maps.get(map_name) {
                // Collect keys to avoid iterator-borrow conflicts with future writes
                let axis_actions: Vec<String> = map.all_axis_actions().cloned().collect();
                let button_actions: Vec<String> = map.all_button_actions().cloned().collect();

                if !locked {
                    for action in &axis_actions {
                        let mut value = 0.0_f32;
                        for binding in map.axis_bindings(action) {
                            let v = eval_axis(binding, cur_keys, mouse_delta, gpad_axes);
                            if v.abs() > value.abs() {
                                value = v;
                            }
                        }
                        new_snapshot.set_axis(action.clone(), value);

                        // Emit an axis-change event when the value meaningfully
                        // crosses the deadzone boundary or changes while active.
                        let prev = self.previous_axis_values.get(action.as_str()).copied().unwrap_or(0.0);
                        let prev_active = prev.abs() > AXIS_DEADZONE;
                        let curr_active = value.abs() > AXIS_DEADZONE;
                        let deadzone_crossed = prev_active != curr_active;
                        let significant_change =
                            prev_active && curr_active && (value - prev).abs() > AXIS_DEADZONE;
                        if deadzone_crossed || significant_change {
                            new_events.push(InputActionEvent {
                                action: format!("axis:{action}"),
                                value,
                            });
                        }
                        self.previous_axis_values.insert(action.clone(), value);
                    }

                    for action in &button_actions {
                        let currently = map
                            .button_bindings(action)
                            .iter()
                            .any(|b| is_btn_active(b, cur_keys, cur_mouse, cur_gpad));
                        let previously = map
                            .button_bindings(action)
                            .iter()
                            .any(|b| is_btn_active(b, prev_keys, prev_mouse, prev_gpad));

                        let pressed = currently && !previously;
                        let held = currently;
                        let released = !currently && previously;

                        new_snapshot.set_button(action.clone(), pressed, held, released);

                        if pressed {
                            new_events.push(InputActionEvent { action: action.clone(), value: 1.0 });
                        } else if released {
                            new_events.push(InputActionEvent { action: action.clone(), value: 0.0 });
                        }
                    }
                }
            }
        }

        // map borrow ends here; safe to mutate self
        self.snapshot = new_snapshot;
        if !locked {
            self.pending_events.lock().unwrap().extend(new_events);
        }
    }
}

impl Module for PlayerController {
    fn name(&self) -> &str {
        "PlayerController"
    }

    fn dependencies(&self) -> Vec<String> {
        vec!["WindowModule".to_string()]
    }

    fn on_load(&mut self, _scheduler: &dyn SchedulerHandle) -> Result<(), EngineError> {
        // The engine entrypoint is responsible for calling tick() each frame.
        Ok(())
    }

    fn on_unload(&mut self, _scheduler: &dyn SchedulerHandle) -> Result<(), EngineError> {
        Ok(())
    }

    fn is_exclusive(&self) -> bool {
        true
    }
}

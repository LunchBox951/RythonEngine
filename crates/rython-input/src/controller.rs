use crate::binding::InputSource;
use crate::bitset::{GamepadButtonSet, KeyCodeSet, MouseButtonSet};
use crate::context::{ActionEvaluation, InputMappingContext};
use crate::events::{EventPhase, InputActionEvent};
use crate::snapshot::InputSnapshot;
use crate::trigger::TriggerState;
use crate::value::{ActionValue, ValueKind};
use rython_core::{EngineError, OwnerId, SchedulerHandle};
use rython_modules::Module;
use rython_window::{GamepadAxisType, RawInputEvent};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Module that processes raw input events each frame, drives the active
/// `InputMappingContext` stack, and produces an `InputSnapshot` + a stream
/// of `InputActionEvent`s.
///
/// Exclusive module — only the current owner may read or reconfigure it.
pub struct PlayerController {
    contexts: Vec<InputMappingContext>,
    per_action_state: HashMap<(String, String), ActionRuntime>,
    locked: bool,
    owner: OwnerId,

    current_keys: KeyCodeSet,
    current_mouse_buttons: MouseButtonSet,
    mouse_delta: (f32, f32),
    current_gamepad_buttons: GamepadButtonSet,
    gamepad_axes: HashMap<GamepadAxisType, f32>,
    gamepad_connected: bool,
    gamepad_name: Option<String>,

    snapshot: InputSnapshot,
    pending_events: Arc<Mutex<Vec<InputActionEvent>>>,
}

#[derive(Debug, Default, Clone)]
struct ActionRuntime {
    previous_phase: Option<EventPhase>,
    elapsed_seconds: f32,
    previous_actuated: bool,
}

impl PlayerController {
    pub fn new(owner: OwnerId) -> Self {
        Self {
            contexts: Vec::new(),
            per_action_state: HashMap::new(),
            locked: false,
            owner,
            current_keys: KeyCodeSet::new(),
            current_mouse_buttons: MouseButtonSet::new(),
            mouse_delta: (0.0, 0.0),
            current_gamepad_buttons: GamepadButtonSet::new(),
            gamepad_axes: HashMap::new(),
            gamepad_connected: false,
            gamepad_name: None,
            snapshot: InputSnapshot::new(),
            pending_events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Push a context onto the stack. Higher-priority contexts evaluate
    /// first; equal-priority contexts evaluate in push order.
    pub fn push_context(&mut self, mut context: InputMappingContext) {
        context.reset_trigger_state();
        // Drop any lingering per-action runtime for this context id so its
        // first tick after re-push emits Started correctly.
        self.per_action_state
            .retain(|(ctx_id, _), _| ctx_id != context.id());
        self.contexts.push(context);
        self.contexts
            .sort_by(|a, b| b.priority().cmp(&a.priority()));
    }

    /// Remove the context with this id, if present. Returns the removed context.
    pub fn pop_context(&mut self, id: &str) -> Option<InputMappingContext> {
        let idx = self.contexts.iter().position(|c| c.id() == id)?;
        let removed = self.contexts.remove(idx);
        self.per_action_state
            .retain(|(ctx_id, _), _| ctx_id != id);
        Some(removed)
    }

    pub fn clear_contexts(&mut self) {
        self.contexts.clear();
        self.per_action_state.clear();
    }

    /// Context ids in priority (descending) order.
    pub fn active_contexts(&self) -> Vec<String> {
        self.contexts.iter().map(|c| c.id().to_owned()).collect()
    }

    /// Mutable handle to a live context by id (for rebind workflows).
    pub fn context_mut(&mut self, id: &str) -> Option<&mut InputMappingContext> {
        self.contexts.iter_mut().find(|c| c.id() == id)
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

    pub fn set_owner(&mut self, new_owner: OwnerId) {
        self.owner = new_owner;
    }

    pub fn get_snapshot(&self, caller: OwnerId) -> Result<&InputSnapshot, EngineError> {
        if caller != self.owner {
            return Err(EngineError::Module {
                module: "PlayerController".into(),
                message: "caller is not the owner".into(),
            });
        }
        Ok(&self.snapshot)
    }

    pub fn pending_events(&self) -> Arc<Mutex<Vec<InputActionEvent>>> {
        Arc::clone(&self.pending_events)
    }

    pub fn active_backend(&self) -> &str {
        if self.gamepad_connected {
            "gamepad"
        } else {
            "keyboard_mouse"
        }
    }

    pub fn gamepad_info(&self) -> Option<&str> {
        self.gamepad_name.as_deref()
    }

    /// Process raw input events for one frame, advance context state by `dt`
    /// seconds, rebuild the snapshot, and queue action events.
    pub fn tick(&mut self, events: &[RawInputEvent], dt: f32) {
        self.mouse_delta = (0.0, 0.0);

        for event in events {
            match event {
                RawInputEvent::KeyPressed(key) => self.current_keys.insert(*key),
                RawInputEvent::KeyReleased(key) => self.current_keys.remove(*key),
                RawInputEvent::MouseMoved { dx, dy } => {
                    self.mouse_delta.0 += *dx as f32;
                    self.mouse_delta.1 += *dy as f32;
                }
                RawInputEvent::MouseButtonPressed(btn) => self.current_mouse_buttons.insert(*btn),
                RawInputEvent::MouseButtonReleased(btn) => self.current_mouse_buttons.remove(*btn),
                RawInputEvent::GamepadButtonPressed(btn) => {
                    self.current_gamepad_buttons.insert(*btn)
                }
                RawInputEvent::GamepadButtonReleased(btn) => {
                    self.current_gamepad_buttons.remove(*btn)
                }
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

        let mut new_snapshot = InputSnapshot::new();
        let mut new_events: Vec<InputActionEvent> = Vec::new();

        if !self.locked {
            let src = InputSource {
                keys: &self.current_keys,
                mouse_buttons: &self.current_mouse_buttons,
                mouse_delta: self.mouse_delta,
                gamepad_buttons: &self.current_gamepad_buttons,
                gamepad_axes: &self.gamepad_axes,
            };

            // Actions consumed by higher-priority contexts are masked out for
            // all lower contexts (shadowing).
            let mut consumed: std::collections::HashSet<String> =
                std::collections::HashSet::new();

            // Borrow splitting: contexts are &mut self.contexts, but we also
            // need to mutate per_action_state. Iterate by index to keep both.
            let mut ctx_idx = 0;
            while ctx_idx < self.contexts.len() {
                // Shortlived borrow for evaluation.
                let evaluations = {
                    let context = &mut self.contexts[ctx_idx];
                    context.evaluate(&src, dt)
                };

                let context_id = self.contexts[ctx_idx].id().to_owned();

                for eval in evaluations {
                    if consumed.contains(&eval.action_id) {
                        continue;
                    }
                    let key = (context_id.clone(), eval.action_id.clone());
                    let runtime = self
                        .per_action_state
                        .entry(key.clone())
                        .or_default();

                    let phase = derive_phase(runtime, &eval);
                    if eval.value.is_actuated() {
                        consumed.insert(eval.action_id.clone());
                    }

                    // Derive the previous_actuated flag for the next tick.
                    runtime.previous_actuated = eval.value.is_actuated()
                        || matches!(eval.state, TriggerState::Ongoing);

                    // Update elapsed timer based on phase.
                    match phase {
                        Some(EventPhase::Started) => runtime.elapsed_seconds = 0.0,
                        Some(EventPhase::Ongoing) | Some(EventPhase::Triggered) => {
                            runtime.elapsed_seconds += dt;
                        }
                        Some(EventPhase::Completed) | Some(EventPhase::Canceled) => {
                            runtime.elapsed_seconds = 0.0;
                        }
                        None => {}
                    }

                    runtime.previous_phase = phase;

                    if let Some(phase) = phase {
                        new_events.push(InputActionEvent {
                            action: eval.action_id.clone(),
                            value: eval.value,
                            phase,
                            elapsed_seconds: runtime.elapsed_seconds,
                        });
                    }

                    // Publish to snapshot only once per action id (from highest-
                    // priority winner). Lower contexts are ignored for polling
                    // once the action is consumed — matching the event shadow.
                    publish_to_snapshot(&mut new_snapshot, &eval, phase);
                }

                ctx_idx += 1;
            }

            // Buttons that have no current active context should still be
            // unregistered in the snapshot (so polling returns defaults).
        }

        self.snapshot = new_snapshot;
        if !self.locked {
            let mut guard = match self.pending_events.lock() {
                Ok(g) => g,
                Err(poison) => poison.into_inner(),
            };
            guard.extend(new_events);
        }
    }

    /// Clear all held hardware state. Call on window focus loss so stale
    /// "held" keys don't keep triggering actions after alt-tab.
    pub fn reset_keys(&mut self) {
        self.current_keys = KeyCodeSet::new();
        self.current_mouse_buttons = MouseButtonSet::new();
        self.current_gamepad_buttons = GamepadButtonSet::new();
        self.gamepad_axes.clear();
        for context in &mut self.contexts {
            context.reset_trigger_state();
        }
        self.per_action_state.clear();
    }
}

/// Derive an `EventPhase` (or `None` for "nothing to emit") from the current
/// frame's evaluation against the previous frame's runtime state.
fn derive_phase(runtime: &ActionRuntime, eval: &ActionEvaluation) -> Option<EventPhase> {
    let was_in_progress = matches!(
        runtime.previous_phase,
        Some(EventPhase::Started) | Some(EventPhase::Ongoing) | Some(EventPhase::Triggered)
    );
    match eval.state {
        TriggerState::None => {
            // Was the action ongoing/triggered last frame? Then it just ended.
            if was_in_progress {
                Some(EventPhase::Completed)
            } else {
                None
            }
        }
        TriggerState::Canceled => Some(EventPhase::Canceled),
        TriggerState::Ongoing => {
            if was_in_progress {
                Some(EventPhase::Ongoing)
            } else {
                Some(EventPhase::Started)
            }
        }
        TriggerState::Triggered => {
            if was_in_progress {
                Some(EventPhase::Triggered)
            } else {
                // First time actuating without a prior Ongoing phase: emit
                // Started and Triggered on the same frame. Callers see one
                // event here (Triggered) because we can only return a single
                // phase; Started is implicit.
                Some(EventPhase::Started)
            }
        }
    }
}

fn publish_to_snapshot(
    snap: &mut InputSnapshot,
    eval: &ActionEvaluation,
    phase: Option<EventPhase>,
) {
    let id = eval.action_id.clone();
    match eval.value {
        ActionValue::Axis1D(v) => {
            snap.set_axis(id.clone(), v);
        }
        ActionValue::Axis2D(v) => {
            snap.set_axis2(id.clone(), v);
        }
        ActionValue::Axis3D(v) => {
            snap.set_axis3(id.clone(), v);
        }
        ActionValue::Button(_) => {}
    }

    if matches!(eval.kind, ValueKind::Button) {
        let pressed = matches!(phase, Some(EventPhase::Started));
        let held = eval.value.is_actuated()
            || matches!(phase, Some(EventPhase::Ongoing) | Some(EventPhase::Triggered));
        let released = matches!(phase, Some(EventPhase::Completed) | Some(EventPhase::Canceled));
        snap.set_button(id.clone(), pressed, held, released);
    }
    snap.set_value(id, eval.value);
}

impl Module for PlayerController {
    fn name(&self) -> &str {
        "PlayerController"
    }

    fn dependencies(&self) -> Vec<String> {
        vec!["WindowModule".to_string()]
    }

    fn on_load(&mut self, _scheduler: &dyn SchedulerHandle) -> Result<(), EngineError> {
        Ok(())
    }

    fn on_unload(&mut self, _scheduler: &dyn SchedulerHandle) -> Result<(), EngineError> {
        Ok(())
    }

    fn is_exclusive(&self) -> bool {
        true
    }
}

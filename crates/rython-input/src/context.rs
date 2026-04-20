//! An input mapping context — Unreal's "Input Mapping Context" ported to
//! RythonEngine: a prioritized bundle of actions + their bindings, pushed
//! onto the `PlayerController`'s context stack at runtime.

use crate::action::InputAction;
use crate::binding::{InputBinding, InputSource};
use crate::trigger::{TriggerCtx, TriggerState};
use crate::value::{ActionValue, ValueKind};
use std::collections::HashMap;

/// Evaluated result for a single action in one tick.
#[derive(Debug, Clone)]
pub struct ActionEvaluation {
    pub action_id: String,
    pub kind: ValueKind,
    pub value: ActionValue,
    pub state: TriggerState,
}

/// A prioritized bundle of action + binding declarations.
///
/// Higher-priority contexts evaluate first; if an action in a higher-priority
/// context actuates, the same action id in lower-priority contexts is
/// shadowed (see `PlayerController::tick` consumption rules).
#[derive(Debug)]
pub struct InputMappingContext {
    id: String,
    priority: i32,
    /// Declaration order is preserved so `Chorded` partners can reference
    /// earlier-declared actions.
    actions: Vec<InputAction>,
    /// action_id → bindings attached to it.
    bindings: HashMap<String, Vec<InputBinding>>,
}

impl InputMappingContext {
    pub fn new(id: impl Into<String>, priority: i32) -> Self {
        Self {
            id: id.into(),
            priority,
            actions: Vec::new(),
            bindings: HashMap::new(),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn priority(&self) -> i32 {
        self.priority
    }

    pub fn add_action(&mut self, action: InputAction) {
        // Keep only the most recent declaration for a given id.
        self.actions.retain(|a| a.id != action.id);
        self.bindings.entry(action.id.clone()).or_default();
        self.actions.push(action);
    }

    pub fn add_binding(&mut self, action_id: &str, binding: InputBinding) {
        self.bindings
            .entry(action_id.to_owned())
            .or_default()
            .push(binding);
    }

    pub fn actions(&self) -> &[InputAction] {
        &self.actions
    }

    pub fn bindings(&self, action_id: &str) -> &[InputBinding] {
        self.bindings
            .get(action_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Mutable access to a binding for live-rebind support.
    pub fn binding_mut(&mut self, action_id: &str, index: usize) -> Option<&mut InputBinding> {
        self.bindings
            .get_mut(action_id)
            .and_then(|v| v.get_mut(index))
    }

    /// Reset per-trigger state across all bindings. Called when the context
    /// is first pushed or when window focus is lost.
    pub fn reset_trigger_state(&mut self) {
        for bindings in self.bindings.values_mut() {
            for b in bindings {
                for t in &mut b.triggers {
                    t.reset();
                }
            }
        }
    }

    /// Evaluate every action in this context in declaration order against
    /// the current `InputSource`, advancing per-binding trigger state by `dt`.
    ///
    /// Returns the per-action outcome in declaration order. The caller is
    /// responsible for comparing against the previous frame's evaluation to
    /// derive `EventPhase::Started / Completed` transitions.
    pub fn evaluate(&mut self, src: &InputSource<'_>, dt: f32) -> Vec<ActionEvaluation> {
        let mut out = Vec::with_capacity(self.actions.len());
        let mut actuated: HashMap<String, bool> = HashMap::new();

        for action in &self.actions {
            let bindings = self
                .bindings
                .get_mut(&action.id)
                .map(Vec::as_mut_slice)
                .unwrap_or(&mut []);

            let mut accumulated = ActionValue::zero(action.kind);
            let mut best_state = TriggerState::None;

            let ctx = TriggerCtx::new(&actuated);

            for binding in bindings.iter_mut() {
                let sampled = binding.sample(src);
                let magnitude = sampled
                    .iter()
                    .map(|c| c * c)
                    .sum::<f32>()
                    .sqrt();

                // Drive triggers with this binding's magnitude.
                let binding_state = if binding.triggers.is_empty() {
                    // No explicit trigger = implicit "Down".
                    if magnitude > crate::value::BUTTON_THRESHOLD {
                        TriggerState::Triggered
                    } else {
                        TriggerState::None
                    }
                } else {
                    let mut any_canceled = false;
                    let mut any_ongoing = false;
                    let mut all_triggered = true;
                    let mut saw_triggered = false;
                    for t in binding.triggers.iter_mut() {
                        match t.update(magnitude, dt, &ctx) {
                            TriggerState::Canceled => any_canceled = true,
                            TriggerState::Ongoing => {
                                any_ongoing = true;
                                all_triggered = false;
                            }
                            TriggerState::Triggered => {
                                saw_triggered = true;
                            }
                            TriggerState::None => {
                                all_triggered = false;
                            }
                        }
                    }
                    if any_canceled {
                        TriggerState::Canceled
                    } else if saw_triggered && all_triggered {
                        TriggerState::Triggered
                    } else if any_ongoing {
                        TriggerState::Ongoing
                    } else {
                        TriggerState::None
                    }
                };

                // Combine binding state into action state (strongest wins).
                best_state = combine_states(best_state, binding_state);

                if matches!(binding_state, TriggerState::Triggered) {
                    let narrowed = ActionValue::from_axis3d(action.kind, sampled);
                    accumulated.accumulate(narrowed);
                }
            }

            let actuated_this_action = matches!(best_state, TriggerState::Triggered)
                && accumulated.is_actuated();
            actuated.insert(action.id.clone(), actuated_this_action);

            out.push(ActionEvaluation {
                action_id: action.id.clone(),
                kind: action.kind,
                value: accumulated,
                state: best_state,
            });
        }

        out
    }
}

fn combine_states(a: TriggerState, b: TriggerState) -> TriggerState {
    use TriggerState::*;
    match (a, b) {
        (Canceled, _) | (_, Canceled) => Canceled,
        (Triggered, _) | (_, Triggered) => Triggered,
        (Ongoing, _) | (_, Ongoing) => Ongoing,
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::HardwareKey;
    use crate::bitset::{GamepadButtonSet, KeyCodeSet, MouseButtonSet};
    use crate::trigger::Trigger;
    use rython_window::KeyCode;

    fn make_src<'a>(
        keys: &'a KeyCodeSet,
        mouse: &'a MouseButtonSet,
        gpad: &'a GamepadButtonSet,
        gaxes: &'a HashMap<rython_window::GamepadAxisType, f32>,
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
    fn evaluate_button_with_pressed_trigger() {
        let mut ctx = InputMappingContext::new("test", 0);
        ctx.add_action(InputAction::new("jump", ValueKind::Button));
        ctx.add_binding(
            "jump",
            InputBinding::new(HardwareKey::Key(KeyCode::Space)).with_trigger(Trigger::pressed()),
        );

        let mut keys = KeyCodeSet::new();
        let mouse = MouseButtonSet::new();
        let gpad = GamepadButtonSet::new();
        let gaxes = HashMap::new();

        // Frame 1: Space not pressed → None.
        let out = ctx.evaluate(&make_src(&keys, &mouse, &gpad, &gaxes), 0.016);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].state, TriggerState::None);

        // Frame 2: Space pressed → Triggered.
        keys.insert(KeyCode::Space);
        let out = ctx.evaluate(&make_src(&keys, &mouse, &gpad, &gaxes), 0.016);
        assert_eq!(out[0].state, TriggerState::Triggered);
        assert_eq!(out[0].value, ActionValue::Button(true));

        // Frame 3: still held → None (Pressed is rising-edge only).
        let out = ctx.evaluate(&make_src(&keys, &mouse, &gpad, &gaxes), 0.016);
        assert_eq!(out[0].state, TriggerState::None);
    }

    #[test]
    fn evaluate_axis2d_composite_implicit_down() {
        let mut ctx = InputMappingContext::new("test", 0);
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

        let mut keys = KeyCodeSet::new();
        let mouse = MouseButtonSet::new();
        let gpad = GamepadButtonSet::new();
        let gaxes = HashMap::new();

        keys.insert(KeyCode::W);
        keys.insert(KeyCode::D);
        let out = ctx.evaluate(&make_src(&keys, &mouse, &gpad, &gaxes), 0.016);
        assert_eq!(out[0].state, TriggerState::Triggered);
        assert_eq!(out[0].value, ActionValue::Axis2D([1.0, 1.0]));
    }

    #[test]
    fn chorded_requires_earlier_partner() {
        let mut ctx = InputMappingContext::new("test", 0);
        ctx.add_action(InputAction::new("crouch", ValueKind::Button));
        ctx.add_binding(
            "crouch",
            InputBinding::new(HardwareKey::Key(KeyCode::LeftControl)),
        );
        ctx.add_action(InputAction::new("special", ValueKind::Button));
        ctx.add_binding(
            "special",
            InputBinding::new(HardwareKey::Key(KeyCode::E))
                .with_trigger(Trigger::chorded("crouch")),
        );

        let mut keys = KeyCodeSet::new();
        let mouse = MouseButtonSet::new();
        let gpad = GamepadButtonSet::new();
        let gaxes = HashMap::new();

        // E alone → no chord.
        keys.insert(KeyCode::E);
        let out = ctx.evaluate(&make_src(&keys, &mouse, &gpad, &gaxes), 0.016);
        assert_eq!(out[1].state, TriggerState::None);

        // E + LeftControl → chord fires.
        keys.insert(KeyCode::LeftControl);
        let out = ctx.evaluate(&make_src(&keys, &mouse, &gpad, &gaxes), 0.016);
        assert_eq!(out[1].state, TriggerState::Triggered);
    }

    #[test]
    fn multiple_bindings_accumulate() {
        let mut ctx = InputMappingContext::new("test", 0);
        ctx.add_action(InputAction::new("jump", ValueKind::Button));
        ctx.add_binding(
            "jump",
            InputBinding::new(HardwareKey::Key(KeyCode::Space)),
        );
        ctx.add_binding(
            "jump",
            InputBinding::new(HardwareKey::Key(KeyCode::Enter)),
        );

        let mut keys = KeyCodeSet::new();
        let mouse = MouseButtonSet::new();
        let gpad = GamepadButtonSet::new();
        let gaxes = HashMap::new();

        // Either key fires jump.
        keys.insert(KeyCode::Enter);
        let out = ctx.evaluate(&make_src(&keys, &mouse, &gpad, &gaxes), 0.016);
        assert_eq!(out[0].value, ActionValue::Button(true));
        assert_eq!(out[0].state, TriggerState::Triggered);
    }

    #[test]
    fn add_action_overwrites_previous_kind() {
        let mut ctx = InputMappingContext::new("test", 0);
        ctx.add_action(InputAction::new("foo", ValueKind::Button));
        ctx.add_action(InputAction::new("foo", ValueKind::Axis1D));
        assert_eq!(ctx.actions().len(), 1);
        assert_eq!(ctx.actions()[0].kind, ValueKind::Axis1D);
    }
}

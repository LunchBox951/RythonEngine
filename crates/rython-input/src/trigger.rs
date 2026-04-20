//! Per-binding triggers — state machines that decide whether (and with what
//! phase) an input sample actuates its action this frame.
//!
//! Each trigger is a value type carrying its own state. Built-ins: `Down`,
//! `Pressed`, `Released`, `Hold`, `Tap`, `Pulse`, `Chorded`.

use crate::value::BUTTON_THRESHOLD;
use std::collections::HashMap;

/// Phase reported by a single trigger for a single frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerState {
    /// Trigger is idle — no action.
    None,
    /// Trigger has started processing but has not yet actuated (e.g. Hold charging).
    Ongoing,
    /// Trigger is actuating this frame.
    Triggered,
    /// Trigger aborted this frame (e.g. Tap held past max_duration).
    Canceled,
}

/// Per-frame context given to `Trigger::update`. Currently carries which
/// actions have already been actuated this frame (used by `Chorded`).
///
/// Actions are evaluated in declaration order within an `InputMappingContext`,
/// so chorded triggers can only reference *earlier*-declared partners.
#[derive(Debug)]
pub struct TriggerCtx<'a> {
    pub actuated_actions: &'a HashMap<String, bool>,
}

impl<'a> TriggerCtx<'a> {
    pub fn new(actuated_actions: &'a HashMap<String, bool>) -> Self {
        Self { actuated_actions }
    }

    pub fn is_actuated(&self, action: &str) -> bool {
        self.actuated_actions
            .get(action)
            .copied()
            .unwrap_or(false)
    }
}

/// A state-carrying input trigger. Built-in variants cover the common
/// single-button patterns from Unreal's Enhanced Input system.
#[derive(Debug, Clone)]
pub enum Trigger {
    /// Fires every frame the input is actuated. Also the trigger you get by
    /// default when a binding declares no explicit triggers.
    Down,

    /// Rising-edge: fires the frame the input becomes actuated.
    Pressed { was_active: bool },

    /// Falling-edge: fires the frame the input becomes inactive after being active.
    Released { was_active: bool },

    /// Fires after `threshold_s` of continuous actuation. Reports `Ongoing`
    /// while charging, `Triggered` each frame once the threshold is crossed,
    /// `Canceled` if released before the threshold.
    Hold {
        threshold_s: f32,
        elapsed_s: f32,
        was_active: bool,
        triggered_once: bool,
    },

    /// Fires once on release if the total hold duration stayed under
    /// `max_duration_s`. Reports `Canceled` the frame it exceeds the budget.
    Tap {
        max_duration_s: f32,
        elapsed_s: f32,
        was_active: bool,
        canceled: bool,
    },

    /// Fires on the initial actuation and then every `interval_s` while held.
    /// Completes when released.
    Pulse {
        interval_s: f32,
        elapsed_since_fire: f32,
        was_active: bool,
    },

    /// Fires only when the input is actuated AND the named partner action has
    /// also been actuated this frame (see `TriggerCtx`).
    Chorded { partner: String },
}

impl Trigger {
    pub fn down() -> Self {
        Self::Down
    }

    pub fn pressed() -> Self {
        Self::Pressed { was_active: false }
    }

    pub fn released() -> Self {
        Self::Released { was_active: false }
    }

    pub fn hold(threshold_s: f32) -> Self {
        Self::Hold {
            threshold_s,
            elapsed_s: 0.0,
            was_active: false,
            triggered_once: false,
        }
    }

    pub fn tap(max_duration_s: f32) -> Self {
        Self::Tap {
            max_duration_s,
            elapsed_s: 0.0,
            was_active: false,
            canceled: false,
        }
    }

    pub fn pulse(interval_s: f32) -> Self {
        Self::Pulse {
            interval_s,
            elapsed_since_fire: 0.0,
            was_active: false,
        }
    }

    pub fn chorded(partner: impl Into<String>) -> Self {
        Self::Chorded {
            partner: partner.into(),
        }
    }

    /// Advance the trigger's state machine by `dt` seconds given the current
    /// sample magnitude (axis magnitude or button-as-float).
    pub fn update(&mut self, magnitude: f32, dt: f32, ctx: &TriggerCtx<'_>) -> TriggerState {
        let active = magnitude.abs() > BUTTON_THRESHOLD;
        match self {
            Self::Down => {
                if active {
                    TriggerState::Triggered
                } else {
                    TriggerState::None
                }
            }

            Self::Pressed { was_active } => {
                let state = if active && !*was_active {
                    TriggerState::Triggered
                } else {
                    TriggerState::None
                };
                *was_active = active;
                state
            }

            Self::Released { was_active } => {
                let state = if !active && *was_active {
                    TriggerState::Triggered
                } else {
                    TriggerState::None
                };
                *was_active = active;
                state
            }

            Self::Hold {
                threshold_s,
                elapsed_s,
                was_active,
                triggered_once,
            } => {
                let state = if active {
                    *elapsed_s += dt;
                    if *elapsed_s >= *threshold_s {
                        *triggered_once = true;
                        TriggerState::Triggered
                    } else {
                        TriggerState::Ongoing
                    }
                } else {
                    let prev_active = *was_active;
                    let was_triggered = *triggered_once;
                    *elapsed_s = 0.0;
                    *triggered_once = false;
                    // Canceled only if we were charging and never reached the
                    // threshold. If the hold completed, we go idle silently.
                    if prev_active && !was_triggered {
                        TriggerState::Canceled
                    } else {
                        TriggerState::None
                    }
                };
                *was_active = active;
                state
            }

            Self::Tap {
                max_duration_s,
                elapsed_s,
                was_active,
                canceled,
            } => {
                let state = if active {
                    *elapsed_s += dt;
                    if *elapsed_s > *max_duration_s {
                        if !*canceled {
                            *canceled = true;
                            TriggerState::Canceled
                        } else {
                            TriggerState::None
                        }
                    } else {
                        TriggerState::Ongoing
                    }
                } else {
                    // Just released.
                    let state = if *was_active && !*canceled && *elapsed_s <= *max_duration_s {
                        TriggerState::Triggered
                    } else {
                        TriggerState::None
                    };
                    *elapsed_s = 0.0;
                    *canceled = false;
                    state
                };
                *was_active = active;
                state
            }

            Self::Pulse {
                interval_s,
                elapsed_since_fire,
                was_active,
            } => {
                let state = if active {
                    if !*was_active {
                        // Initial press fires immediately.
                        *elapsed_since_fire = 0.0;
                        TriggerState::Triggered
                    } else {
                        *elapsed_since_fire += dt;
                        if *elapsed_since_fire >= *interval_s {
                            *elapsed_since_fire -= *interval_s;
                            TriggerState::Triggered
                        } else {
                            TriggerState::Ongoing
                        }
                    }
                } else {
                    *elapsed_since_fire = 0.0;
                    TriggerState::None
                };
                *was_active = active;
                state
            }

            Self::Chorded { partner } => {
                if active && ctx.is_actuated(partner) {
                    TriggerState::Triggered
                } else {
                    TriggerState::None
                }
            }
        }
    }

    /// Reset any accumulated state. Call when the map is re-pushed or focus is lost.
    pub fn reset(&mut self) {
        match self {
            Self::Down | Self::Chorded { .. } => {}
            Self::Pressed { was_active } | Self::Released { was_active } => *was_active = false,
            Self::Hold {
                elapsed_s,
                was_active,
                triggered_once,
                ..
            } => {
                *elapsed_s = 0.0;
                *was_active = false;
                *triggered_once = false;
            }
            Self::Tap {
                elapsed_s,
                was_active,
                canceled,
                ..
            } => {
                *elapsed_s = 0.0;
                *was_active = false;
                *canceled = false;
            }
            Self::Pulse {
                elapsed_since_fire,
                was_active,
                ..
            } => {
                *elapsed_since_fire = 0.0;
                *was_active = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_ctx() -> HashMap<String, bool> {
        HashMap::new()
    }

    fn run(trigger: &mut Trigger, samples: &[(f32, f32)]) -> Vec<TriggerState> {
        let actuated = empty_ctx();
        let ctx = TriggerCtx::new(&actuated);
        samples
            .iter()
            .map(|(mag, dt)| trigger.update(*mag, *dt, &ctx))
            .collect()
    }

    #[test]
    fn down_triggers_while_active() {
        let mut t = Trigger::down();
        let states = run(&mut t, &[(0.0, 0.016), (1.0, 0.016), (1.0, 0.016), (0.0, 0.016)]);
        assert_eq!(
            states,
            vec![
                TriggerState::None,
                TriggerState::Triggered,
                TriggerState::Triggered,
                TriggerState::None,
            ]
        );
    }

    #[test]
    fn pressed_fires_only_on_rising_edge() {
        let mut t = Trigger::pressed();
        let states = run(
            &mut t,
            &[
                (0.0, 0.016),
                (1.0, 0.016),
                (1.0, 0.016),
                (0.0, 0.016),
                (1.0, 0.016),
            ],
        );
        assert_eq!(
            states,
            vec![
                TriggerState::None,
                TriggerState::Triggered,
                TriggerState::None,
                TriggerState::None,
                TriggerState::Triggered,
            ]
        );
    }

    #[test]
    fn released_fires_only_on_falling_edge() {
        let mut t = Trigger::released();
        let states = run(
            &mut t,
            &[(0.0, 0.016), (1.0, 0.016), (1.0, 0.016), (0.0, 0.016)],
        );
        assert_eq!(
            states,
            vec![
                TriggerState::None,
                TriggerState::None,
                TriggerState::None,
                TriggerState::Triggered,
            ]
        );
    }

    #[test]
    fn hold_reaches_threshold() {
        let mut t = Trigger::hold(0.3);
        // Active for 0.1s, 0.2s, 0.3s total → Ongoing, Ongoing, Triggered.
        let states = run(&mut t, &[(1.0, 0.1), (1.0, 0.1), (1.0, 0.1), (1.0, 0.1)]);
        assert_eq!(
            states,
            vec![
                TriggerState::Ongoing,
                TriggerState::Ongoing,
                TriggerState::Triggered,
                TriggerState::Triggered,
            ]
        );
    }

    #[test]
    fn hold_canceled_if_released_early() {
        let mut t = Trigger::hold(0.3);
        let states = run(&mut t, &[(1.0, 0.1), (1.0, 0.1), (0.0, 0.016)]);
        assert_eq!(
            states,
            vec![
                TriggerState::Ongoing,
                TriggerState::Ongoing,
                TriggerState::Canceled,
            ]
        );
    }

    #[test]
    fn hold_not_canceled_after_successful_fire() {
        let mut t = Trigger::hold(0.2);
        let states = run(&mut t, &[(1.0, 0.1), (1.0, 0.1), (1.0, 0.1), (0.0, 0.016)]);
        assert_eq!(
            states,
            vec![
                TriggerState::Ongoing,
                TriggerState::Triggered,
                TriggerState::Triggered,
                TriggerState::None, // clean release after a successful hold
            ]
        );
    }

    #[test]
    fn tap_fires_on_quick_release() {
        let mut t = Trigger::tap(0.25);
        let states = run(&mut t, &[(1.0, 0.1), (0.0, 0.016)]);
        assert_eq!(
            states,
            vec![TriggerState::Ongoing, TriggerState::Triggered]
        );
    }

    #[test]
    fn tap_canceled_if_held_too_long() {
        let mut t = Trigger::tap(0.2);
        let states = run(
            &mut t,
            &[(1.0, 0.1), (1.0, 0.1), (1.0, 0.1), (0.0, 0.016)],
        );
        assert_eq!(
            states,
            vec![
                TriggerState::Ongoing,
                TriggerState::Ongoing,
                TriggerState::Canceled,
                TriggerState::None, // release after cancel — silent
            ]
        );
    }

    #[test]
    fn pulse_fires_on_initial_and_every_interval() {
        let mut t = Trigger::pulse(0.2);
        // Starting inactive, then held for 0.8s with 0.1s dt each frame.
        let states = run(
            &mut t,
            &[
                (0.0, 0.016),
                (1.0, 0.1), // initial press → Triggered
                (1.0, 0.1), // 0.1s accumulated → Ongoing
                (1.0, 0.1), // 0.2s accumulated → Triggered
                (1.0, 0.1), // 0.1s accumulated → Ongoing
                (1.0, 0.1), // 0.2s → Triggered
                (0.0, 0.016),
            ],
        );
        assert_eq!(
            states,
            vec![
                TriggerState::None,
                TriggerState::Triggered,
                TriggerState::Ongoing,
                TriggerState::Triggered,
                TriggerState::Ongoing,
                TriggerState::Triggered,
                TriggerState::None,
            ]
        );
    }

    #[test]
    fn chorded_requires_partner_actuated() {
        let mut t = Trigger::chorded("crouch");

        let mut actuated = HashMap::new();
        let ctx = TriggerCtx::new(&actuated);
        // Partner absent → no fire.
        assert_eq!(t.update(1.0, 0.016, &ctx), TriggerState::None);

        actuated.insert("crouch".into(), true);
        let ctx = TriggerCtx::new(&actuated);
        // Partner present → fire.
        assert_eq!(t.update(1.0, 0.016, &ctx), TriggerState::Triggered);
        // Partner present but input inactive → no fire.
        assert_eq!(t.update(0.0, 0.016, &ctx), TriggerState::None);
    }
}

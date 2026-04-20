use crate::value::ActionValue;
use rython_core::Event;

/// Phase of an action event, mirroring the state machine of `Trigger`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventPhase {
    /// First frame the action becomes active (rising edge).
    Started,
    /// Action is progressing but not yet actuating (e.g. Hold still charging).
    Ongoing,
    /// Action is actuating this frame.
    Triggered,
    /// Action just finished (clean falling edge after being actuated).
    Completed,
    /// Action aborted (e.g. Tap held past max_duration).
    Canceled,
}

impl EventPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Ongoing => "ongoing",
            Self::Triggered => "triggered",
            Self::Completed => "completed",
            Self::Canceled => "canceled",
        }
    }
}

/// Fired on the event bus when an input action changes state.
#[derive(Debug, Clone)]
pub struct InputActionEvent {
    pub action: String,
    pub value: ActionValue,
    pub phase: EventPhase,
    /// Seconds the action has been in its current "in-progress" run. Zero for
    /// one-shot phases like `Started`; counts up across frames for `Ongoing`
    /// and `Triggered`, then resets on `Completed` / `Canceled`.
    pub elapsed_seconds: f32,
}

impl Event for InputActionEvent {}

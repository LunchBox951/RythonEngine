use rython_core::Event;

/// Fired on the event bus when an input action changes state.
#[derive(Debug, Clone)]
pub struct InputActionEvent {
    pub action: String,
    pub value: f32,
}

impl Event for InputActionEvent {}

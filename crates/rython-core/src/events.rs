/// Marker trait for engine events. Events must be Send + Sync + 'static.
pub trait Event: Send + Sync + 'static {}

/// Unique identifier for an event handler registration.
pub type HandlerId = u64;

/// A named event that can carry arbitrary payload.
#[derive(Debug, Clone)]
pub struct NamedEvent {
    pub name: String,
    pub payload: serde_json::Value,
}

impl Event for NamedEvent {}

//! Logical input action declarations.

use crate::value::ValueKind;

/// A logical action declared in an `InputMappingContext`.
///
/// Actions are addressed by their `id` (e.g. `"jump"`, `"move"`) and carry a
/// `kind` that determines the dimensionality of the value that flows through
/// them at runtime.
#[derive(Debug, Clone)]
pub struct InputAction {
    pub id: String,
    pub kind: ValueKind,
}

impl InputAction {
    pub fn new(id: impl Into<String>, kind: ValueKind) -> Self {
        Self {
            id: id.into(),
            kind,
        }
    }
}

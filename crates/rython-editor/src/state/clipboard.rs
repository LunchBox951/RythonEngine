use crate::state::undo::EntitySnapshot;

/// In-memory editor clipboard for copy/paste of entities.
///
/// Stores root-level `EntitySnapshot`s; children are embedded recursively
/// within each snapshot's `children` field.
///
/// On paste, callers must assign fresh `EntityId`s and remap parent-child
/// relationships — this struct stores the serialized source data only.
#[derive(Clone, Default)]
pub struct Clipboard {
    /// Root entity snapshots copied from the scene (children nested inside).
    pub roots: Vec<EntitySnapshot>,
}

impl Clipboard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }

    /// Replace clipboard contents with new root snapshots.
    pub fn set(&mut self, roots: Vec<EntitySnapshot>) {
        self.roots = roots;
    }

    pub fn clear(&mut self) {
        self.roots.clear();
    }
}

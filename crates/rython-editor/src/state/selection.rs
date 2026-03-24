use std::path::PathBuf;

use rython_ecs::EntityId;

/// Identifier for a UI widget (Phase 4).
pub type WidgetId = u64;

#[derive(Debug, Clone, PartialEq)]
pub enum Selection {
    None,
    Entity(EntityId),
    Widget(WidgetId),
    Asset(PathBuf),
}

impl Default for Selection {
    fn default() -> Self {
        Selection::None
    }
}

#[derive(Debug, Default)]
pub struct SelectionState {
    pub current: Selection,
}

impl SelectionState {
    pub fn select_entity(&mut self, id: EntityId) {
        self.current = Selection::Entity(id);
    }

    pub fn clear(&mut self) {
        self.current = Selection::None;
    }

    pub fn selected_entity(&self) -> Option<EntityId> {
        if let Selection::Entity(id) = self.current {
            Some(id)
        } else {
            None
        }
    }
}

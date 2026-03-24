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

/// Tracks the current editor selection, including multi-entity selection.
///
/// `current` is the primary (last-clicked) selection. `multi` holds any
/// additional entity IDs added via Ctrl+Click or Shift+Click.
///
/// Note: the spec refers to `current` as `primary`; that rename will occur
/// once ui_editor.rs and asset_browser.rs are updated by their owners.
#[derive(Debug, Default)]
pub struct SelectionState {
    /// Primary (last-clicked) selection.
    pub current: Selection,
    /// Additional selected entities (multi-select).
    pub multi: Vec<EntityId>,
}

impl SelectionState {
    /// Select a single entity, clearing any multi-select.
    pub fn select_entity(&mut self, id: EntityId) {
        self.current = Selection::Entity(id);
        self.multi.clear();
    }

    /// Toggle `id` in/out of the multi-select set (Ctrl+Click behaviour).
    ///
    /// - If `id` is already the primary entity, does nothing.
    /// - If `id` is already in `multi`, removes it.
    /// - Otherwise promotes the existing primary into `multi` and sets `id`
    ///   as the new primary.
    pub fn toggle_multi(&mut self, id: EntityId) {
        // Already in multi → deselect it
        if let Some(pos) = self.multi.iter().position(|&e| e == id) {
            self.multi.remove(pos);
            if self.current == Selection::Entity(id) {
                self.current = self
                    .multi
                    .first()
                    .copied()
                    .map(Selection::Entity)
                    .unwrap_or(Selection::None);
            }
            return;
        }
        // Already the primary → nothing to do
        if self.current == Selection::Entity(id) {
            return;
        }
        // Promote existing primary into multi
        if let Selection::Entity(existing) = self.current {
            if !self.multi.contains(&existing) {
                self.multi.push(existing);
            }
        }
        self.current = Selection::Entity(id);
        self.multi.push(id);
    }

    /// Range-select all entities between `from` and `to` in `ordered_entities`
    /// (Shift+Click hierarchy behaviour).
    ///
    /// Entities in the range are added to `multi`; `current` is set to `to`.
    pub fn range_select(&mut self, from: EntityId, to: EntityId, ordered_entities: &[EntityId]) {
        let from_idx = ordered_entities.iter().position(|&e| e == from);
        let to_idx = ordered_entities.iter().position(|&e| e == to);
        let (Some(a), Some(b)) = (from_idx, to_idx) else {
            self.select_entity(to);
            return;
        };
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        for &entity in &ordered_entities[lo..=hi] {
            if !self.multi.contains(&entity) {
                self.multi.push(entity);
            }
        }
        self.current = Selection::Entity(to);
    }

    /// Clear all selections.
    pub fn clear(&mut self) {
        self.current = Selection::None;
        self.multi.clear();
    }

    /// Returns the primary selected entity, if any.
    pub fn selected_entity(&self) -> Option<EntityId> {
        if let Selection::Entity(id) = self.current {
            Some(id)
        } else {
            None
        }
    }

    /// Returns all selected entity IDs (primary + multi, deduplicated).
    pub fn all_selected_entities(&self) -> Vec<EntityId> {
        let mut result = Vec::new();
        if let Selection::Entity(id) = self.current {
            result.push(id);
        }
        for &id in &self.multi {
            if !result.contains(&id) {
                result.push(id);
            }
        }
        result
    }

    /// Returns true if more than one entity is selected.
    pub fn is_multi_select(&self) -> bool {
        !self.multi.is_empty() && self.selected_entity().is_some()
    }
}

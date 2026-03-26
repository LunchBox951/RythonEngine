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

#[cfg(test)]
mod tests {
    use super::*;

    fn eid(n: u64) -> EntityId {
        EntityId(n)
    }

    #[test]
    fn default_selection_is_none() {
        let s = SelectionState::default();
        assert_eq!(s.current, Selection::None);
        assert!(s.multi.is_empty());
    }

    #[test]
    fn select_entity_sets_primary() {
        let mut s = SelectionState::default();
        s.select_entity(eid(1));
        assert_eq!(s.current, Selection::Entity(eid(1)));
    }

    #[test]
    fn select_entity_clears_multi() {
        let mut s = SelectionState::default();
        s.select_entity(eid(1));
        s.toggle_multi(eid(2));
        assert!(!s.multi.is_empty());
        s.select_entity(eid(3));
        assert!(s.multi.is_empty());
        assert_eq!(s.current, Selection::Entity(eid(3)));
    }

    #[test]
    fn toggle_multi_adds_second_entity_as_primary() {
        let mut s = SelectionState::default();
        s.select_entity(eid(1));
        s.toggle_multi(eid(2));
        assert_eq!(s.current, Selection::Entity(eid(2)));
        assert!(s.multi.contains(&eid(1)));
    }

    #[test]
    fn toggle_multi_three_entities_all_tracked() {
        let mut s = SelectionState::default();
        s.select_entity(eid(1));
        s.toggle_multi(eid(2));
        s.toggle_multi(eid(3));
        let all = s.all_selected_entities();
        assert!(all.contains(&eid(1)));
        assert!(all.contains(&eid(2)));
        assert!(all.contains(&eid(3)));
    }

    #[test]
    fn toggle_multi_removes_entity_already_in_multi() {
        let mut s = SelectionState::default();
        s.select_entity(eid(1));
        s.toggle_multi(eid(2)); // multi=[1], current=2
        s.toggle_multi(eid(1)); // remove 1 from multi
        assert!(!s.multi.contains(&eid(1)));
    }

    #[test]
    fn toggle_multi_noop_on_primary_entity() {
        let mut s = SelectionState::default();
        s.select_entity(eid(5));
        s.toggle_multi(eid(5)); // same as current — should be noop
        assert_eq!(s.current, Selection::Entity(eid(5)));
        assert!(s.multi.is_empty());
    }

    #[test]
    fn range_select_forward() {
        let mut s = SelectionState::default();
        let order = vec![eid(10), eid(20), eid(30), eid(40)];
        s.range_select(eid(10), eid(30), &order);
        assert_eq!(s.current, Selection::Entity(eid(30)));
        let all = s.all_selected_entities();
        assert!(all.contains(&eid(10)));
        assert!(all.contains(&eid(20)));
        assert!(all.contains(&eid(30)));
        assert!(!all.contains(&eid(40)));
    }

    #[test]
    fn range_select_backward() {
        let mut s = SelectionState::default();
        let order = vec![eid(10), eid(20), eid(30), eid(40)];
        s.range_select(eid(30), eid(10), &order);
        assert_eq!(s.current, Selection::Entity(eid(10)));
        let all = s.all_selected_entities();
        assert!(all.contains(&eid(10)));
        assert!(all.contains(&eid(20)));
        assert!(all.contains(&eid(30)));
    }

    #[test]
    fn range_select_single_element() {
        let mut s = SelectionState::default();
        let order = vec![eid(1), eid(2), eid(3)];
        s.range_select(eid(2), eid(2), &order);
        assert_eq!(s.current, Selection::Entity(eid(2)));
        let all = s.all_selected_entities();
        assert_eq!(all.len(), 1);
    }

    #[test]
    fn range_select_missing_entity_falls_back_to_select_entity() {
        let mut s = SelectionState::default();
        let order = vec![eid(1), eid(2)];
        s.range_select(eid(99), eid(2), &order);
        assert_eq!(s.current, Selection::Entity(eid(2)));
    }

    #[test]
    fn clear_resets_all_selections() {
        let mut s = SelectionState::default();
        s.select_entity(eid(1));
        s.toggle_multi(eid(2));
        s.clear();
        assert_eq!(s.current, Selection::None);
        assert!(s.multi.is_empty());
    }

    #[test]
    fn selected_entity_returns_some_for_entity_selection() {
        let mut s = SelectionState::default();
        s.select_entity(eid(7));
        assert_eq!(s.selected_entity(), Some(eid(7)));
    }

    #[test]
    fn selected_entity_returns_none_for_widget_selection() {
        let mut s = SelectionState::default();
        s.current = Selection::Widget(42);
        assert_eq!(s.selected_entity(), None);
    }

    #[test]
    fn selected_entity_returns_none_when_none() {
        let s = SelectionState::default();
        assert_eq!(s.selected_entity(), None);
    }

    #[test]
    fn all_selected_entities_deduplicates_primary_and_multi() {
        let mut s = SelectionState::default();
        s.select_entity(eid(1));
        // Manually push a duplicate (edge case — normally toggle_multi prevents this)
        s.multi.push(eid(1));
        s.multi.push(eid(2));
        let all = s.all_selected_entities();
        let count_1 = all.iter().filter(|&&e| e == eid(1)).count();
        assert_eq!(count_1, 1, "primary entity must appear only once");
        assert!(all.contains(&eid(2)));
    }

    #[test]
    fn all_selected_entities_returns_empty_when_none() {
        let s = SelectionState::default();
        assert!(s.all_selected_entities().is_empty());
    }

    #[test]
    fn is_multi_select_false_with_single_entity() {
        let mut s = SelectionState::default();
        s.select_entity(eid(1));
        assert!(!s.is_multi_select());
    }

    #[test]
    fn is_multi_select_true_when_multiple_entities_selected() {
        let mut s = SelectionState::default();
        s.select_entity(eid(1));
        s.toggle_multi(eid(2));
        assert!(s.is_multi_select());
    }

    #[test]
    fn is_multi_select_false_when_selection_is_none() {
        let s = SelectionState::default();
        assert!(!s.is_multi_select());
    }

    #[test]
    fn selection_none_is_default() {
        assert_eq!(Selection::default(), Selection::None);
    }
}

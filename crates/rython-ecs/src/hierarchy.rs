use std::collections::HashMap;
use parking_lot::RwLock;
use crate::entity::EntityId;

pub const MAX_HIERARCHY_DEPTH: usize = 64;

/// Internal maps bundled under a single lock to eliminate multi-lock overhead.
struct HierarchyMaps {
    parent_map: HashMap<EntityId, EntityId>,
    children_map: HashMap<EntityId, Vec<EntityId>>,
}

pub struct Hierarchy {
    maps: RwLock<HierarchyMaps>,
}

impl Default for Hierarchy {
    fn default() -> Self {
        Self {
            maps: RwLock::new(HierarchyMaps {
                parent_map: HashMap::new(),
                children_map: HashMap::new(),
            }),
        }
    }
}

impl Hierarchy {
    pub fn new() -> Self { Self::default() }

    pub fn set_parent(&self, child: EntityId, parent: EntityId) {
        let mut m = self.maps.write();
        // Remove from old parent's children list
        if let Some(old_parent) = m.parent_map.get(&child).copied() {
            if let Some(siblings) = m.children_map.get_mut(&old_parent) {
                siblings.retain(|e| *e != child);
            }
        }
        m.parent_map.insert(child, parent);
        m.children_map.entry(parent).or_default().push(child);
    }

    pub fn clear_parent(&self, child: EntityId) {
        let mut m = self.maps.write();
        if let Some(old_parent) = m.parent_map.remove(&child) {
            if let Some(siblings) = m.children_map.get_mut(&old_parent) {
                siblings.retain(|e| *e != child);
            }
        }
    }

    pub fn get_parent(&self, child: EntityId) -> Option<EntityId> {
        self.maps.read().parent_map.get(&child).copied()
    }

    pub fn get_children(&self, parent: EntityId) -> Vec<EntityId> {
        self.maps.read().children_map.get(&parent).cloned().unwrap_or_default()
    }

    /// Walk the ancestor chain from `entity` up. Returns ordered chain [entity, parent, grandparent, ...].
    /// Stops at root or MAX_HIERARCHY_DEPTH. Returns (chain, depth_exceeded).
    pub fn ancestor_chain(&self, entity: EntityId) -> (Vec<EntityId>, bool) {
        let mut chain = vec![entity];
        let m = self.maps.read();
        let mut current = entity;
        for _ in 0..MAX_HIERARCHY_DEPTH {
            match m.parent_map.get(&current).copied() {
                Some(p) => {
                    chain.push(p);
                    current = p;
                }
                None => return (chain, false),
            }
        }
        (chain, true)
    }

    /// Orphan all children of `entity` (remove their parent pointer without cascade-despawn).
    pub fn orphan_children(&self, parent: EntityId) {
        let mut m = self.maps.write();
        let children = m.children_map.remove(&parent).unwrap_or_default();
        for child in children {
            m.parent_map.remove(&child);
        }
    }

    /// Remove entity from all hierarchy references.
    pub fn remove_entity(&self, entity: EntityId) {
        // Single write lock for the entire operation
        let mut m = self.maps.write();
        // Orphan children
        let children = m.children_map.remove(&entity).unwrap_or_default();
        for child in children {
            m.parent_map.remove(&child);
        }
        // Clear parent
        if let Some(old_parent) = m.parent_map.remove(&entity) {
            if let Some(siblings) = m.children_map.get_mut(&old_parent) {
                siblings.retain(|e| *e != entity);
            }
        }
    }

    pub fn clear(&self) {
        let mut m = self.maps.write();
        m.parent_map.clear();
        m.children_map.clear();
    }
}

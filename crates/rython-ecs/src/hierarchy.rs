use std::collections::HashMap;
use parking_lot::RwLock;
use crate::entity::EntityId;

pub const MAX_HIERARCHY_DEPTH: usize = 64;

pub struct Hierarchy {
    parent_map: RwLock<HashMap<EntityId, EntityId>>,
    children_map: RwLock<HashMap<EntityId, Vec<EntityId>>>,
}

impl Default for Hierarchy {
    fn default() -> Self {
        Self {
            parent_map: RwLock::new(HashMap::new()),
            children_map: RwLock::new(HashMap::new()),
        }
    }
}

impl Hierarchy {
    pub fn new() -> Self { Self::default() }

    pub fn set_parent(&self, child: EntityId, parent: EntityId) {
        // Remove from old parent's children list
        if let Some(old_parent) = self.parent_map.read().get(&child).copied() {
            let mut cm = self.children_map.write();
            if let Some(siblings) = cm.get_mut(&old_parent) {
                siblings.retain(|e| *e != child);
            }
        }
        self.parent_map.write().insert(child, parent);
        self.children_map.write().entry(parent).or_default().push(child);
    }

    pub fn clear_parent(&self, child: EntityId) {
        if let Some(old_parent) = self.parent_map.write().remove(&child) {
            let mut cm = self.children_map.write();
            if let Some(siblings) = cm.get_mut(&old_parent) {
                siblings.retain(|e| *e != child);
            }
        }
    }

    pub fn get_parent(&self, child: EntityId) -> Option<EntityId> {
        self.parent_map.read().get(&child).copied()
    }

    pub fn get_children(&self, parent: EntityId) -> Vec<EntityId> {
        self.children_map.read().get(&parent).cloned().unwrap_or_default()
    }

    /// Walk the ancestor chain from `entity` up. Returns ordered chain [entity, parent, grandparent, ...].
    /// Stops at root or MAX_HIERARCHY_DEPTH. Returns (chain, depth_exceeded).
    pub fn ancestor_chain(&self, entity: EntityId) -> (Vec<EntityId>, bool) {
        let mut chain = vec![entity];
        let pm = self.parent_map.read();
        let mut current = entity;
        for _ in 0..MAX_HIERARCHY_DEPTH {
            match pm.get(&current).copied() {
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
        let children: Vec<EntityId> = {
            let mut cm = self.children_map.write();
            cm.remove(&parent).unwrap_or_default()
        };
        let mut pm = self.parent_map.write();
        for child in children {
            pm.remove(&child);
        }
    }

    /// Remove entity from all hierarchy references.
    pub fn remove_entity(&self, entity: EntityId) {
        self.orphan_children(entity);
        self.clear_parent(entity);
    }

    pub fn clear(&self) {
        self.parent_map.write().clear();
        self.children_map.write().clear();
    }
}

use std::any::TypeId;
use std::collections::HashMap;
use parking_lot::RwLock;
use downcast_rs::{impl_downcast, Downcast};
use serde::{Serialize, Deserialize};
use crate::entity::EntityId;

/// Core component trait — all components must implement this.
pub trait Component: Downcast + Send + Sync + 'static {
    fn component_type_name(&self) -> &'static str;
    fn clone_box(&self) -> Box<dyn Component>;
    fn serialize_json(&self) -> serde_json::Value;
}

impl_downcast!(Component);

// ── Built-in Component Types ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformComponent {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rot_x: f32,
    pub rot_y: f32,
    pub rot_z: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub scale_z: f32,
}

impl Default for TransformComponent {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0, z: 0.0, rot_x: 0.0, rot_y: 0.0, rot_z: 0.0, scale_x: 1.0, scale_y: 1.0, scale_z: 1.0 }
    }
}

impl Component for TransformComponent {
    fn component_type_name(&self) -> &'static str { "TransformComponent" }
    fn clone_box(&self) -> Box<dyn Component> { Box::new(self.clone()) }
    fn serialize_json(&self) -> serde_json::Value { serde_json::to_value(self).unwrap() }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshComponent {
    pub mesh_id: String,
    pub texture_id: String,
    pub yaw_offset: f32,
    pub shininess: f32,
    pub visible: bool,
    /// PBR metallic hint [0, 1]; 0 = dielectric (default), 1 = metal.
    #[serde(default)]
    pub metallic: f32,
    /// PBR roughness hint [0, 1]; 0 = mirror-smooth, 1 = fully rough (default 0.5).
    #[serde(default = "MeshComponent::default_roughness")]
    pub roughness: f32,
}

impl MeshComponent {
    fn default_roughness() -> f32 { 0.5 }
}

impl Default for MeshComponent {
    fn default() -> Self {
        Self {
            mesh_id: String::new(),
            texture_id: String::new(),
            yaw_offset: 0.0,
            shininess: 0.0,
            visible: true,
            metallic: 0.0,
            roughness: 0.5,
        }
    }
}

impl Component for MeshComponent {
    fn component_type_name(&self) -> &'static str { "MeshComponent" }
    fn clone_box(&self) -> Box<dyn Component> { Box::new(self.clone()) }
    fn serialize_json(&self) -> serde_json::Value { serde_json::to_value(self).unwrap() }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillboardComponent {
    pub asset_id: String,
    pub width: f32,
    pub height: f32,
    pub uv_rect: [f32; 4],
    pub alpha: f32,
}

impl Default for BillboardComponent {
    fn default() -> Self {
        Self {
            asset_id: String::new(),
            width: 1.0,
            height: 1.0,
            uv_rect: [0.0, 0.0, 1.0, 1.0],
            alpha: 1.0,
        }
    }
}

impl Component for BillboardComponent {
    fn component_type_name(&self) -> &'static str { "BillboardComponent" }
    fn clone_box(&self) -> Box<dyn Component> { Box::new(self.clone()) }
    fn serialize_json(&self) -> serde_json::Value { serde_json::to_value(self).unwrap() }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagComponent {
    pub tags: Vec<String>,
}

impl Default for TagComponent {
    fn default() -> Self { Self { tags: Vec::new() } }
}

impl Component for TagComponent {
    fn component_type_name(&self) -> &'static str { "TagComponent" }
    fn clone_box(&self) -> Box<dyn Component> { Box::new(self.clone()) }
    fn serialize_json(&self) -> serde_json::Value { serde_json::to_value(self).unwrap() }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RigidBodyComponent {
    pub body_type: String,
    pub mass: f32,
    pub gravity_factor: f32,
    pub collision_layer: u32,
    pub collision_mask: u32,
}

impl Default for RigidBodyComponent {
    fn default() -> Self {
        Self {
            body_type: "dynamic".to_string(),
            mass: 1.0,
            gravity_factor: 1.0,
            collision_layer: 1,
            collision_mask: 1,
        }
    }
}

impl Component for RigidBodyComponent {
    fn component_type_name(&self) -> &'static str { "RigidBodyComponent" }
    fn clone_box(&self) -> Box<dyn Component> { Box::new(self.clone()) }
    fn serialize_json(&self) -> serde_json::Value { serde_json::to_value(self).unwrap() }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColliderComponent {
    pub shape: String,
    pub size: [f32; 3],
    pub is_trigger: bool,
}

impl Default for ColliderComponent {
    fn default() -> Self {
        Self { shape: "box".to_string(), size: [1.0, 1.0, 1.0], is_trigger: false }
    }
}

impl Component for ColliderComponent {
    fn component_type_name(&self) -> &'static str { "ColliderComponent" }
    fn clone_box(&self) -> Box<dyn Component> { Box::new(self.clone()) }
    fn serialize_json(&self) -> serde_json::Value { serde_json::to_value(self).unwrap() }
}

// ── Component Storage ────────────────────────────────────────────────────────

/// Per-type storage: entity ID -> boxed component, protected by RwLock.
pub type TypeStore = RwLock<HashMap<EntityId, Box<dyn Component>>>;

/// Global component storage: one TypeStore per component TypeId.
pub struct ComponentStorage {
    stores: RwLock<HashMap<TypeId, TypeStore>>,
}

impl Default for ComponentStorage {
    fn default() -> Self {
        Self { stores: RwLock::new(HashMap::new()) }
    }
}

impl ComponentStorage {
    pub fn new() -> Self { Self::default() }

    pub fn insert<C: Component>(&self, entity: EntityId, component: C) {
        let tid = TypeId::of::<C>();
        // Get or create store for this type
        {
            let read = self.stores.read();
            if let Some(store) = read.get(&tid) {
                store.write().insert(entity, Box::new(component));
                return;
            }
        }
        let mut write = self.stores.write();
        let store = write.entry(tid).or_insert_with(|| RwLock::new(HashMap::new()));
        store.write().insert(entity, Box::new(component));
    }

    pub fn insert_boxed(&self, entity: EntityId, tid: TypeId, component: Box<dyn Component>) {
        let read = self.stores.read();
        if let Some(store) = read.get(&tid) {
            store.write().insert(entity, component);
            return;
        }
        drop(read);
        let mut write = self.stores.write();
        let store = write.entry(tid).or_insert_with(|| RwLock::new(HashMap::new()));
        store.write().insert(entity, component);
    }

    pub fn get<C: Component>(&self, entity: EntityId) -> Option<C>
    where
        C: Clone,
    {
        let tid = TypeId::of::<C>();
        let stores = self.stores.read();
        let store = stores.get(&tid)?;
        let map = store.read();
        let comp = map.get(&entity)?;
        comp.downcast_ref::<C>().map(|c| c.clone())
    }

    pub fn get_ref<C: Component, F, R>(&self, entity: EntityId, f: F) -> Option<R>
    where
        F: FnOnce(&C) -> R,
    {
        let tid = TypeId::of::<C>();
        let stores = self.stores.read();
        let store = stores.get(&tid)?;
        let map = store.read();
        let comp = map.get(&entity)?;
        comp.downcast_ref::<C>().map(f)
    }

    pub fn get_mut<C: Component, F>(&self, entity: EntityId, f: F) -> bool
    where
        F: FnOnce(&mut C),
    {
        let tid = TypeId::of::<C>();
        let stores = self.stores.read();
        if let Some(store) = stores.get(&tid) {
            let mut map = store.write();
            if let Some(comp) = map.get_mut(&entity) {
                if let Some(c) = comp.downcast_mut::<C>() {
                    f(c);
                    return true;
                }
            }
        }
        false
    }

    pub fn remove<C: Component>(&self, entity: EntityId) -> bool {
        let tid = TypeId::of::<C>();
        let stores = self.stores.read();
        if let Some(store) = stores.get(&tid) {
            return store.write().remove(&entity).is_some();
        }
        false
    }

    pub fn remove_by_tid(&self, entity: EntityId, tid: TypeId) -> bool {
        let stores = self.stores.read();
        if let Some(store) = stores.get(&tid) {
            return store.write().remove(&entity).is_some();
        }
        false
    }

    pub fn has<C: Component>(&self, entity: EntityId) -> bool {
        let tid = TypeId::of::<C>();
        let stores = self.stores.read();
        if let Some(store) = stores.get(&tid) {
            return store.read().contains_key(&entity);
        }
        false
    }

    /// Remove all components for the given entity across all type stores.
    pub fn remove_all_for(&self, entity: EntityId) {
        let stores = self.stores.read();
        for store in stores.values() {
            store.write().remove(&entity);
        }
    }

    /// Iterate all entities that have component type C, calling f for each.
    pub fn for_each<C: Component, F>(&self, mut f: F)
    where
        F: FnMut(EntityId, &C),
    {
        let tid = TypeId::of::<C>();
        let stores = self.stores.read();
        if let Some(store) = stores.get(&tid) {
            let map = store.read();
            for (eid, comp) in map.iter() {
                if let Some(c) = comp.downcast_ref::<C>() {
                    f(*eid, c);
                }
            }
        }
    }

    /// Collect all entity IDs with component C.
    pub fn entities_with<C: Component>(&self) -> Vec<EntityId> {
        let tid = TypeId::of::<C>();
        let stores = self.stores.read();
        if let Some(store) = stores.get(&tid) {
            return store.read().keys().copied().collect();
        }
        Vec::new()
    }

    /// Count entities with component C.
    pub fn count<C: Component>(&self) -> usize {
        let tid = TypeId::of::<C>();
        let stores = self.stores.read();
        if let Some(store) = stores.get(&tid) {
            return store.read().len();
        }
        0
    }

    /// Snapshot all components for entity as (type_name, json_value) pairs.
    pub fn snapshot_entity(&self, entity: EntityId) -> Vec<(&'static str, serde_json::Value)> {
        let stores = self.stores.read();
        let mut result = Vec::new();
        for store in stores.values() {
            let map = store.read();
            if let Some(comp) = map.get(&entity) {
                result.push((comp.component_type_name(), comp.serialize_json()));
            }
        }
        result
    }

    pub fn clear(&self) {
        let stores = self.stores.read();
        for store in stores.values() {
            store.write().clear();
        }
    }
}

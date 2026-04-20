use crate::entity::EntityId;
use downcast_rs::{impl_downcast, Downcast};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::any::TypeId;
use std::collections::HashMap;

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
        Self {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            rot_x: 0.0,
            rot_y: 0.0,
            rot_z: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            scale_z: 1.0,
        }
    }
}

impl Component for TransformComponent {
    fn component_type_name(&self) -> &'static str {
        "TransformComponent"
    }
    fn clone_box(&self) -> Box<dyn Component> {
        Box::new(self.clone())
    }
    fn serialize_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshComponent {
    pub mesh_id: String,
    pub texture_id: String,
    /// Asset key for normal map texture; `None` means flat normals (vertex normals only).
    #[serde(default)]
    pub normal_map_id: Option<String>,
    /// Asset key for specular map (R=intensity, G=glossiness); `None` uses scalar shininess.
    #[serde(default)]
    pub specular_map_id: Option<String>,
    /// Asset key for emissive map (RGB); `None` means no emissive texture.
    #[serde(default)]
    pub emissive_map_id: Option<String>,
    /// RGBA linear emissive color; default [0,0,0,0] (off). Alpha reserved for bloom threshold.
    #[serde(default = "MeshComponent::default_emissive_color")]
    pub emissive_color: [f32; 4],
    /// Scalar multiplier for emissive; clamped to ≥ 0 at render time. Default 1.0.
    #[serde(default = "MeshComponent::default_emissive_intensity")]
    pub emissive_intensity: f32,
    pub yaw_offset: f32,
    /// Scalar shininess fallback used when `specular_map_id` is `None`.
    #[serde(default = "MeshComponent::default_shininess")]
    pub shininess: f32,
    /// Tint applied to specular highlight; default [1, 1, 1] (white).
    #[serde(default = "MeshComponent::default_specular_color")]
    pub specular_color: [f32; 3],
    pub visible: bool,
    /// PBR metallic hint [0, 1]; 0 = dielectric (default), 1 = metal.
    #[serde(default)]
    pub metallic: f32,
    /// PBR roughness hint [0, 1]; 0 = mirror-smooth, 1 = fully rough (default 0.5).
    #[serde(default = "MeshComponent::default_roughness")]
    pub roughness: f32,
}

impl MeshComponent {
    fn default_shininess() -> f32 {
        32.0
    }
    fn default_specular_color() -> [f32; 3] {
        [1.0, 1.0, 1.0]
    }
    fn default_roughness() -> f32 {
        0.5
    }
    fn default_emissive_color() -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }
    fn default_emissive_intensity() -> f32 {
        1.0
    }
}

impl Default for MeshComponent {
    fn default() -> Self {
        Self {
            mesh_id: String::new(),
            texture_id: String::new(),
            normal_map_id: None,
            specular_map_id: None,
            emissive_map_id: None,
            emissive_color: [0.0, 0.0, 0.0, 0.0],
            emissive_intensity: 1.0,
            yaw_offset: 0.0,
            shininess: 32.0,
            specular_color: [1.0, 1.0, 1.0],
            visible: true,
            metallic: 0.0,
            roughness: 0.5,
        }
    }
}

impl Component for MeshComponent {
    fn component_type_name(&self) -> &'static str {
        "MeshComponent"
    }
    fn clone_box(&self) -> Box<dyn Component> {
        Box::new(self.clone())
    }
    fn serialize_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
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
    fn component_type_name(&self) -> &'static str {
        "BillboardComponent"
    }
    fn clone_box(&self) -> Box<dyn Component> {
        Box::new(self.clone())
    }
    fn serialize_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TagComponent {
    pub tags: Vec<String>,
}

impl Component for TagComponent {
    fn component_type_name(&self) -> &'static str {
        "TagComponent"
    }
    fn clone_box(&self) -> Box<dyn Component> {
        Box::new(self.clone())
    }
    fn serialize_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
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
    fn component_type_name(&self) -> &'static str {
        "RigidBodyComponent"
    }
    fn clone_box(&self) -> Box<dyn Component> {
        Box::new(self.clone())
    }
    fn serialize_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColliderComponent {
    pub shape: String,
    pub size: [f32; 3],
    pub is_trigger: bool,
    #[serde(default)]
    pub restitution: f32,
}

impl Default for ColliderComponent {
    fn default() -> Self {
        Self {
            shape: "box".to_string(),
            size: [1.0, 1.0, 1.0],
            is_trigger: false,
            restitution: 0.0,
        }
    }
}

impl Component for ColliderComponent {
    fn component_type_name(&self) -> &'static str {
        "ColliderComponent"
    }
    fn clone_box(&self) -> Box<dyn Component> {
        Box::new(self.clone())
    }
    fn serialize_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum LightKind {
    Directional {
        direction: [f32; 3],
    },
    Point {
        radius: f32,
    },
    Spot {
        direction: [f32; 3],
        inner_angle: f32,
        outer_angle: f32,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LightComponent {
    pub kind: LightKind,
    pub color: [f32; 3],
    pub intensity: f32,
    pub enabled: bool,
    pub cast_shadows: bool,
}

impl Default for LightComponent {
    fn default() -> Self {
        Self {
            kind: LightKind::Directional {
                direction: [0.5, 1.0, 0.5],
            },
            color: [1.0, 1.0, 1.0],
            intensity: 1.0,
            enabled: true,
            cast_shadows: false,
        }
    }
}

impl Component for LightComponent {
    fn component_type_name(&self) -> &'static str {
        "LightComponent"
    }
    fn clone_box(&self) -> Box<dyn Component> {
        Box::new(self.clone())
    }
    fn serialize_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

// ── Component Storage ────────────────────────────────────────────────────────

/// Per-type storage: entity ID -> boxed component (plain HashMap, no inner lock).
/// All mutation goes through `drain_commands()` at a deterministic point and
/// the engine is effectively single-threaded for component access, so the
/// outer RwLock on `stores` is sufficient.
pub type TypeStore = HashMap<EntityId, Box<dyn Component>>;

/// Global component storage: one TypeStore per component TypeId.
pub struct ComponentStorage {
    stores: RwLock<HashMap<TypeId, TypeStore>>,
}

impl Default for ComponentStorage {
    fn default() -> Self {
        Self {
            stores: RwLock::new(HashMap::new()),
        }
    }
}

impl ComponentStorage {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert<C: Component>(&self, entity: EntityId, component: C) {
        let tid = TypeId::of::<C>();
        let mut write = self.stores.write();
        write
            .entry(tid)
            .or_default()
            .insert(entity, Box::new(component));
    }

    pub fn insert_boxed(&self, entity: EntityId, tid: TypeId, component: Box<dyn Component>) {
        let mut write = self.stores.write();
        write.entry(tid).or_default().insert(entity, component);
    }

    pub fn get<C: Component + Clone>(&self, entity: EntityId) -> Option<C> {
        let tid = TypeId::of::<C>();
        let stores = self.stores.read();
        let store = stores.get(&tid)?;
        let comp = store.get(&entity)?;
        comp.downcast_ref::<C>().cloned()
    }

    pub fn get_ref<C: Component, F, R>(&self, entity: EntityId, f: F) -> Option<R>
    where
        F: FnOnce(&C) -> R,
    {
        let tid = TypeId::of::<C>();
        let stores = self.stores.read();
        let store = stores.get(&tid)?;
        let comp = store.get(&entity)?;
        comp.downcast_ref::<C>().map(f)
    }

    pub fn get_mut<C: Component, F>(&self, entity: EntityId, f: F) -> bool
    where
        F: FnOnce(&mut C),
    {
        let tid = TypeId::of::<C>();
        let mut stores = self.stores.write();
        if let Some(store) = stores.get_mut(&tid) {
            if let Some(comp) = store.get_mut(&entity) {
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
        let mut stores = self.stores.write();
        if let Some(store) = stores.get_mut(&tid) {
            return store.remove(&entity).is_some();
        }
        false
    }

    pub fn remove_by_tid(&self, entity: EntityId, tid: TypeId) -> bool {
        let mut stores = self.stores.write();
        if let Some(store) = stores.get_mut(&tid) {
            return store.remove(&entity).is_some();
        }
        false
    }

    pub fn has<C: Component>(&self, entity: EntityId) -> bool {
        let tid = TypeId::of::<C>();
        let stores = self.stores.read();
        if let Some(store) = stores.get(&tid) {
            return store.contains_key(&entity);
        }
        false
    }

    /// Remove all components for the given entity across all type stores.
    pub fn remove_all_for(&self, entity: EntityId) {
        let mut stores = self.stores.write();
        for store in stores.values_mut() {
            store.remove(&entity);
        }
    }

    /// Iterate all entities that have component type C, calling f for each.
    /// Iteration order is sorted by EntityId so downstream systems (render,
    /// light, physics sync) see a deterministic sequence independent of the
    /// HashMap hash seed.
    pub fn for_each<C: Component, F>(&self, mut f: F)
    where
        F: FnMut(EntityId, &C),
    {
        let tid = TypeId::of::<C>();
        let stores = self.stores.read();
        if let Some(store) = stores.get(&tid) {
            let mut entities: Vec<EntityId> = store.keys().copied().collect();
            entities.sort_unstable();
            for eid in entities {
                if let Some(comp) = store.get(&eid) {
                    if let Some(c) = comp.downcast_ref::<C>() {
                        f(eid, c);
                    }
                }
            }
        }
    }

    /// Collect all entity IDs with component C, sorted by id.
    pub fn entities_with<C: Component>(&self) -> Vec<EntityId> {
        let tid = TypeId::of::<C>();
        let stores = self.stores.read();
        if let Some(store) = stores.get(&tid) {
            let mut out: Vec<EntityId> = store.keys().copied().collect();
            out.sort_unstable();
            return out;
        }
        Vec::new()
    }

    /// Count entities with component C.
    pub fn count<C: Component>(&self) -> usize {
        let tid = TypeId::of::<C>();
        let stores = self.stores.read();
        if let Some(store) = stores.get(&tid) {
            return store.len();
        }
        0
    }

    /// Snapshot all components for entity as (type_name, json_value) pairs.
    /// Output is sorted by component type name so two `save_json` calls on
    /// identical scene state produce byte-identical JSON.
    pub fn snapshot_entity(&self, entity: EntityId) -> Vec<(&'static str, serde_json::Value)> {
        let stores = self.stores.read();
        let mut result = Vec::new();
        for store in stores.values() {
            if let Some(comp) = store.get(&entity) {
                result.push((comp.component_type_name(), comp.serialize_json()));
            }
        }
        result.sort_by_key(|(name, _)| *name);
        result
    }

    pub fn clear(&self) {
        let mut stores = self.stores.write();
        for store in stores.values_mut() {
            store.clear();
        }
    }
}

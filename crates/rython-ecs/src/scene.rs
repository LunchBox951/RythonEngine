use std::any::TypeId;
use std::collections::HashSet;
use parking_lot::RwLock;
use serde_json::{json, Value};

use crate::command::{Command, CommandQueue};
use crate::component::{
    BillboardComponent, ColliderComponent, Component, ComponentStorage, MeshComponent,
    RigidBodyComponent, TagComponent, TransformComponent,
};
use crate::entity::EntityId;
use crate::event_bus::{EventBus, HandlerId};
use crate::hierarchy::Hierarchy;

pub struct Scene {
    /// All live entity IDs.
    entities: RwLock<HashSet<EntityId>>,
    pub components: ComponentStorage,
    pub hierarchy: Hierarchy,
    pub commands: CommandQueue,
    pub events: EventBus,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            entities: RwLock::new(HashSet::new()),
            components: ComponentStorage::new(),
            hierarchy: Hierarchy::new(),
            commands: CommandQueue::new(),
            events: EventBus::new(),
        }
    }
}

impl Scene {
    pub fn new() -> Self { Self::default() }

    // ── Entity queries ───────────────────────────────────────────────────────

    pub fn entity_exists(&self, entity: EntityId) -> bool {
        self.entities.read().contains(&entity)
    }

    pub fn entity_count(&self) -> usize {
        self.entities.read().len()
    }

    pub fn all_entities(&self) -> Vec<EntityId> {
        self.entities.read().iter().copied().collect()
    }

    // ── Command submission ───────────────────────────────────────────────────

    /// Queue a spawn with initial components. Returns a handle that will hold
    /// the new EntityId after drain.
    pub fn queue_spawn(&self, components: Vec<(TypeId, Box<dyn Component>)>) -> SpawnHandle {
        let slot = std::sync::Arc::new(parking_lot::Mutex::new(None));
        self.commands.push(Command::SpawnEntity {
            components,
            result_tx: Some(slot.clone()),
        });
        SpawnHandle(slot)
    }

    /// Queue a spawn without caring about the result ID.
    pub fn queue_spawn_anon(&self, components: Vec<(TypeId, Box<dyn Component>)>) {
        self.commands.push(Command::SpawnEntity { components, result_tx: None });
    }

    pub fn queue_despawn(&self, entity: EntityId) {
        self.commands.push(Command::DespawnEntity { entity });
    }

    pub fn queue_attach<C: Component>(&self, entity: EntityId, component: C) {
        self.commands.push(Command::AttachComponent {
            entity,
            type_id: TypeId::of::<C>(),
            component: Box::new(component),
        });
    }

    pub fn queue_detach<C: Component>(&self, entity: EntityId) {
        self.commands.push(Command::DetachComponent {
            entity,
            type_id: TypeId::of::<C>(),
        });
    }

    pub fn queue_set_parent(&self, child: EntityId, parent: EntityId) {
        self.commands.push(Command::SetParent { child, parent });
    }

    pub fn queue_clear_parent(&self, child: EntityId) {
        self.commands.push(Command::ClearParent { child });
    }

    // ── Immediate (non-queued) operations for internal use ───────────────────

    /// Re-insert an entity with a known ID (for undo/redo restore). Does not
    /// advance the global counter — call `EntityId::ensure_counter_past` after
    /// loading a batch.
    pub fn spawn_with_id(&self, id: EntityId, components: Vec<(TypeId, Box<dyn Component>)>) {
        self.entities.write().insert(id);
        for (tid, comp) in components {
            self.components.insert_boxed(id, tid, comp);
        }
        self.events.emit_entity_spawned(id.0);
    }

    pub fn spawn_immediate(&self, components: Vec<(TypeId, Box<dyn Component>)>) -> EntityId {
        let id = EntityId::next();
        self.entities.write().insert(id);
        for (tid, comp) in components {
            self.components.insert_boxed(id, tid, comp);
        }
        self.events.emit_entity_spawned(id.0);
        id
    }

    pub fn despawn_immediate(&self, entity: EntityId) {
        self.events.emit_entity_despawned(entity.0);
        self.components.remove_all_for(entity);
        self.hierarchy.remove_entity(entity);
        self.entities.write().remove(&entity);
    }

    // ── Drain ────────────────────────────────────────────────────────────────

    /// Apply all pending commands in submission order.
    pub fn drain_commands(&self) {
        let commands = self.commands.drain();
        for cmd in commands {
            match cmd {
                Command::SpawnEntity { components, result_tx } => {
                    let id = self.spawn_immediate(components);
                    if let Some(slot) = result_tx {
                        *slot.lock() = Some(id);
                    }
                }
                Command::DespawnEntity { entity } => {
                    self.despawn_immediate(entity);
                }
                Command::AttachComponent { entity, type_id, component } => {
                    if self.entity_exists(entity) {
                        self.components.insert_boxed(entity, type_id, component);
                    }
                }
                Command::DetachComponent { entity, type_id } => {
                    self.components.remove_by_tid(entity, type_id);
                }
                Command::SetParent { child, parent } => {
                    self.hierarchy.set_parent(child, parent);
                }
                Command::ClearParent { child } => {
                    self.hierarchy.clear_parent(child);
                }
            }
        }
    }

    // ── Event bus convenience wrappers ───────────────────────────────────────

    pub fn subscribe(&self, event_name: &str, handler: impl Fn(&str, &Value) + Send + Sync + 'static) -> HandlerId {
        self.events.subscribe(event_name, handler)
    }

    pub fn unsubscribe(&self, event_name: &str, id: HandlerId) {
        self.events.unsubscribe(event_name, id);
    }

    pub fn emit(&self, event_name: &str, payload: Value) {
        self.events.emit(event_name, &payload);
    }

    // ── Scene serialization ──────────────────────────────────────────────────

    pub fn save_json(&self) -> Value {
        let entities: Vec<EntityId> = self.all_entities();
        let mut entity_records = Vec::new();

        for eid in &entities {
            let components = self.components.snapshot_entity(*eid);
            let parent = self.hierarchy.get_parent(*eid).map(|p| p.0);
            entity_records.push(json!({
                "id": eid.0,
                "parent": parent,
                "components": components.into_iter().map(|(name, val)| json!({
                    "type": name,
                    "data": val,
                })).collect::<Vec<_>>(),
            }));
        }

        json!({ "entities": entity_records })
    }

    pub fn load_json(&self, data: &Value) {
        // Clear existing state
        self.components.clear();
        self.hierarchy.clear();
        self.entities.write().clear();

        let Some(entities) = data["entities"].as_array() else { return };

        // First pass: spawn all entities with components
        {
            let mut entity_set = self.entities.write();
            for record in entities {
                let id = EntityId(record["id"].as_u64().unwrap_or(0));
                entity_set.insert(id);
            }
        }
        // Second: load components (separate pass so entity lock is released)
        for record in entities {
            let id = EntityId(record["id"].as_u64().unwrap_or(0));
            if let Some(comps) = record["components"].as_array() {
                for comp_record in comps {
                    let type_name = comp_record["type"].as_str().unwrap_or("");
                    let data = &comp_record["data"];
                    self.load_component(id, type_name, data);
                }
            }
        }

        // Second pass: restore hierarchy
        for record in entities {
            let child_id = EntityId(record["id"].as_u64().unwrap_or(0));
            if let Some(parent_id) = record["parent"].as_u64() {
                self.hierarchy.set_parent(child_id, EntityId(parent_id));
            }
        }
    }

    pub fn load_component(&self, entity: EntityId, type_name: &str, data: &Value) {
        match type_name {
            "TransformComponent" => {
                if let Ok(c) = serde_json::from_value::<TransformComponent>(data.clone()) {
                    self.components.insert(entity, c);
                }
            }
            "MeshComponent" => {
                if let Ok(c) = serde_json::from_value::<MeshComponent>(data.clone()) {
                    self.components.insert(entity, c);
                }
            }
            "BillboardComponent" => {
                if let Ok(c) = serde_json::from_value::<BillboardComponent>(data.clone()) {
                    self.components.insert(entity, c);
                }
            }
            "TagComponent" => {
                if let Ok(c) = serde_json::from_value::<TagComponent>(data.clone()) {
                    self.components.insert(entity, c);
                }
            }
            "RigidBodyComponent" => {
                if let Ok(c) = serde_json::from_value::<RigidBodyComponent>(data.clone()) {
                    self.components.insert(entity, c);
                }
            }
            "ColliderComponent" => {
                if let Ok(c) = serde_json::from_value::<ColliderComponent>(data.clone()) {
                    self.components.insert(entity, c);
                }
            }
            _ => {}
        }
    }

    pub fn clear(&self) {
        self.components.clear();
        self.hierarchy.clear();
        self.events.clear();
        self.entities.write().clear();
    }
}

/// Handle to receive a spawned entity's ID after drain.
pub struct SpawnHandle(pub std::sync::Arc<parking_lot::Mutex<Option<EntityId>>>);

impl SpawnHandle {
    pub fn get(&self) -> Option<EntityId> {
        *self.0.lock()
    }
}

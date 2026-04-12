use parking_lot::RwLock;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

pub type HandlerId = u64;

/// Handler for named (custom) events.
pub type NamedHandler = Arc<dyn Fn(&str, &Value) + Send + Sync + 'static>;

/// Handler for entity lifecycle events (spawned / despawned).
pub type EntityHandler = Arc<dyn Fn(u64) + Send + Sync + 'static>;

struct Subscription {
    id: HandlerId,
    handler: NamedHandler,
}

pub struct EventBus {
    next_id: std::sync::atomic::AtomicU64,
    /// event_name -> list of subscriptions
    named: RwLock<HashMap<String, Vec<Subscription>>>,
    /// entity_spawned subscribers
    entity_spawned: RwLock<Vec<(HandlerId, EntityHandler)>>,
    /// entity_despawned subscribers
    entity_despawned: RwLock<Vec<(HandlerId, EntityHandler)>>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self {
            next_id: std::sync::atomic::AtomicU64::new(1),
            named: RwLock::new(HashMap::new()),
            entity_spawned: RwLock::new(Vec::new()),
            entity_despawned: RwLock::new(Vec::new()),
        }
    }
}

impl EventBus {
    pub fn new() -> Self {
        Self::default()
    }

    fn alloc_id(&self) -> HandlerId {
        self.next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    /// Subscribe to a named event. Returns HandlerId for unsubscribing.
    pub fn subscribe<F>(&self, event_name: &str, handler: F) -> HandlerId
    where
        F: Fn(&str, &Value) + Send + Sync + 'static,
    {
        let id = self.alloc_id();
        self.named
            .write()
            .entry(event_name.to_string())
            .or_default()
            .push(Subscription {
                id,
                handler: Arc::new(handler),
            });
        id
    }

    pub fn unsubscribe(&self, event_name: &str, handler_id: HandlerId) {
        if let Some(subs) = self.named.write().get_mut(event_name) {
            subs.retain(|s| s.id != handler_id);
        }
    }

    pub fn emit(&self, event_name: &str, payload: &Value) {
        // Snapshot handler Arcs while holding read lock, then drop lock before calling.
        // This prevents deadlock when a handler calls subscribe() (which needs a write lock).
        let named = self.named.read();
        let Some(subs) = named.get(event_name) else {
            return;
        };
        if subs.is_empty() {
            return;
        }
        // Fast path: single handler — avoid Vec allocation
        if subs.len() == 1 {
            let h = Arc::clone(&subs[0].handler);
            drop(named);
            h(event_name, payload);
            return;
        }
        let handlers: Vec<NamedHandler> = subs.iter().map(|s| Arc::clone(&s.handler)).collect();
        drop(named);
        for h in handlers {
            h(event_name, payload);
        }
    }

    pub fn subscribe_entity_spawned<F>(&self, handler: F) -> HandlerId
    where
        F: Fn(u64) + Send + Sync + 'static,
    {
        let id = self.alloc_id();
        self.entity_spawned.write().push((id, Arc::new(handler)));
        id
    }

    pub fn unsubscribe_entity_spawned(&self, handler_id: HandlerId) {
        self.entity_spawned
            .write()
            .retain(|(id, _)| *id != handler_id);
    }

    pub fn emit_entity_spawned(&self, entity_id: u64) {
        let subs = self.entity_spawned.read();
        if subs.is_empty() {
            return;
        }
        // Fast path: single handler — avoid Vec allocation
        if subs.len() == 1 {
            let h = Arc::clone(&subs[0].1);
            drop(subs);
            h(entity_id);
            return;
        }
        let handlers: Vec<Arc<dyn Fn(u64) + Send + Sync + 'static>> =
            subs.iter().map(|(_, h)| Arc::clone(h)).collect();
        drop(subs);
        for h in handlers {
            h(entity_id);
        }
    }

    pub fn subscribe_entity_despawned<F>(&self, handler: F) -> HandlerId
    where
        F: Fn(u64) + Send + Sync + 'static,
    {
        let id = self.alloc_id();
        self.entity_despawned.write().push((id, Arc::new(handler)));
        id
    }

    pub fn unsubscribe_entity_despawned(&self, handler_id: HandlerId) {
        self.entity_despawned
            .write()
            .retain(|(id, _)| *id != handler_id);
    }

    pub fn emit_entity_despawned(&self, entity_id: u64) {
        let subs = self.entity_despawned.read();
        if subs.is_empty() {
            return;
        }
        // Fast path: single handler — avoid Vec allocation
        if subs.len() == 1 {
            let h = Arc::clone(&subs[0].1);
            drop(subs);
            h(entity_id);
            return;
        }
        let handlers: Vec<Arc<dyn Fn(u64) + Send + Sync + 'static>> =
            subs.iter().map(|(_, h)| Arc::clone(h)).collect();
        drop(subs);
        for h in handlers {
            h(entity_id);
        }
    }

    pub fn clear(&self) {
        self.named.write().clear();
        self.entity_spawned.write().clear();
        self.entity_despawned.write().clear();
    }
}

use crate::component::Component;
use crate::entity::EntityId;
use parking_lot::Mutex;
use std::any::TypeId;

/// All mutations go through commands, drained at a deterministic frame point.
pub enum Command {
    SpawnEntity {
        components: Vec<(TypeId, Box<dyn Component>)>,
        /// Callback to receive the spawned entity ID.
        /// None if caller doesn't care.
        result_tx: Option<std::sync::Arc<Mutex<Option<EntityId>>>>,
    },
    DespawnEntity {
        entity: EntityId,
    },
    AttachComponent {
        entity: EntityId,
        type_id: TypeId,
        component: Box<dyn Component>,
    },
    DetachComponent {
        entity: EntityId,
        type_id: TypeId,
    },
    SetParent {
        child: EntityId,
        parent: EntityId,
    },
    ClearParent {
        child: EntityId,
    },
}

pub struct CommandQueue {
    queue: Mutex<Vec<Command>>,
}

impl Default for CommandQueue {
    fn default() -> Self {
        Self {
            queue: Mutex::new(Vec::new()),
        }
    }
}

impl CommandQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&self, cmd: Command) {
        self.queue.lock().push(cmd);
    }

    /// Drain all pending commands, returning them in submission order.
    pub fn drain(&self) -> Vec<Command> {
        std::mem::take(&mut *self.queue.lock())
    }

    pub fn len(&self) -> usize {
        self.queue.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.lock().is_empty()
    }
}

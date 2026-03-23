use std::sync::atomic::{AtomicU64, Ordering};

/// Lightweight entity handle — just a monotonic numeric ID.
/// IDs are never reused within a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct EntityId(pub u64);

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

impl EntityId {
    pub fn next() -> Self {
        EntityId(NEXT_ID.fetch_add(1, Ordering::Relaxed))
    }

    /// Reset counter — only for tests that need a clean slate.
    #[cfg(test)]
    pub fn reset_counter(val: u64) {
        NEXT_ID.store(val, Ordering::SeqCst);
    }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Entity({})", self.0)
    }
}

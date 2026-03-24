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

    /// Advance the global counter to be at least `val + 1`.
    /// Called after loading a scene to prevent ID collisions.
    pub fn ensure_counter_past(val: u64) {
        NEXT_ID.fetch_max(val + 1, Ordering::SeqCst);
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

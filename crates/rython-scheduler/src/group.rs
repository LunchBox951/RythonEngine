use rython_core::{EngineError, GroupId, OwnerId};
use std::any::Any;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

type BgResult = Result<Box<dyn Any + Send + 'static>, EngineError>;
type GroupCallback = Box<dyn FnOnce(Vec<BgResult>) -> Result<(), EngineError> + Send + 'static>;

/// Tracks the state shared between the group and its member tasks.
pub struct GroupState {
    pub id: GroupId,
    pub owner: OwnerId,
    pub remaining: AtomicUsize,
    pub sealed: AtomicBool,
    pub results: Mutex<Vec<BgResult>>,
    pub callback: Mutex<Option<GroupCallback>>,
}

impl GroupState {
    pub fn new(id: GroupId, owner: OwnerId, callback: GroupCallback) -> Arc<Self> {
        Arc::new(Self {
            id,
            owner,
            remaining: AtomicUsize::new(0),
            sealed: AtomicBool::new(false),
            results: Mutex::new(Vec::new()),
            callback: Mutex::new(Some(callback)),
        })
    }

    /// Called when a member completes. Returns true if the group is now done
    /// (sealed and all members completed) and the callback should fire.
    pub fn member_complete(&self, result: BgResult) -> bool {
        {
            let mut results = self.results.lock().unwrap();
            results.push(result);
        }
        let prev = self.remaining.fetch_sub(1, Ordering::AcqRel);
        let now_zero = prev == 1;
        now_zero && self.sealed.load(Ordering::Acquire)
    }

    /// Add a member before sealing. Returns the new count.
    pub fn add_member(&self) -> usize {
        self.remaining.fetch_add(1, Ordering::AcqRel) + 1
    }

    /// Seal the group. Returns true if already at zero (callback should fire immediately).
    pub fn seal(&self) -> bool {
        self.sealed.store(true, Ordering::Release);
        self.remaining.load(Ordering::Acquire) == 0
    }

    /// Take the callback and results for firing.
    pub fn take_callback_and_results(&self) -> Option<(GroupCallback, Vec<BgResult>)> {
        let cb = self.callback.lock().unwrap().take()?;
        let results = std::mem::take(&mut *self.results.lock().unwrap());
        Some((cb, results))
    }
}

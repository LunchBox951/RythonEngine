use rython_core::{EngineError, GroupId, OwnerId, Priority, TaskId};
use std::any::Any;

/// The type-erased result produced by a background task.
pub type BgResult = Result<Box<dyn Any + Send + 'static>, EngineError>;

/// Erased body of a background task.
pub type BgTaskFn = Box<dyn FnOnce() -> BgResult + Send + 'static>;

/// Callback invoked once a single background task completes.
pub type BgCallback = Box<dyn FnOnce(BgResult) -> Result<(), EngineError> + Send + 'static>;

/// Callback invoked once all members of a task group have completed.
pub type GroupCallback = Box<dyn FnOnce(Vec<BgResult>) -> Result<(), EngineError> + Send + 'static>;

/// A one-shot sequential task to run on the main thread.
pub struct SequentialTask {
    pub id: TaskId,
    pub owner: OwnerId,
    pub priority: Priority,
    pub f: Box<dyn FnOnce() -> Result<(), EngineError> + Send + 'static>,
}

/// A one-shot parallel task to run on the rayon thread pool.
pub struct ParallelTask {
    pub id: TaskId,
    pub owner: OwnerId,
    pub priority: Priority,
    pub f: Box<dyn Fn() -> Result<(), EngineError> + Send + Sync + 'static>,
}

/// A recurring sequential task. Returns true to keep running, false to stop.
pub struct RecurringTask {
    pub id: TaskId,
    pub owner: OwnerId,
    pub priority: Priority,
    pub f: Box<dyn FnMut() -> bool + Send + 'static>,
}

/// A background (fire-and-forget) task submitted to the thread pool.
pub struct BackgroundTask {
    pub id: TaskId,
    pub owner: OwnerId,
    pub priority: Priority,
}

/// Result sent back from a completed background task.
pub struct BgComplete {
    pub task_id: TaskId,
    pub owner: OwnerId,
    pub result: BgResult,
    pub callback: Option<BgCallback>,
    pub group_id: Option<GroupId>,
}

/// A task submitted remotely (from another thread via the channel).
pub struct RemoteTask {
    pub owner: OwnerId,
    pub priority: Priority,
    pub f: Box<dyn FnOnce() -> Result<(), EngineError> + Send + 'static>,
}

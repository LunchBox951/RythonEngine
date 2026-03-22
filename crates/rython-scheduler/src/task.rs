use rython_core::{EngineError, OwnerId, Priority, TaskId};
use std::any::Any;

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
    pub result: Result<Box<dyn Any + Send + 'static>, EngineError>,
    pub callback:
        Option<Box<dyn FnOnce(Result<Box<dyn Any + Send + 'static>, EngineError>) -> Result<(), EngineError> + Send + 'static>>,
    pub group_id: Option<rython_core::GroupId>,
}

/// A task submitted remotely (from another thread via the channel).
pub struct RemoteTask {
    pub owner: OwnerId,
    pub priority: Priority,
    pub f: Box<dyn FnOnce() -> Result<(), EngineError> + Send + 'static>,
}

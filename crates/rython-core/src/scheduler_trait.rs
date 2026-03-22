use crate::{
    errors::EngineError,
    types::{OwnerId, Priority},
};

/// Trait object for submitting tasks to the scheduler.
/// Defined in rython-core so that rython-modules can use it without
/// depending on rython-scheduler (preserving Layer 1 independence).
pub trait SchedulerHandle: Send + Sync {
    /// Submit a one-shot sequential task (runs on the main thread).
    fn submit_sequential(
        &self,
        f: Box<dyn FnOnce() -> Result<(), EngineError> + Send + 'static>,
        priority: Priority,
        owner: OwnerId,
    );

    /// Cancel all pending tasks for the given owner.
    fn cancel_owned(&self, owner: OwnerId);
}

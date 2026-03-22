use downcast_rs::{impl_downcast, Downcast};
use rython_core::{EngineError, SchedulerHandle};

/// Trait implemented by every engine system that participates in the lifecycle.
pub trait Module: Downcast + Send + Sync + 'static {
    /// Human-readable name, used as the registry key.
    fn name(&self) -> &str;

    /// Names of modules this module depends on. Must be loaded before this module.
    fn dependencies(&self) -> Vec<String> {
        Vec::new()
    }

    /// Called when the module is being loaded. May submit tasks to the scheduler.
    fn on_load(&mut self, scheduler: &dyn SchedulerHandle) -> Result<(), EngineError>;

    /// Called when the module is being unloaded. May submit cleanup tasks.
    fn on_unload(&mut self, scheduler: &dyn SchedulerHandle) -> Result<(), EngineError>;

    /// Returns true if this module only allows a single owner at a time.
    fn is_exclusive(&self) -> bool {
        false
    }
}

impl_downcast!(Module);

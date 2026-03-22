/// Module lifecycle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleState {
    /// on_load() has been called; initialization tasks are running.
    Loading,
    /// Module is fully initialized and ready for use.
    Loaded,
    /// on_unload() has been called; teardown is in progress.
    Unloading,
}

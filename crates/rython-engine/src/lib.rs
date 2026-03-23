#![deny(warnings)]

pub mod builder;

pub use builder::EngineBuilder;

use std::sync::Arc;

use rython_core::EngineError;
use rython_ecs::Scene;
use rython_modules::ModuleLoader;
use rython_scheduler::{RemoteSender, TaskScheduler};

/// The assembled engine. Construct with [`EngineBuilder`].
///
/// ```no_run
/// let mut engine = rython_engine::EngineBuilder::new().build().unwrap();
/// engine.boot().unwrap();
/// engine.run_headless(60).unwrap();
/// engine.shutdown().unwrap();
/// ```
pub struct Engine {
    pub(crate) scheduler: TaskScheduler,
    pub(crate) loader: ModuleLoader,
    pub(crate) scene: Arc<Scene>,
}

impl Engine {
    /// Create a new [`EngineBuilder`].
    pub fn builder() -> EngineBuilder {
        EngineBuilder::new()
    }

    /// Load all registered modules in dependency order.
    pub fn boot(&mut self) -> Result<(), EngineError> {
        let sender = self.scheduler.remote_sender();
        self.loader.load_all(&sender)
    }

    /// Unload all modules in reverse load order.
    pub fn shutdown(&mut self) -> Result<(), EngineError> {
        let sender = self.scheduler.remote_sender();
        self.loader.unload_all(&sender)
    }

    /// Execute one frame tick: drain remote submissions, run sequential/parallel/
    /// background phases, then pace to target FPS.
    pub fn tick(&mut self) -> Result<(), EngineError> {
        self.scheduler.tick()
    }

    /// Run `frames` headless ticks without a platform event loop.
    pub fn run_headless(&mut self, frames: usize) -> Result<(), EngineError> {
        for _ in 0..frames {
            self.tick()?;
        }
        Ok(())
    }

    /// Shared ECS scene.
    pub fn scene(&self) -> &Arc<Scene> {
        &self.scene
    }

    /// Mutable access to the underlying task scheduler.
    pub fn scheduler(&mut self) -> &mut TaskScheduler {
        &mut self.scheduler
    }

    /// A cloneable handle for cross-thread task submission.
    pub fn remote_sender(&self) -> RemoteSender {
        self.scheduler.remote_sender()
    }
}

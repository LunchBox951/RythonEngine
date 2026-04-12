use rython_core::{EngineConfig, EngineError};
use rython_ecs::Scene;
use rython_modules::{Module, ModuleLoader};
use rython_scheduler::TaskScheduler;
use std::sync::Arc;

use crate::Engine;

/// Builder for constructing an [`Engine`] instance.
///
/// Use [`Engine::builder()`] or [`EngineBuilder::new()`] as entry points.
pub struct EngineBuilder {
    config: EngineConfig,
    modules: Vec<Box<dyn Module>>,
    scene: Option<Arc<Scene>>,
}

impl EngineBuilder {
    pub fn new() -> Self {
        Self {
            config: EngineConfig::default(),
            modules: Vec::new(),
            scene: None,
        }
    }

    /// Override the engine configuration programmatically.
    pub fn with_config(mut self, config: EngineConfig) -> Self {
        self.config = config;
        self
    }

    /// Load engine configuration from a JSON file.
    /// Falls back to the default config if the file cannot be read or parsed.
    pub fn with_config_file(mut self, path: &str) -> Self {
        if let Ok(contents) = std::fs::read_to_string(path) {
            if let Ok(config) = serde_json::from_str::<EngineConfig>(&contents) {
                self.config = config;
            }
        }
        self
    }

    /// Register a module. Only registered modules are loaded on boot.
    /// Omitting a module is the feature-flag mechanism for disabling it.
    pub fn add_module(mut self, module: Box<dyn Module>) -> Self {
        self.modules.push(module);
        self
    }

    /// Share an externally created scene with the engine (e.g. for scripting modules that
    /// need the same Arc<Scene> instance before build() creates it).
    pub fn with_scene(mut self, scene: Arc<Scene>) -> Self {
        self.scene = Some(scene);
        self
    }

    /// Consume the builder and produce a ready-to-[`boot`](Engine::boot) [`Engine`].
    ///
    /// Returns `Err` if the scheduler configuration is invalid (e.g.
    /// `target_fps == 0`, which would previously panic inside `FramePacer`).
    pub fn build(self) -> Result<Engine, EngineError> {
        // Validate the scheduler config up front so we can surface configuration
        // errors as an EngineError instead of panicking in the scheduler
        // constructor.
        if self.config.scheduler.target_fps == 0 {
            return Err(EngineError::Config(
                "scheduler.target_fps must be at least 1".to_string(),
            ));
        }
        let scheduler = TaskScheduler::new(&self.config.scheduler);
        let scene = self.scene.unwrap_or_else(|| Arc::new(Scene::new()));
        let mut loader = ModuleLoader::new();
        for module in self.modules {
            loader.register(module, None);
        }
        Ok(Engine {
            scheduler,
            loader,
            scene,
        })
    }
}

impl Default for EngineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

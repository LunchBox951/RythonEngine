use rython_core::{EngineError, SchedulerHandle, WindowConfig};
use rython_modules::Module;
use std::sync::{Arc, Mutex};
use winit::window::WindowAttributes;

/// Window module: holds window configuration and collects raw input events.
/// The platform event loop pushes events here; the input system drains them each frame.
pub struct WindowModule {
    config: WindowConfig,
    event_queue: Arc<Mutex<Vec<super::raw_events::RawInputEvent>>>,
}

impl WindowModule {
    pub fn new(config: WindowConfig) -> Self {
        Self {
            config,
            event_queue: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn config(&self) -> &WindowConfig {
        &self.config
    }

    /// Build winit WindowAttributes from the stored config.
    pub fn window_attributes(&self) -> WindowAttributes {
        WindowAttributes::default()
            .with_title(self.config.title.clone())
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.config.width,
                self.config.height,
            ))
    }

    /// Push a raw input event (called by the platform event loop).
    pub fn push_event(&self, event: super::raw_events::RawInputEvent) {
        self.event_queue.lock().unwrap().push(event);
    }

    /// Drain all pending events (called by the input system each frame).
    pub fn drain_events(&self) -> Vec<super::raw_events::RawInputEvent> {
        let mut q = self.event_queue.lock().unwrap();
        std::mem::take(&mut *q)
    }

    /// Clone the event queue handle for external pushers.
    pub fn event_sender(&self) -> Arc<Mutex<Vec<super::raw_events::RawInputEvent>>> {
        Arc::clone(&self.event_queue)
    }
}

impl Module for WindowModule {
    fn name(&self) -> &str {
        "WindowModule"
    }

    fn dependencies(&self) -> Vec<String> {
        Vec::new()
    }

    fn on_load(&mut self, _scheduler: &dyn SchedulerHandle) -> Result<(), EngineError> {
        Ok(())
    }

    fn on_unload(&mut self, _scheduler: &dyn SchedulerHandle) -> Result<(), EngineError> {
        Ok(())
    }
}

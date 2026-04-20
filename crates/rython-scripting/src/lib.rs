#![deny(warnings)]

pub mod bridge;
pub mod component;
pub mod config;
pub mod system;

pub use bridge::{
    call_entry_point, clear_recurring_callbacks, dispatch_input_events, drain_draw_commands,
    drain_ui_draw_commands, ensure_rython_module, flush_python_bg_completions,
    flush_python_bg_tasks, flush_python_par_tasks, flush_python_seq_tasks,
    flush_recurring_callbacks, flush_timers, get_scene_settings, get_script_class, load_bundle,
    register_script_class, reset_quit_requested, set_active_audio, set_active_input,
    set_active_physics, set_active_player_controller, set_active_resources, set_active_scene,
    set_active_ui, set_elapsed_secs, was_quit_requested, CameraPy, EntityPy, JobHandlePy,
    TransformPy, Vec3Py,
};
pub use component::ScriptComponent;
pub use config::ScriptingConfig;
pub use system::{gil_dispatch_count, reset_gil_dispatch_count, ScriptEvent, ScriptSystem};

use std::sync::Arc;

use pyo3::prelude::*;
use rython_core::{EngineError, SchedulerHandle};
use rython_ecs::Scene;
use rython_modules::Module;

// ─── ScriptingModule ──────────────────────────────────────────────────────────

/// Engine module that owns the Python interpreter lifecycle and ScriptSystem.
pub struct ScriptingModule {
    config: ScriptingConfig,
    scene: Arc<Scene>,
    system: Option<Arc<ScriptSystem>>,
}

impl ScriptingModule {
    pub fn new(config: ScriptingConfig, scene: Arc<Scene>) -> Self {
        Self {
            config,
            scene,
            system: None,
        }
    }

    pub fn system(&self) -> Option<&Arc<ScriptSystem>> {
        self.system.as_ref()
    }
}

impl Module for ScriptingModule {
    fn name(&self) -> &str {
        "scripting"
    }

    fn on_load(&mut self, _scheduler: &dyn SchedulerHandle) -> Result<(), EngineError> {
        let sys = ScriptSystem::new(Arc::clone(&self.scene));
        self.system = Some(Arc::clone(&sys));

        Python::attach(|py| {
            ensure_rython_module(py, Arc::clone(&self.scene)).map_err(|e| {
                EngineError::Script(rython_core::ScriptError::PythonException {
                    script: "scripting".to_string(),
                    exception: e.to_string(),
                })
            })?;

            match &self.config {
                ScriptingConfig::Dev {
                    script_dir,
                    entry_point,
                } => {
                    let path_code = format!(
                        "import sys; sys.path.insert(0, '{}')",
                        script_dir.replace('\'', "\\'")
                    );
                    let path_cstr = std::ffi::CString::new(path_code)
                        .map_err(|e| EngineError::Config(e.to_string()))?;
                    py.run(path_cstr.as_c_str(), None, None).map_err(|e| {
                        EngineError::Script(rython_core::ScriptError::PythonException {
                            script: "scripting".to_string(),
                            exception: e.to_string(),
                        })
                    })?;

                    if let Some(ep) = entry_point {
                        call_entry_point(py, ep).map_err(|e| {
                            EngineError::Script(rython_core::ScriptError::PythonException {
                                script: ep.clone(),
                                exception: e.to_string(),
                            })
                        })?;
                    }
                }
                ScriptingConfig::Release {
                    bundle_path,
                    entry_point,
                } => {
                    load_bundle(py, bundle_path).map_err(|e| {
                        EngineError::Script(rython_core::ScriptError::PythonException {
                            script: "bundle".to_string(),
                            exception: e.to_string(),
                        })
                    })?;
                    if let Some(ep) = entry_point {
                        call_entry_point(py, ep).map_err(|e| {
                            EngineError::Script(rython_core::ScriptError::PythonException {
                                script: ep.clone(),
                                exception: e.to_string(),
                            })
                        })?;
                    }
                }
            }

            Ok::<(), EngineError>(())
        })?;

        Ok(())
    }

    fn on_unload(&mut self, _scheduler: &dyn SchedulerHandle) -> Result<(), EngineError> {
        self.system = None;
        Ok(())
    }
}

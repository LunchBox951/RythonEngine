use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use rython_audio::{AudioCategory, AudioManager, PlayRequest, PlaybackHandle};

static AUDIO_MANAGER: OnceLock<Arc<Mutex<AudioManager>>> = OnceLock::new();

fn audio_store() -> &'static Arc<Mutex<AudioManager>> {
    AUDIO_MANAGER.get_or_init(|| Arc::new(Mutex::new(AudioManager::with_default_config())))
}

/// Share the engine AudioManager with the Python bridge.
/// Must be called before ensure_rython_module().
pub fn set_active_audio(manager: Arc<Mutex<AudioManager>>) {
    let _ = AUDIO_MANAGER.set(manager);
}

// ─── Audio bridge ─────────────────────────────────────────────────────────────

#[pyclass(name = "AudioBridge")]
pub struct AudioBridge {}

#[pymethods]
impl AudioBridge {
    #[pyo3(signature = (path, category = "sfx", looping = false))]
    fn play(&self, path: &str, category: &str, looping: bool) -> PyResult<u64> {
        let cat = AudioCategory::from_name(category).ok_or_else(|| {
            PyErr::new::<PyRuntimeError, _>(format!("Unknown audio category: {category}"))
        })?;
        let request = PlayRequest {
            path: path.to_string(),
            category: cat,
            position: None,
            looping,
        };
        let handle = audio_store()
            .lock()
            .play(request)
            .map_err(|e| PyErr::new::<PyRuntimeError, _>(e.to_string()))?;
        Ok(handle.id())
    }

    fn stop(&self, handle: u64) -> PyResult<()> {
        audio_store()
            .lock()
            .stop(PlaybackHandle::from_raw(handle))
            .map_err(|e| PyErr::new::<PyRuntimeError, _>(e.to_string()))
    }

    fn stop_category(&self, category: &str) -> PyResult<()> {
        audio_store()
            .lock()
            .stop_category(category)
            .map_err(|e| PyErr::new::<PyRuntimeError, _>(e.to_string()))
    }

    fn set_volume(&self, category: &str, volume: f32) -> PyResult<()> {
        audio_store()
            .lock()
            .set_volume(category, volume)
            .map_err(|e| PyErr::new::<PyRuntimeError, _>(e.to_string()))
    }

    fn set_master_volume(&self, volume: f32) {
        audio_store().lock().set_master_volume(volume);
    }

    fn __repr__(&self) -> String {
        "rython.audio".to_string()
    }
}

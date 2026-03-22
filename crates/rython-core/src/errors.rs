use thiserror::Error;

/// Layer 3: Errors originating from Python game scripts.
#[derive(Debug, Error)]
pub enum ScriptError {
    #[error("Python exception in {script}: {exception}")]
    PythonException { script: String, exception: String },

    #[error("Script not found: {name}")]
    NotFound { name: String },

    #[error("Reload failed for {path}: {reason}")]
    ReloadFailed { path: String, reason: String },
}

/// Layer 2: Errors that occur during task execution.
#[derive(Debug, Error)]
pub enum TaskError {
    #[error("Task panicked: {message}")]
    Panicked { message: String },

    #[error("Task was cancelled")]
    Cancelled,

    #[error("Task timed out")]
    TimedOut,

    #[error("Task failed: {source}")]
    Failed {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

/// Layer 1: Top-level engine error type. Every function that can fail returns Result<T, EngineError>.
#[derive(Debug, Error)]
pub enum EngineError {
    #[error("Task error: {0}")]
    Task(#[from] TaskError),

    #[error("Module error in {module}: {message}")]
    Module { module: String, message: String },

    #[error("Resource error: {0}")]
    Resource(String),

    #[error("Renderer error: {0}")]
    Renderer(String),

    #[error("Physics error: {0}")]
    Physics(String),

    #[error("Audio error: {0}")]
    Audio(String),

    #[error("Script error: {0}")]
    Script(#[from] ScriptError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Config error: {0}")]
    Config(String),
}

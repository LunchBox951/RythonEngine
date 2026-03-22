use rython_core::{EngineError, ScriptError, TaskError};

// T-ERR-01: EngineError Wraps TaskError
#[test]
fn t_err_01_engine_error_wraps_task_error() {
    let source: Box<dyn std::error::Error + Send + Sync + 'static> =
        Box::new(std::io::Error::new(std::io::ErrorKind::Other, "test failure"));

    let task_err = TaskError::Failed { source };
    let engine_err: EngineError = task_err.into();

    // Variant must be Task
    assert!(matches!(engine_err, EngineError::Task(_)));

    // to_string() must contain the inner message
    let msg = engine_err.to_string();
    assert!(msg.contains("test failure"), "expected 'test failure' in: {msg}");

    // The original error is accessible via std::error::Error::source()
    use std::error::Error;
    if let EngineError::Task(ref te) = engine_err {
        assert!(te.source().is_some(), "TaskError::Failed should have a source");
    }
}

// T-ERR-02: EngineError Wraps ScriptError
#[test]
fn t_err_02_engine_error_wraps_script_error() {
    let script_err = ScriptError::PythonException {
        script: "player.py".to_string(),
        exception: "NameError: x".to_string(),
    };
    let engine_err: EngineError = script_err.into();

    assert!(matches!(engine_err, EngineError::Script(_)));

    let msg = engine_err.to_string();
    assert!(msg.contains("player.py"), "expected 'player.py' in: {msg}");
    assert!(msg.contains("NameError"), "expected 'NameError' in: {msg}");
}

// T-ERR-05: ScriptError captures script name and exception text
#[test]
fn t_err_05_script_error_python_exception() {
    let err = ScriptError::PythonException {
        script: "enemy.py".to_string(),
        exception: "AttributeError: 'CollisionEvent' has no attribute 'damage'\n  line 15".to_string(),
    };
    let msg = err.to_string();
    assert!(msg.contains("enemy.py"), "expected 'enemy.py' in: {msg}");
    assert!(msg.contains("AttributeError"), "expected 'AttributeError' in: {msg}");
    assert!(msg.contains("line 15"), "expected 'line 15' in: {msg}");
}

// T-ERR-06: ScriptError::NotFound
#[test]
fn t_err_06_script_error_not_found() {
    let err = ScriptError::NotFound {
        name: "nonexistent_module".to_string(),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("nonexistent_module"),
        "expected 'nonexistent_module' in: {msg}"
    );
}

// T-ERR-07: ScriptError::ReloadFailed
#[test]
fn t_err_07_script_error_reload_failed() {
    let err = ScriptError::ReloadFailed {
        path: "scripts/enemy.py".to_string(),
        reason: "SyntaxError: invalid syntax at line 5".to_string(),
    };
    let msg = err.to_string();
    assert!(msg.contains("scripts/enemy.py"), "expected file path in: {msg}");
    assert!(msg.contains("SyntaxError"), "expected error description in: {msg}");
}

// T-ERR-08: Error propagation chain — Python to Engine
#[test]
fn t_err_08_error_propagation_chain() {
    // Layer 3: ScriptError
    let script_err = ScriptError::PythonException {
        script: "handler.py".to_string(),
        exception: "ValueError: bad value".to_string(),
    };

    // Layer 2: TaskError wraps EngineError::Script wrapping ScriptError
    let engine_with_script: EngineError = ScriptError::PythonException {
        script: "handler.py".to_string(),
        exception: "ValueError: bad value".to_string(),
    }
    .into();

    let task_err = TaskError::Failed {
        source: Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            engine_with_script.to_string(),
        )),
    };

    // Layer 1: EngineError::Task wraps TaskError
    let engine_err: EngineError = task_err.into();
    assert!(matches!(engine_err, EngineError::Task(_)));

    // The script_err also converts cleanly to EngineError::Script
    let script_engine_err: EngineError = script_err.into();
    assert!(matches!(script_engine_err, EngineError::Script(_)));
    let msg = script_engine_err.to_string();
    assert!(msg.contains("handler.py"));
    assert!(msg.contains("ValueError"));
}

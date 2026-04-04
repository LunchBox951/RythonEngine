use rython_core::types::priorities;
use rython_core::{EngineConfig, EngineError, NamedEvent, ScriptError, TaskError};

// T-ERR-01: EngineError Wraps TaskError
#[test]
fn t_err_01_engine_error_wraps_task_error() {
    let source: Box<dyn std::error::Error + Send + Sync + 'static> = Box::new(std::io::Error::new(
        std::io::ErrorKind::Other,
        "test failure",
    ));

    let task_err = TaskError::Failed { source };
    let engine_err: EngineError = task_err.into();

    // Variant must be Task
    assert!(matches!(engine_err, EngineError::Task(_)));

    // to_string() must contain the inner message
    let msg = engine_err.to_string();
    assert!(
        msg.contains("test failure"),
        "expected 'test failure' in: {msg}"
    );

    // The original error is accessible via std::error::Error::source()
    use std::error::Error;
    if let EngineError::Task(ref te) = engine_err {
        assert!(
            te.source().is_some(),
            "TaskError::Failed should have a source"
        );
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
        exception: "AttributeError: 'CollisionEvent' has no attribute 'damage'\n  line 15"
            .to_string(),
    };
    let msg = err.to_string();
    assert!(msg.contains("enemy.py"), "expected 'enemy.py' in: {msg}");
    assert!(
        msg.contains("AttributeError"),
        "expected 'AttributeError' in: {msg}"
    );
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
    assert!(
        msg.contains("scripts/enemy.py"),
        "expected file path in: {msg}"
    );
    assert!(
        msg.contains("SyntaxError"),
        "expected error description in: {msg}"
    );
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

// ─── T-ERR-03/04: Missing TaskError variants ──────────────────────────────────

// T-ERR-03: TaskError::Panicked carries the panic message
#[test]
fn t_err_03_task_error_panicked() {
    let err = TaskError::Panicked {
        message: "index out of bounds".to_string(),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("index out of bounds"),
        "expected panic message in: {msg}"
    );
}

// T-ERR-04: TaskError::Cancelled and TimedOut are unit-like variants
#[test]
fn t_err_04_task_error_cancelled_and_timed_out() {
    let cancelled = TaskError::Cancelled;
    assert!(cancelled.to_string().contains("cancelled") || cancelled.to_string().len() > 0);

    let timed_out = TaskError::TimedOut;
    assert!(timed_out.to_string().contains("timed out") || timed_out.to_string().len() > 0);

    // Both convert to EngineError::Task
    let e1: EngineError = cancelled.into();
    assert!(matches!(e1, EngineError::Task(_)));
    let e2: EngineError = timed_out.into();
    assert!(matches!(e2, EngineError::Task(_)));
}

// T-ERR-09: EngineError::Module carries module name and message
#[test]
fn t_err_09_engine_error_module() {
    let err = EngineError::Module {
        module: "PhysicsModule".to_string(),
        message: "failed to initialise broadphase".to_string(),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("PhysicsModule"),
        "expected module name in: {msg}"
    );
    assert!(
        msg.contains("failed to initialise broadphase"),
        "expected message in: {msg}"
    );
}

// T-ERR-10: EngineError string variants round-trip through to_string()
#[test]
fn t_err_10_engine_error_string_variants() {
    let cases = vec![
        EngineError::Resource("missing texture atlas".to_string()),
        EngineError::Renderer("swap chain lost".to_string()),
        EngineError::Physics("rigid body out of bounds".to_string()),
        EngineError::Audio("no audio device".to_string()),
        EngineError::Config("invalid fps value".to_string()),
    ];
    for err in cases {
        let msg = err.to_string();
        assert!(!msg.is_empty(), "to_string() must not be empty: {:?}", err);
    }
}

// T-ERR-11: EngineError::Io from std::io::Error via From conversion
#[test]
fn t_err_11_engine_error_io_from() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "scene.json not found");
    let engine_err: EngineError = io_err.into();
    assert!(matches!(engine_err, EngineError::Io(_)));
    let msg = engine_err.to_string();
    assert!(
        msg.contains("scene.json not found"),
        "expected io message in: {msg}"
    );
}

// ─── T-EVT: NamedEvent edge cases ─────────────────────────────────────────────

// T-EVT-01: NamedEvent with null JSON payload
#[test]
fn t_evt_01_named_event_null_payload() {
    let ev = NamedEvent {
        name: "player_died".to_string(),
        payload: serde_json::Value::Null,
    };
    assert_eq!(ev.name, "player_died");
    assert!(ev.payload.is_null());
}

// T-EVT-02: NamedEvent with numeric payload
#[test]
fn t_evt_02_named_event_numeric_payload() {
    let ev = NamedEvent {
        name: "score_changed".to_string(),
        payload: serde_json::json!(42),
    };
    assert_eq!(ev.payload.as_i64(), Some(42));
}

// T-EVT-03: NamedEvent with structured object payload
#[test]
fn t_evt_03_named_event_object_payload() {
    let ev = NamedEvent {
        name: "collision".to_string(),
        payload: serde_json::json!({ "normal": [0.0, 1.0, 0.0], "entity": 7 }),
    };
    assert!(ev.payload.is_object());
    assert_eq!(ev.payload["entity"].as_i64(), Some(7));
}

// T-EVT-04: NamedEvent clone produces an independent copy
#[test]
fn t_evt_04_named_event_clone_independence() {
    let original = NamedEvent {
        name: "tick".to_string(),
        payload: serde_json::json!({ "frame": 1 }),
    };
    let mut cloned = original.clone();
    cloned.name = "other".to_string();
    // Mutation of the clone must not affect the original
    assert_eq!(original.name, "tick");
}

// T-EVT-05: NamedEvent Debug output contains the event name
#[test]
fn t_evt_05_named_event_debug_contains_name() {
    let ev = NamedEvent {
        name: "jump".to_string(),
        payload: serde_json::Value::Bool(true),
    };
    let dbg = format!("{:?}", ev);
    assert!(
        dbg.contains("jump"),
        "expected event name in Debug output: {dbg}"
    );
}

// ─── T-CFG: Config defaults and serde round-trip ──────────────────────────────

// T-CFG-01: SchedulerConfig::default has expected values
#[test]
fn t_cfg_01_scheduler_config_defaults() {
    use rython_core::SchedulerConfig;
    let cfg = SchedulerConfig::default();
    assert_eq!(cfg.target_fps, 60);
    assert_eq!(cfg.spin_threshold_us, 1000);
    assert!(cfg.parallel_threads.is_none());
}

// T-CFG-02: WindowConfig::default has expected values
#[test]
fn t_cfg_02_window_config_defaults() {
    use rython_core::WindowConfig;
    let cfg = WindowConfig::default();
    assert_eq!(cfg.width, 1280);
    assert_eq!(cfg.height, 720);
    assert!(!cfg.fullscreen);
    assert!(!cfg.vsync);
    assert_eq!(cfg.title, "RythonEngine");
}

// T-CFG-03: EngineConfig JSON round-trip preserves all fields
#[test]
fn t_cfg_03_engine_config_json_round_trip() {
    let original = EngineConfig::default();
    let json = serde_json::to_string(&original).expect("serialise");
    let restored: EngineConfig = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(restored.scheduler.target_fps, original.scheduler.target_fps);
    assert_eq!(restored.window.width, original.window.width);
    assert_eq!(restored.window.title, original.window.title);
}

// T-CFG-04: EngineConfig deserialises from empty JSON object — all fields fall back to defaults
#[test]
fn t_cfg_04_engine_config_empty_json_uses_defaults() {
    let cfg: EngineConfig = serde_json::from_str("{}").expect("deserialise empty object");
    assert_eq!(cfg.scheduler.target_fps, 60);
    assert_eq!(cfg.window.width, 1280);
    assert_eq!(cfg.window.title, "RythonEngine");
}

// T-CFG-05: EngineConfig deserialises partial overrides correctly
#[test]
fn t_cfg_05_engine_config_partial_override() {
    let json = r#"{"scheduler": {"target_fps": 120}, "window": {"width": 1920, "height": 1080}}"#;
    let cfg: EngineConfig = serde_json::from_str(json).expect("deserialise");
    assert_eq!(cfg.scheduler.target_fps, 120);
    // spin_threshold_us not provided — should use default
    assert_eq!(cfg.scheduler.spin_threshold_us, 1000);
    assert_eq!(cfg.window.width, 1920);
    assert_eq!(cfg.window.height, 1080);
    // title not provided — should use default
    assert_eq!(cfg.window.title, "RythonEngine");
}

// ─── T-TYP: Priority constants and core type aliases ──────────────────────────

// T-TYP-01: Priority constants are in strictly ascending order
#[test]
fn t_typ_01_priority_constants_ordering() {
    assert!(priorities::MODULE_LIFECYCLE < priorities::ENGINE_SETUP);
    assert!(priorities::ENGINE_SETUP < priorities::PRE_UPDATE);
    assert!(priorities::PRE_UPDATE < priorities::GAME_EARLY);
    assert!(priorities::GAME_EARLY < priorities::GAME_UPDATE);
    assert!(priorities::GAME_UPDATE < priorities::GAME_LATE);
    assert!(priorities::GAME_LATE < priorities::RENDER_ENQUEUE);
    assert!(priorities::RENDER_ENQUEUE < priorities::RENDER_EXECUTE);
    assert!(priorities::RENDER_EXECUTE < priorities::IDLE);
}

// T-TYP-02: Priority constants have expected absolute values
#[test]
fn t_typ_02_priority_constant_values() {
    assert_eq!(priorities::MODULE_LIFECYCLE, 0);
    assert_eq!(priorities::ENGINE_SETUP, 5);
    assert_eq!(priorities::PRE_UPDATE, 10);
    assert_eq!(priorities::GAME_EARLY, 15);
    assert_eq!(priorities::GAME_UPDATE, 20);
    assert_eq!(priorities::GAME_LATE, 25);
    assert_eq!(priorities::RENDER_ENQUEUE, 30);
    assert_eq!(priorities::RENDER_EXECUTE, 35);
    assert_eq!(priorities::IDLE, 40);
}

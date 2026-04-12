use rython_core::types::priorities;
use rython_core::{EngineConfig, EngineError, NamedEvent, ScriptError, TaskError};

// T-ERR-01: EngineError Wraps TaskError
#[test]
fn t_err_01_engine_error_wraps_task_error() {
    let source: Box<dyn std::error::Error + Send + Sync + 'static> =
        Box::new(std::io::Error::other("test failure"));

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
        source: Box::new(std::io::Error::other(engine_with_script.to_string())),
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
    assert!(cancelled.to_string().contains("cancelled") || !cancelled.to_string().is_empty());

    let timed_out = TaskError::TimedOut;
    assert!(timed_out.to_string().contains("timed out") || !timed_out.to_string().is_empty());

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
#[allow(clippy::assertions_on_constants)]
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

// ─── T-CFG-06..08: Malformed JSON edge cases ─────────────────────────────────

// T-CFG-06: Malformed JSON — wrong type for target_fps (string instead of number)
#[test]
fn t_cfg_06_malformed_json_wrong_type() {
    let json = r#"{"scheduler": {"target_fps": "sixty"}}"#;
    let result = serde_json::from_str::<EngineConfig>(json);
    assert!(
        result.is_err(),
        "target_fps as string must fail deserialization"
    );
}

// T-CFG-07: Unknown keys in JSON are silently ignored (serde default behavior)
#[test]
fn t_cfg_07_malformed_json_unknown_keys_ignored() {
    let json = r#"{"unknown_field": true, "scheduler": {"target_fps": 90}}"#;
    let cfg: EngineConfig = serde_json::from_str(json).expect("unknown keys should be ignored");
    assert_eq!(
        cfg.scheduler.target_fps, 90,
        "known field must have the provided value"
    );
    // Remaining fields should be defaults
    assert_eq!(cfg.scheduler.spin_threshold_us, 1000);
    assert!(cfg.scheduler.parallel_threads.is_none());
    assert_eq!(cfg.window.width, 1280);
    assert_eq!(cfg.window.height, 720);
    assert_eq!(cfg.window.title, "RythonEngine");
}

// T-CFG-08: Completely invalid JSON string
#[test]
fn t_cfg_08_malformed_json_completely_invalid() {
    let result = serde_json::from_str::<EngineConfig>("not json at all");
    assert!(result.is_err(), "non-JSON input must fail deserialization");
}

// ─── T-ERR-12..13: Error variant coverage and source chain ───────────────────

// T-ERR-12: Every EngineError variant has a non-empty Display string with relevant info
#[test]
fn t_err_12_engine_error_display_all_variants() {
    let task_err = TaskError::Failed {
        source: Box::new(std::io::Error::other("task inner")),
    };

    let variants: Vec<EngineError> = vec![
        task_err.into(),
        EngineError::Script(ScriptError::PythonException {
            script: "test.py".to_string(),
            exception: "RuntimeError".to_string(),
        }),
        EngineError::Module {
            module: "TestMod".to_string(),
            message: "init failed".to_string(),
        },
        EngineError::Resource("missing asset".to_string()),
        EngineError::Renderer("GPU lost".to_string()),
        EngineError::Physics("solver diverged".to_string()),
        EngineError::Audio("device unavailable".to_string()),
        EngineError::Config("bad value".to_string()),
        EngineError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "file missing",
        )),
    ];

    let expected_substrings = [
        "task inner",
        "test.py",
        "TestMod",
        "missing asset",
        "GPU lost",
        "solver diverged",
        "device unavailable",
        "bad value",
        "file missing",
    ];

    for (err, substr) in variants.iter().zip(expected_substrings.iter()) {
        let msg = err.to_string();
        assert!(!msg.is_empty(), "Display must not be empty for {:?}", err);
        assert!(
            msg.contains(substr),
            "expected '{substr}' in Display of {:?}, got: {msg}",
            err
        );
    }
}

// T-ERR-13: Error source chain traversal — walk at least 2 levels deep
#[test]
fn t_err_13_error_source_chain_traversal() {
    use std::error::Error;

    // Build a chain: EngineError::Task -> TaskError::Failed -> io::Error
    let io_err = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "connection lost");
    let task_err = TaskError::Failed {
        source: Box::new(io_err),
    };
    let engine_err: EngineError = task_err.into();

    // Level 0: EngineError itself
    let msg_0 = engine_err.to_string();
    assert!(!msg_0.is_empty(), "EngineError Display must not be empty");

    // Level 1: TaskError::Failed (via source())
    let source_1 = engine_err
        .source()
        .expect("EngineError::Task must have a source");
    let msg_1 = source_1.to_string();
    assert!(
        msg_1.contains("connection lost"),
        "TaskError::Failed source should surface the inner message: {msg_1}"
    );

    // Level 2: io::Error (via source() on TaskError::Failed)
    let source_2 = source_1
        .source()
        .expect("TaskError::Failed must have a source");
    let msg_2 = source_2.to_string();
    assert!(
        msg_2.contains("connection lost"),
        "inner io::Error message should be accessible: {msg_2}"
    );

    // Verify chain depth is at least 2
    let mut depth = 0u32;
    let mut current: Option<&dyn Error> = Some(&engine_err);
    while let Some(err) = current {
        current = err.source();
        if current.is_some() {
            depth += 1;
        }
    }
    assert!(
        depth >= 2,
        "error source chain must be at least 2 levels deep, got {depth}"
    );
}

// ─── T-EVT-06..07: NamedEvent edge cases ─────────────────────────────────────

// T-EVT-06: NamedEvent with an empty string name is valid (no panic)
#[test]
fn t_evt_06_named_event_empty_name() {
    let ev = NamedEvent {
        name: String::new(),
        payload: serde_json::json!({"key": "value"}),
    };
    assert_eq!(ev.name, "", "empty name must be preserved");
    assert!(ev.payload.is_object(), "payload must remain intact");
    assert_eq!(ev.payload["key"], "value");
}

// T-EVT-07: NamedEvent with a large JSON array payload clones correctly
#[test]
fn t_evt_07_named_event_large_payload() {
    let large_array: Vec<i32> = (0..1000).collect();
    let ev = NamedEvent {
        name: "big_event".to_string(),
        payload: serde_json::json!(large_array),
    };

    let cloned = ev.clone();

    let original_arr = ev.payload.as_array().expect("payload must be an array");
    let cloned_arr = cloned
        .payload
        .as_array()
        .expect("cloned payload must be an array");
    assert_eq!(
        original_arr.len(),
        1000,
        "original array must have 1000 elements"
    );
    assert_eq!(
        cloned_arr.len(),
        original_arr.len(),
        "cloned payload length must match original"
    );
    // Spot-check a few values
    assert_eq!(original_arr[0], 0);
    assert_eq!(original_arr[999], 999);
    assert_eq!(cloned_arr[500], 500);
}

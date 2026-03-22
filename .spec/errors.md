# Error Handling

RythonEngine uses a three-layer error model. This is one of the engine's distinguishing design choices: errors are structured, propagated cleanly, and never cause panics. The engine recovers from every error — a broken script or a missing asset does not crash the game.


## Three Layers

```
+-----------------------------------------------------+
|  Layer 3: Python Errors                              |
|  Caught, wrapped in ScriptError, never panic         |
+--------------------------+--------------------------+
|  Layer 2: Task Errors                                |
|  Returned as TaskError, never panic                  |
+--------------------------+--------------------------+
|  Layer 1: Engine Errors                              |
|  Rust Result<T, E> everywhere, logged + recovered    |
+-----------------------------------------------------+
```


## Layer 1: Engine Errors

The top-level error type used throughout the Rust engine. Every function that can fail returns `Result<T, EngineError>`. Engine errors are never panics — they are values that propagate through the call stack.

Engine error variants:
- **Task**: Wraps a TaskError from the scheduler
- **Module**: A module failed to load, unload, or execute (includes module name and message)
- **Resource**: Asset loading failed (file not found, decode error, budget exceeded)
- **Renderer**: GPU error (shader compilation, device lost, surface configuration)
- **Physics**: Physics simulation error (invalid body, NaN detected)
- **Audio**: Audio system error (device not found, format unsupported)
- **Script**: Wraps a ScriptError from the Python bridge
- **Io**: File system error
- **Config**: Configuration parsing error

When an EngineError occurs, the engine logs it and continues. The only errors that stop the engine are truly fatal conditions like losing the GPU device.


## Layer 2: Task Errors

Errors that occur during task execution. The scheduler catches these and handles them without stopping the frame.

Task error variants:
- **Panicked**: A task's closure panicked (caught by `catch_unwind`). The panic message is captured.
- **Cancelled**: A task was cancelled due to owner module unloading.
- **TimedOut**: A task exceeded its timeout (for tasks with deadlines).
- **Failed**: A task returned an error. The original error is preserved as the source.

Task errors are logged by the scheduler. If the task had an error callback, it is invoked. If not, the error is logged and the scheduler moves to the next task.

```python
# From Python, task errors appear as exceptions with context:
# rython.TaskError: Task 'physics_step' failed: NaN detected in body position
```


## Layer 3: Script Errors

Errors originating from Python game scripts. These are the most common errors during development — typos, missing attributes, logic bugs. They are always caught and never crash the engine.

Script error variants:
- **PythonException**: A Python exception was raised during script execution. Contains the script name, method, and full traceback.
- **NotFound**: A script file or module could not be found.
- **ReloadFailed**: Hot-reload failed for a script (syntax error in the modified file, import error).

```python
# What the developer sees in the console (dev mode):
#
# ERROR Script error in EnemyScript.on_collision (enemy.py:23):
#   Traceback (most recent call last):
#     File "scripts/enemy.py", line 23, in on_collision
#       self.health -= event.damage
#   AttributeError: 'CollisionEvent' has no attribute 'damage'
#
# (Engine continues running. The enemy script is disabled for this entity
#  until the file is fixed and hot-reloaded.)
```


## Error Propagation

Errors propagate upward through the layers:

```
Python ValueError in script handler
  -> PyO3 catches it
  -> ScriptSystem wraps in ScriptError::PythonException
  -> ScriptError converts to EngineError::Script
  -> Scheduler wraps in TaskError::Failed (if running as a task)
  -> Error logged with full context
  -> Engine continues
```

At each layer, context is added (which script, which task, which module), making errors easy to diagnose.


## Recovery Strategies

Different error types trigger different recovery strategies:

| Error | Recovery |
|-------|----------|
| Script exception | Log traceback, disable handler until hot-reload fixes it |
| Asset not found | Log warning, return a fallback asset (pink texture, silent sound) |
| Shader compilation failed | Log error, skip draw commands that need the shader |
| Physics NaN | Log warning, reset the offending body to its last valid state |
| Module load failed | Log error, skip the module, continue without it if non-critical |
| GPU device lost | Attempt to recreate the device and reload GPU resources |
| Config parse error | Log warning, use default configuration |

The engine strives to never show a blank screen or freeze. Something is always rendered, even if it is a fallback state.


## Dev vs Release Error Presentation

In **dev mode**:
- Full Python tracebacks are printed
- Errors include file paths and line numbers
- Warnings are logged at debug level
- GPU validation layers are enabled for detailed GPU error messages

In **release mode**:
- Python tracebacks are condensed to one-line summaries
- File paths are stripped (scripts are bundled)
- Only warnings and errors are logged
- GPU validation is disabled
- A friendly error screen can be shown to the player instead of console output


## Acceptance Tests

### T-ERR-01: EngineError Wraps TaskError
Create a TaskError::Failed with message "test failure". Convert it to EngineError.
- Expected: The EngineError variant is Task
- Expected: `to_string()` contains "test failure"
- Expected: The original TaskError is accessible via source/downcast

### T-ERR-02: EngineError Wraps ScriptError
Create a ScriptError::PythonException with script="player.py", exception="NameError: x". Convert to EngineError.
- Expected: The EngineError variant is Script
- Expected: `to_string()` contains "player.py" and "NameError"

### T-ERR-03: TaskError Captures Panic Message
Submit a task that panics with `panic!("something broke")`. Run the tick.
- Expected: A TaskError::Panicked is produced
- Expected: The panic message "something broke" is captured in the error
- Expected: The scheduler does not itself panic

### T-ERR-04: TaskError::Cancelled on Owner Unload
Submit a task with owner_id=5. Cancel all tasks for owner 5 before the tick.
- Expected: The task's state is set to Cancelled
- Expected: The task does not execute

### T-ERR-05: ScriptError Captures Traceback
Execute a Python script that raises `AttributeError` on line 15 of `enemy.py`.
- Expected: The ScriptError contains script="enemy.py"
- Expected: The exception string contains "AttributeError"
- Expected: The exception string contains "line 15"

### T-ERR-06: ScriptError::NotFound
Attempt to import a Python module "nonexistent_module".
- Expected: A ScriptError::NotFound is produced
- Expected: The error message contains "nonexistent_module"

### T-ERR-07: ScriptError::ReloadFailed
Attempt to hot-reload a script with a syntax error.
- Expected: A ScriptError::ReloadFailed is produced
- Expected: The error contains the file path and the syntax error description

### T-ERR-08: Error Propagation Chain — Python to Engine
A Python script raises ValueError in an event handler. Trace the error through all layers.
- Expected: Layer 3: ScriptError::PythonException is created
- Expected: Layer 2: TaskError::Failed wraps the ScriptError (as source)
- Expected: Layer 1: EngineError::Script wraps the ScriptError
- Expected: Each layer adds context (script name, task name, module name)

### T-ERR-09: Recovery — Script Exception Disables Handler
A script's `on_collision` raises an exception. Trigger the collision again on the next frame.
- Expected: First collision: error logged, handler disabled
- Expected: Second collision: handler does NOT fire (it is disabled)
- Expected: Engine continues running both times

### T-ERR-10: Recovery — Missing Asset Returns Fallback
Load an image "does_not_exist.png". Attempt to render with it.
- Expected: AssetHandle transitions to FAILED
- Expected: The renderer uses a fallback texture (e.g., solid pink/magenta) instead of crashing
- Expected: A warning is logged mentioning the missing file

### T-ERR-11: Recovery — Config Parse Error Uses Defaults
Provide a window.json with invalid JSON (e.g., `{width: }`, missing value).
- Expected: A Config error is logged
- Expected: The window module uses default config (1280x720, windowed, vsync off)
- Expected: The engine starts successfully

### T-ERR-12: Recovery — Physics NaN Reset
Set a body position to (NaN, 0, 0) and step the simulation.
- Expected: A Physics error is logged mentioning NaN
- Expected: The body is reset to its last valid position
- Expected: The simulation continues on subsequent frames

### T-ERR-13: No Panics Under Any Error
Run a comprehensive error injection test: missing config, missing scripts, missing assets, Python exceptions, invalid shader, NaN physics values — all at once.
- Expected: The engine does not panic or abort at any point
- Expected: All errors are logged
- Expected: The engine reaches a running state (even if degraded)

### T-ERR-14: Error Display — Dev Mode
Trigger a Python exception in dev mode.
- Expected: The full traceback is printed including file path, line number, and exception type
- Expected: The log level is ERROR

### T-ERR-15: Error Display — Release Mode
Trigger a Python exception in release mode.
- Expected: A one-line summary is logged (no full traceback)
- Expected: File paths are not included (scripts are bundled)
- Expected: The log level is ERROR

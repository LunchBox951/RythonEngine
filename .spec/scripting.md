# Scripting

The scripting system bridges Rust engine modules to Python game code via PyO3. It exposes a `rython` Python module that game developers import to interact with all engine systems. Scripts are event-driven: they declare handler methods that the engine calls when events fire. There are no per-frame tick callbacks.


## Python Module: `rython`

The PyO3 bridge exposes a single Python module called `rython`. This module provides access to all engine systems through a clean, Pythonic API.

```python
import rython

# Access engine systems
rython.scene        # Entity/component management
rython.renderer     # Draw commands
rython.physics      # Forces, velocities, raycasts
rython.audio        # Sound playback
rython.input        # Input state and maps
rython.ui           # UI widgets
rython.resources    # Asset loading
rython.scheduler    # Task submission
rython.modules      # Module registry
rython.camera       # Camera control
```

Each sub-module is a thin Python wrapper around the corresponding Rust module. The wrappers hold references to the Rust objects and translate Python calls into Rust method invocations.


## Script Classes

Game logic is written as Python classes attached to entities via ScriptComponent. A script class receives an entity reference on construction and declares handler methods for events it cares about.

```python
import rython

class PlayerScript:
    def __init__(self, entity):
        self.entity = entity
        self.speed = 10.0
        self.health = 100

    def on_collision(self, other, normal):
        """Called when this entity's physics body collides with another."""
        if other.has_tag("damage"):
            self.health -= 25
            if self.health <= 0:
                self.entity.despawn()
                rython.audio.play("sfx/death.wav", category="sfx")

    def on_trigger_enter(self, other):
        """Called when this entity enters a trigger volume."""
        if other.has_tag("checkpoint"):
            rython.audio.play("sfx/checkpoint.wav", category="sfx")

    def on_input_action(self, action, value):
        """Called when a mapped input action fires."""
        if action == "move_x":
            vel = rython.physics.get_linear_velocity(self.entity)
            rython.physics.set_linear_velocity(
                self.entity, (value * self.speed, vel.y, vel.z)
            )
        elif action == "jump" and value > 0:
            rython.physics.apply_impulse(self.entity, (0, 50, 0))

    def on_spawn(self):
        """Called once when the entity spawns."""
        rython.audio.play("sfx/spawn.wav", category="sfx")

    def on_despawn(self):
        """Called just before the entity is removed."""
        pass
```


## ScriptSystem

The ScriptSystem is an engine system (not a Module) that manages the lifecycle of Python script instances. It subscribes to EntitySpawnedEvent and EntityDespawnedEvent on the Scene's event bus.

**On entity spawn**: If the entity has a ScriptComponent, the ScriptSystem:
1. Instantiates the Python script class, passing the entity wrapper
2. Calls the script's `on_spawn()` method if defined
3. Introspects the script class for handler methods (on_collision, on_trigger_enter, on_input_action, etc.)
4. Subscribes those handlers to the corresponding event types on the event bus

**On entity despawn**: The ScriptSystem:
1. Calls the script's `on_despawn()` method if defined
2. Unsubscribes all handlers from the event bus
3. Drops the Python script instance

The ScriptSystem runs all Python calls within a single GIL acquisition per batch. When multiple events fire in a frame, the system acquires the GIL once, dispatches all events, then releases it. This minimizes GIL overhead.


## Event Dispatch Flow

```
1. Physics step emits CollisionEvent(entity_A, entity_B, normal)
2. Event bus delivers to ScriptSystem
3. ScriptSystem acquires Python GIL
4. ScriptSystem calls entity_A's script.on_collision(entity_B_wrapper, normal)
5. ScriptSystem calls entity_B's script.on_collision(entity_A_wrapper, -normal)
6. ScriptSystem releases GIL
```

Script handlers are always called as sequential tasks on the main thread. They never run in parallel. This means scripts don't need to worry about thread safety — they have exclusive access to all engine state during their execution.


## Custom Events

Games can define and emit custom events from Python:

```python
# Emit a custom event
rython.scene.emit("PlayerDied", entity_id=player.id, cause="fall_damage")

# Subscribe to it
def on_player_died(entity_id, cause):
    print(f"Player {entity_id} died from {cause}")
    rython.ui.show(game_over_screen)

rython.scene.subscribe("PlayerDied", on_player_died)
```


## Dev Mode: Hot-Reload

In development builds (compiled with the `dev-reload` feature flag), the ScriptingModule watches the scripts directory for file changes using the `notify` crate.

When a `.py` file changes:
1. The file watcher detects the modification
2. The ScriptingModule reloads the Python module using `importlib.reload()`
3. For each entity with a ScriptComponent pointing to the reloaded module:
   a. Unsubscribe old handlers from the event bus
   b. Instantiate the new script class (preserving entity reference)
   c. Subscribe new handlers
4. Log which scripts were reloaded

Hot-reload preserves entity state (position, components) but resets script instance state (Python `self.*` attributes). If the game needs persistent state across reloads, it should store it on components rather than on the script instance.

```
File change detected: scripts/player.py
  -> Reloading module: scripts.player
  -> Re-attached PlayerScript to 3 entities
  -> Hot-reload complete (12ms)
```


## Release Mode: Bundled Scripts

In release builds, scripts are bundled into an archive by the `tools/bundler` tool. The archive is loaded at startup and scripts are imported from memory rather than from disk.

The bundler tool:
1. Reads all `.py` files from the scripts directory
2. Optionally compiles them to `.pyc` bytecode
3. Packs them into a zip archive or custom bundle format
4. Embeds the bundle as a resource in the final binary

At runtime:
1. The ScriptingModule extracts the bundle
2. Adds the bundle to Python's import path (via `sys.path` or a custom finder)
3. Scripts are imported normally via `import` statements

No file watcher runs in release mode. No hot-reload.


## Error Handling

Python exceptions raised in script handlers are caught by the ScriptSystem and wrapped into ScriptErrors. They never crash the engine.

```
Script error flow:

Python raises ValueError in on_collision()
  -> PyO3 catches it
  -> ScriptSystem wraps it in ScriptError::PythonException
  -> ScriptError wraps into TaskError::Failed
  -> TaskError wraps into EngineError::Script
  -> Error is logged with script name, method, and traceback
  -> Engine continues running
```

In dev mode, the full Python traceback is logged. In release mode, a condensed error message is shown.

```python
# In dev mode, you see:
# ERROR [rython::scripting] Script error in PlayerScript.on_collision:
#   Traceback (most recent call last):
#     File "scripts/player.py", line 15, in on_collision
#       self.health -= damage
#   AttributeError: 'PlayerScript' object has no attribute 'health'
```


## Python Wrappers

The scripting bridge provides Python wrapper classes for engine types:

### Entity Wrapper
```python
entity = rython.scene.spawn(transform=rython.Transform(x=0, y=5, z=0))

entity.id                    # Numeric entity ID
entity.transform             # TransformComponent (read/write properties)
entity.transform.x = 10.0
entity.has_tag("player")     # Check tag
entity.add_tag("damaged")    # Add tag
entity.despawn()             # Queue despawn
```

### Vec3 Wrapper
```python
pos = rython.Vec3(1.0, 2.0, 3.0)
pos.x, pos.y, pos.z
pos.length()
pos.normalized()
pos + other_vec3
pos * 2.0
```

### Transform Wrapper
```python
t = rython.Transform(x=0, y=5, z=0, rot_y=90, scale=2.0)
t.x, t.y, t.z           # Position
t.rot_x, t.rot_y, t.rot_z  # Rotation in degrees
t.scale                  # Uniform scale
```


## Entry Point

The game's entry point is a Python module specified in the engine config. The ScriptingModule imports this module at startup, which typically registers the game's modules, spawns initial entities, and sets up input maps.

```python
# scripts/main.py — the game entry point

import rython
from player import PlayerScript
from enemy import EnemyScript

def init():
    """Called by the engine after all modules are loaded."""
    # Set up input
    movement = rython.InputMap("movement")
    movement.bind_axis("move_x", keyboard=("A", "D"), gamepad="left_stick_x")
    movement.bind_axis("move_y", keyboard=("S", "W"), gamepad="left_stick_y")
    movement.bind_button("jump", keyboard="SPACE", gamepad="south")
    rython.input.set_active_map("movement")

    # Spawn player
    player = rython.scene.spawn(
        transform=rython.Transform(x=0, y=5, z=0),
        mesh=rython.MeshComponent(mesh_id="player", texture_id="player_tex"),
        rigid_body=rython.RigidBodyComponent(body_type="dynamic", mass=10),
        collider=rython.ColliderComponent(shape="capsule", size=(0.5, 1.8, 0.5)),
        tag=rython.TagComponent(tags=["player"]),
    )
    rython.scene.attach_script(player, PlayerScript)
```


## Configuration

```json
{
    "scripting": {
        "type": "dev",
        "script_dir": "./scripts",
        "entry_point": "main"
    }
}
```

For release:
```json
{
    "scripting": {
        "type": "release",
        "bundle_path": "./assets/scripts.bundle"
    }
}
```

- `type`: "dev" (file-based with hot-reload) or "release" (bundled)
- `script_dir`: Directory containing Python scripts (dev mode)
- `entry_point`: Python module name to import at startup (the `init()` function is called)
- `bundle_path`: Path to the bundled script archive (release mode)


## Acceptance Tests

### T-SCRIPT-01: Python Module Import
Initialize the ScriptingModule. From Python, run `import rython`.
- Expected: Import succeeds without error
- Expected: `rython.scene`, `rython.renderer`, `rython.physics`, `rython.audio`, `rython.input`, `rython.ui`, `rython.resources`, `rython.scheduler`, `rython.modules`, `rython.camera` are all accessible (not None)

### T-SCRIPT-02: Script Class Instantiation
Define a Python script class with `__init__(self, entity)`. Spawn an entity with a ScriptComponent pointing to this class.
- Expected: After entity spawn + command drain, the ScriptSystem creates an instance of the class
- Expected: `self.entity` in the instance refers to the correct entity
- Expected: `self.entity.id` matches the spawned entity's ID

### T-SCRIPT-03: on_spawn Callback
Define a script with `on_spawn(self)` that sets a flag. Spawn the entity.
- Expected: `on_spawn` is called exactly once after spawn
- Expected: The flag is set

### T-SCRIPT-04: on_despawn Callback
Define a script with `on_despawn(self)` that sets a flag. Spawn and then despawn the entity.
- Expected: `on_despawn` is called exactly once, before the entity is removed
- Expected: During `on_despawn`, the entity's components are still accessible

### T-SCRIPT-05: on_collision Handler Wiring
Define a script with `on_collision(self, other, normal)`. Spawn the entity with physics components. Cause a collision with another entity.
- Expected: `on_collision` is called on the script instance
- Expected: `other` is a valid entity wrapper for the other entity
- Expected: `normal` is a Vec3 with non-zero values

### T-SCRIPT-06: on_trigger_enter / on_trigger_exit
Define a script with both handlers. Move an entity through a trigger volume.
- Expected: `on_trigger_enter` fires when overlap begins
- Expected: `on_trigger_exit` fires when overlap ends
- Expected: Each fires exactly once per enter/exit cycle

### T-SCRIPT-07: on_input_action
Define a script with `on_input_action(self, action, value)`. Bind "jump" to SPACE. Press SPACE.
- Expected: Handler is called with action="jump" and value=1.0

### T-SCRIPT-08: Custom Event from Python
From a Python script, call `rython.scene.emit("MyEvent", data=42)`. Subscribe a handler.
- Expected: The handler receives data=42
- Expected: The event name is "MyEvent"

### T-SCRIPT-09: Entity Wrapper — Transform Read/Write
From Python: `entity.transform.x = 15.0`. Then read `entity.transform.x`.
- Expected: Returns 15.0
- Expected: The underlying ECS TransformComponent's x value is 15.0

### T-SCRIPT-10: Entity Wrapper — Tag Operations
From Python: `entity.add_tag("test")`. Then `entity.has_tag("test")`.
- Expected: `has_tag` returns True
- Expected: `entity.has_tag("nonexistent")` returns False

### T-SCRIPT-11: Vec3 Wrapper — Arithmetic
From Python: `a = rython.Vec3(1, 2, 3)`, `b = rython.Vec3(4, 5, 6)`, `c = a + b`.
- Expected: c.x == 5, c.y == 7, c.z == 9
- Expected: `(a * 2.0).x == 2.0`
- Expected: `rython.Vec3(3, 4, 0).length()` == 5.0

### T-SCRIPT-12: Python Exception Does Not Crash Engine
Define a script with `on_collision` that raises `ValueError("test error")`. Trigger a collision.
- Expected: The engine does NOT crash or panic
- Expected: An error log entry contains "ValueError" and "test error"
- Expected: The error log contains the script file name and line number
- Expected: Subsequent frames continue normally

### T-SCRIPT-13: Multiple Script Errors Per Frame
Three entities with scripts that all raise exceptions in their `on_collision` handlers. Trigger collisions for all three in the same frame.
- Expected: All 3 errors are logged (not just the first)
- Expected: The engine continues running after all 3 failures

### T-SCRIPT-14: Hot-Reload — File Change Detection
Write a Python script to disk. Wait for the file watcher to detect it. Modify the script.
- Expected: Within 1 second of saving, the ScriptingModule detects the change
- Expected: A log message indicates the reload is in progress

### T-SCRIPT-15: Hot-Reload — Handler Rebinding
Entity has a script with `on_collision` that sets flag_v1=True. Hot-reload the script to a new version where `on_collision` sets flag_v2=True. Trigger a collision.
- Expected: flag_v2 is set (new handler is active)
- Expected: flag_v1 is NOT set (old handler was unsubscribed)

### T-SCRIPT-16: Hot-Reload — Syntax Error Resilience
Hot-reload a script with a syntax error (e.g., missing colon on `def`).
- Expected: Reload fails gracefully (ScriptError::ReloadFailed)
- Expected: The old version of the script remains active
- Expected: A log message indicates the reload failed with the syntax error details

### T-SCRIPT-17: Hot-Reload — Entity State Preserved
Entity at position (10, 20, 30). Hot-reload its script.
- Expected: Entity position is still (10, 20, 30) after reload
- Expected: All components are preserved

### T-SCRIPT-18: Release Mode — Bundle Loading
Build with release config. Place scripts in a bundle archive. Start the engine.
- Expected: Scripts are loaded from the bundle (not from disk)
- Expected: `import rython` works from bundled scripts
- Expected: No file watcher is active

### T-SCRIPT-19: GIL Batch Acquisition
Emit 50 events in a single frame that dispatch to Python handlers. Instrument GIL acquire/release calls.
- Expected: The GIL is acquired at most 2 times per frame (GAME_UPDATE batch + GAME_LATE batch)
- Expected: The GIL is NOT acquired 50 times (once per event)

### T-SCRIPT-20: Entry Point Execution
Configure scripting with entry_point="main". Place a `scripts/main.py` with `def init()` that sets a global flag.
- Expected: After module loading, the `init()` function has been called
- Expected: The global flag is set

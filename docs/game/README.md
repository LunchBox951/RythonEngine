# RythonEngine — Game Scripting Guide

Game logic in RythonEngine is written in Python. The engine exposes a `rython` module through PyO3 bindings that gives scripts full access to the scene, camera, renderer, scheduler, and engine lifecycle.

**See also:** [`docs/engine/`](../engine/README.md) for Rust implementation details.

---

## Contents

1. [Entry Point Convention](#entry-point-convention)
2. [The `rython` Module](#the-rython-module)
3. [Type Wrappers](#type-wrappers)
4. [Spawning Entities](#spawning-entities)
5. [Entity API](#entity-api)
6. [Camera Control](#camera-control)
7. [Per-Frame Updates](#per-frame-updates)
8. [Custom Events](#custom-events)
9. [Drawing Text](#drawing-text)
10. [Time](#time)
11. [Quitting](#quitting)
12. [Script Classes](#script-classes)
13. [Hot-Reload (Dev Mode)](#hot-reload-dev-mode)
14. [Complete Example: Spinning Cubes](#complete-example-spinning-cubes)
15. [@throttle Decorator](#throttle-decorator)
16. [Parallel & Background Tasks](#parallel--background-tasks)

---

## Entry Point Convention

The engine loads scripts from the directory specified by `--script-dir` (default: `./scripts`). The entry point module is specified by `--entry-point` (default: `main`).

The engine imports the module and calls `init()` once on load:

```python
# scripts/main.py
import rython

def init():
    """Called once when the scripting module is loaded."""
    rython.camera.set_position(0.0, 5.0, -10.0)
    rython.camera.set_look_at(0.0, 0.0, 0.0)
```

If no `--entry-point` is given, the engine looks for a `main` module (`scripts/main.py`).

---

## The `rython` Module

The `rython` module is injected into `sys.modules` by the engine. It has the following sub-modules:

| Sub-module | Purpose |
|---|---|
| `rython.scene` | Spawn/despawn entities, emit and subscribe to events |
| `rython.camera` | Control the camera position and orientation |
| `rython.scheduler` | Register per-frame callbacks; one-shot timers and events; parallel/background task submission |
| `rython.renderer` | Queue draw commands (text overlays) |
| `rython.time` | Read elapsed engine time |
| `rython.engine` | Engine lifecycle control |
| `rython.physics` | Physics world control (gravity, impulses) |
| `rython.audio` | Audio playback |
| `rython.input` | Input state queries |
| `rython.ui` | UI widget management |
| `rython.resources` | Asset/resource loading |
| `rython.Vec3` | 3D vector type |
| `rython.Transform` | Entity transform type |

`rython.modules` is a stub and will raise `ValueError` if accessed. All other sub-modules listed above are fully implemented bridges.

---

## Type Wrappers

### `rython.Vec3`

A 3D vector with `f32` components.

```python
v = rython.Vec3(1.0, 0.0, 0.0)

v.x, v.y, v.z      # read/write component access

v.length()          # float — Euclidean length
v.normalized()      # Vec3 — unit vector (returns zero-vector if length < epsilon)
v.dot(other)        # float — dot product

# Arithmetic operators
a + b               # Vec3 addition
a - b               # Vec3 subtraction
a * scalar          # Vec3 scaled by float
scalar * a          # same (right-multiplication)
-a                  # Vec3 negation
```

### `rython.Transform`

Position, rotation (Euler angles in radians), and scale for an entity.

```python
t = rython.Transform(x=0.0, y=0.0, z=0.0,
                     rot_x=0.0, rot_y=0.0, rot_z=0.0,
                     scale=1.0,
                     scale_x=None, scale_y=None, scale_z=None)
```

All parameters are keyword-optional. Position and rotation default to `0.0`. `scale` defaults to `1.0` and sets all three axes uniformly. `scale_x`, `scale_y`, `scale_z` override the uniform scale on a per-axis basis; when omitted they inherit `scale`.

Per-axis scale properties are also readable/writable on a live transform:

```python
t.scale         # float — uniform getter (returns scale_x); setter sets all three axes
t.scale_x       # float — X scale
t.scale_y       # float — Y scale
t.scale_z       # float — Z scale
```

When a `Transform` is retrieved from an `Entity`, writing to its fields immediately propagates the change back to the ECS component:

```python
cube.transform.rot_y += 0.05   # live write-back to ECS
```

---

## Spawning Entities

Use `rython.scene.spawn()` to create a new entity. All parameters are keyword arguments:

```python
entity = rython.scene.spawn(
    transform=rython.Transform(x=1.0, y=0.0, z=0.0, scale=1.0),
    mesh="cube",            # built-in mesh ID, or registered asset ID
    tags=["player", "solid"],
)
```

| kwarg | Type | Description |
|---|---|---|
| `transform` | `rython.Transform` | Initial position/rotation/scale |
| `mesh` | `str` or `dict` | Mesh to render. String = mesh ID shorthand. Dict keys: `mesh_id`, `texture_id`, `visible` |
| `tags` | `list[str]` | Tag strings attached to the entity |
| `rigid_body` | `dict` | Attach a physics body. Keys: `body_type` (`"dynamic"`, `"static"`, `"kinematic"`), `mass` (`float`, default `1.0`), `gravity_factor` (`float`, default `1.0`) |
| `collider` | `dict` | Attach a collision shape. Keys: `shape` (`"box"`, `"sphere"`, `"capsule"`), `size` (`[f32, f32, f32]`), `is_trigger` (`bool`, default `false`) |

`spawn()` returns an `Entity` object. The entity is immediately live in the scene.

**Mesh dict form:**

```python
entity = rython.scene.spawn(
    transform=rython.Transform(x=0.0, y=0.0, z=0.0),
    mesh={"mesh_id": "cube", "texture_id": "stone", "visible": True},
)
```

**Physics body + collider:**

```python
ball = rython.scene.spawn(
    transform=rython.Transform(x=0.0, y=5.0, z=0.0),
    mesh="sphere",
    rigid_body={"body_type": "dynamic", "mass": 2.0, "gravity_factor": 1.0},
    collider={"shape": "sphere", "size": [0.5, 0.5, 0.5], "is_trigger": False},
)
```

---

## Entity API

```python
entity = rython.scene.spawn(transform=rython.Transform(), mesh="cube")

entity.id                   # int — unique entity ID

entity.transform            # Transform — live read/write to ECS
entity.transform.x = 5.0   # immediately propagates to the engine

entity.has_tag("player")    # bool
entity.add_tag("flying")    # add a tag at runtime

entity.despawn()            # queue the entity for removal

# Physics (requires a RigidBodyComponent on the entity)
entity.apply_force(x, y, z)    # apply a continuous force (world space)
entity.apply_impulse(x, y, z)  # apply an instantaneous impulse (world space)
entity.set_velocity(x, y, z)   # set linear velocity directly
entity.velocity                # Vec3 — current linear velocity
```

---

## Camera Control

`rython.camera` is a singleton camera object. Changes take effect on the next rendered frame.

```python
# Position
rython.camera.set_position(x, y, z)

# Orientation — Euler angles in radians (pitch, yaw, roll)
rython.camera.set_rotation(pitch, yaw, roll)

# Point-at — computes pitch/yaw from current position to target
rython.camera.set_look_at(target_x, target_y, target_z)

# Read current position
px = rython.camera.pos_x
py = rython.camera.pos_y
pz = rython.camera.pos_z

# Read current orientation
rython.camera.rot_pitch
rython.camera.rot_yaw
rython.camera.rot_roll

# Read current look-at target (world-space point the camera is aimed at)
rython.camera.target_x
rython.camera.target_y
rython.camera.target_z
```

---

## Per-Frame Updates

Register a callback to be called every frame:

```python
def on_tick():
    t = rython.time.elapsed
    # update game state here

rython.scheduler.register_recurring(on_tick)
```

`register_recurring` accepts any callable. Multiple callbacks can be registered; they are called in registration order each frame.

**Use `on_timer` for delayed one-shot actions and `on_event` for reacting to the next occurrence of an event.** Both are one-shot: the callback fires exactly once.

```python
# Fire once after 5 seconds
rython.scheduler.on_timer(5.0, spawn_boss)

# Repeat by re-arming from within the callback
def spawn_wave():
    do_spawn()
    rython.scheduler.on_timer(10.0, spawn_wave)   # re-arm for next wave

rython.scheduler.on_timer(10.0, spawn_wave)

# React to the next occurrence of a named event (fires once, then done)
rython.scheduler.on_event("player_died", lambda **kw: show_respawn_screen())
```

For a persistent subscription that fires on every occurrence, use `rython.scene.subscribe` instead (see [Custom Events](#custom-events)).

---

## Custom Events

Scripts can communicate through named events:

```python
# Emit an event with a keyword payload
rython.scene.emit("player_died", score=42, reason="fell")

# Subscribe to an event — handler receives payload as keyword arguments
def on_player_died(score=0, reason="unknown"):
    print(f"Player died with score {score} ({reason})")

subscription_id = rython.scene.subscribe("player_died", on_player_died)

# Unsubscribe when the handler is no longer needed
rython.scene.unsubscribe("player_died", subscription_id)
```

Event payloads support `None`, `bool`, `int`, `float`, and `str` values.

---

## Drawing Text

Queue a text overlay draw command for the current frame:

```python
rython.renderer.draw_text(
    "Hello World",
    font_id="default",   # font asset ID (default: "default")
    x=0.5,               # normalized screen X (0.0 = left, 1.0 = right)
    y=0.1,               # normalized screen Y (0.0 = top, 1.0 = bottom)
    size=16,             # font size in pixels
    r=255,               # red channel (0–255)
    g=255,               # green channel
    b=255,               # blue channel
    z=0.0,               # depth sort order (higher = on top)
)
```

All parameters except `text` are optional and use the defaults shown above.

---

## Time

```python
t = rython.time.elapsed   # float — seconds since engine start
```

Use this in `register_recurring` callbacks to drive time-based animation.

---

## Quitting

Signal the engine to exit cleanly after the current frame:

```python
rython.engine.request_quit()
```

The engine completes the current frame, calls shutdown on all modules, and exits.

---

## Script Classes

For entity-attached behaviour, define a class with lifecycle handlers. Attach it to an entity via `rython.scene.attach_script()`:

```python
class Player:
    def __init__(self, entity):
        self.entity = entity

    def on_spawn(self):
        """Called once immediately after attach_script() registers the instance."""
        pass

    def on_despawn(self):
        """Called just before the entity is removed from the scene."""
        pass

    def on_collision(self, other_entity, normal_vec):
        """Called when this entity collides with another (requires RigidBodyComponent).
        other_entity: Entity — the colliding entity
        normal_vec:   Vec3  — contact normal pointing away from other_entity
        """
        pass

    def on_trigger_enter(self, other_entity):
        """Called when another entity enters this entity's trigger collider."""
        pass

    def on_trigger_exit(self, other_entity):
        """Called when another entity exits this entity's trigger collider."""
        pass

    def on_input_action(self, action_name, value):
        """Called when a mapped input action fires.
        action_name: str   — the action identifier
        value:       float — axis value (1.0 for digital press, analogue for axes)
        """
        pass

entity = rython.scene.spawn(transform=rython.Transform(), mesh="cube")
rython.scene.attach_script(entity, Player)
```

The engine instantiates the class with the entity as the first argument. All lifecycle handlers listed above are fully implemented — define only the ones you need. Handlers that are absent on the class are silently skipped.

---

## Hot-Reload (Dev Mode)

In development builds (`--features dev-reload`), the engine watches `--script-dir` for file changes. When a `.py` file is modified, the engine re-imports it without restarting. The `init()` function is called again on reload.

Hot-reload is not available in release builds. Scripts are bundled into the binary at release time.

---

## Complete Example: Spinning Cubes

This example demonstrates the core scripting API: spawning entities, per-frame updates, camera setup, and HUD text.

```python
"""Spinning Cubes — full API example.

Spawns a ring of nine cubes that rotate around the Y axis, with a frame counter
overlaid as on-screen text.
"""
import math
import rython

CUBE_COUNT = 9
RING_RADIUS = 3.0
RUN_DURATION = 10.0  # seconds before auto-quit

cubes = []
frame = 0


def init():
    """Called once by the engine when the script module is loaded."""
    # Position camera above and behind the ring
    rython.camera.set_position(0.0, 6.0, -14.0)
    rython.camera.set_look_at(0.0, 0.0, 0.0)

    # Spawn nine cubes in a ring
    for i in range(CUBE_COUNT):
        angle = (2.0 * math.pi * i) / CUBE_COUNT
        x = math.cos(angle) * RING_RADIUS
        z = math.sin(angle) * RING_RADIUS
        cube = rython.scene.spawn(
            transform=rython.Transform(x=x, y=0.0, z=z, scale=1.0),
            mesh="cube",
            tags=["cube", "spinning"],
        )
        cubes.append(cube)

    # Register the per-frame callback
    rython.scheduler.register_recurring(on_tick)


def on_tick():
    """Called every frame."""
    global frame
    frame += 1
    t = rython.time.elapsed

    # Spin each cube with a phase offset
    for i, cube in enumerate(cubes):
        phase = (2.0 * math.pi * i) / CUBE_COUNT
        cube.transform.rot_y = t + phase

    # Draw HUD text
    rython.renderer.draw_text(
        f"Spinning Cubes  frame={frame}  t={t:.2f}s",
        font_id="default",
        x=0.02,
        y=0.02,
        size=20,
        r=255,
        g=255,
        b=200,
    )

    # Auto-quit after RUN_DURATION
    if t >= RUN_DURATION:
        rython.engine.request_quit()
```

Run it with:

```bash
cargo run --bin rython -- \
    --script-dir scripts \
    --entry-point main
```

---

## @throttle Decorator

`rython.throttle(hz)` is a decorator factory that limits how often a function executes. It is imported directly from the `rython` module:

```python
import rython

@rython.throttle(hz=30)
def update(dt: float) -> None:
    # runs at most 30 times per second
    ...
```

**Parameters:**

| Parameter | Type | Description |
|---|---|---|
| `hz` | `float` | Maximum invocations per second. Must be greater than zero. |

**Behaviour:**

- The first call always executes.
- Subsequent calls that arrive before the minimum interval (`1/hz` seconds) has elapsed are silently skipped — the wrapper returns `None`.
- Uses `rython.time.elapsed` as the clock, so timing is relative to engine time, not wall-clock time.
- If the engine clock resets (e.g. a scene reload causes `rython.time.elapsed` to return a value lower than the last recorded call time), the tracking state is cleared and the next call executes immediately.

**When to use:**

Use `@throttle` on per-frame callbacks registered with `rython.scheduler.register_recurring` when the update does not need to run at full frame rate. This is especially useful for AI ticks, camera smoothing, and UI refreshes:

```python
import rython
from game.scripts import player

OFFSET = (0.0, 8.0, -12.0)

@rython.throttle(hz=30)
def camera_update(dt: float) -> None:
    """Camera follow — capped at 30 Hz, no need to run every frame."""
    px, py, pz = player.get_position()
    rython.camera.set_position(px + OFFSET[0], py + OFFSET[1], pz + OFFSET[2])
    rython.camera.set_look_at(px, py + 1.0, pz)


@rython.throttle(hz=15)
def enemy_update(dt: float) -> None:
    """AI tick — 15 Hz is sufficient for pathfinding updates."""
    ...
```

**Note:** Prefer `rython.scheduler.on_timer` for logic that fires once after a delay. `@throttle` is for functions that are called every frame but should only *run* at a reduced rate. For CPU-intensive work that must not block the main thread, prefer `rython.scheduler.submit_parallel` (same-frame, GIL-held) or `rython.scheduler.submit_background` (off-thread, fire-and-forget) — see [Parallel & Background Tasks](#parallel--background-tasks).

---

## Parallel & Background Tasks

`rython.scheduler` exposes three methods for pushing work outside the normal per-frame callback:

| Method | Executes on | Returns | When done |
|---|---|---|---|
| `submit_background(fn)` | rayon thread pool (off main thread) | `JobHandle` | Future frame, after `flush_python_bg_completions` |
| `submit_parallel(fn)` | Main thread, current tick (GIL held) | `JobHandle` | End of current tick's parallel flush |
| `run_sequential(fn)` | Main thread, next tick | *(nothing)* | Next sequential phase |

### `submit_background`

Submits a zero-argument callable to run on the rayon thread pool. The callable acquires the GIL
independently when it runs, so it can call Python freely. The returned `JobHandle` transitions
from pending to done in a future frame when the completion channel is drained.

```python
import rython

def heavy_work():
    # runs on a rayon thread; can call Python via the GIL
    result = compute_something_expensive()
    rython.scene.emit("heavy_done", value=result)

handle = rython.scheduler.submit_background(heavy_work)

# Poll later if needed (e.g. inside register_recurring)
if handle.is_done and not handle.is_failed:
    print("background task complete")
```

### `submit_parallel`

Submits a zero-argument callable to run on the main thread during the current tick's parallel
flush phase (GIL is already held). The `JobHandle` is done by the end of the same tick — useful
for CPU-intensive Python work that must stay in-frame but should be scheduled explicitly.

```python
handle = rython.scheduler.submit_parallel(my_cpu_work)
# handle.is_done is True by the next register_recurring callback
```

### `run_sequential`

Queues a zero-argument callable to run on the main thread during the *next* tick's sequential
phase. No `JobHandle` is returned. Use `on_timer` or events for continuation logic.

```python
rython.scheduler.run_sequential(lambda: rython.scene.emit("setup_done"))
```

### `JobHandle`

`submit_background` and `submit_parallel` both return a `JobHandle` object:

```python
handle = rython.scheduler.submit_background(my_fn)
```

| Attribute / Method | Type | Description |
|---|---|---|
| `handle.is_done` | `bool` | `True` once the task has finished (success or failure) |
| `handle.is_pending` | `bool` | `True` while the task is still queued or running |
| `handle.is_failed` | `bool` | `True` if the callable raised an exception |
| `handle.error` | `str \| None` | Error message when `is_failed`, otherwise `None` |
| `handle.on_complete(cb)` | — | Register a zero-argument callback. If the task is already done, `cb` fires immediately. |

**Checking completion in a recurring callback:**

```python
import rython

handle = None

def init():
    global handle
    handle = rython.scheduler.submit_background(load_assets)
    rython.scheduler.register_recurring(on_tick)

def load_assets():
    # runs off-thread
    pass

def on_tick():
    if handle and handle.is_done:
        if handle.is_failed:
            print(f"asset load failed: {handle.error}")
        else:
            print("assets ready")
```

**Using `on_complete` for a one-shot reaction:**

```python
def init():
    handle = rython.scheduler.submit_background(load_assets)
    handle.on_complete(lambda: rython.scene.emit("assets_ready"))
```

**Error handling:**

If the callable raises an exception, `is_failed` is `True` and `error` contains the exception
string. `on_complete` callbacks fire regardless of success or failure, so check `is_failed` inside
the callback if the outcome matters.

```python
def on_done():
    if handle.is_failed:
        print(f"Task failed: {handle.error}")
    else:
        apply_result()

handle = rython.scheduler.submit_background(risky_work)
handle.on_complete(on_done)
```

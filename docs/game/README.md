# RythonEngine — Game Scripting Guide

Game logic in RythonEngine is written in Python. The engine exposes a `rython` module through PyO3 bindings that gives scripts full access to the scene, camera, renderer, scheduler, and engine lifecycle.

**See also:** [`docs/engine/`](../engine/README.md) for Rust implementation details, [`.spec/scripting.md`](../../.spec/scripting.md) for the full scripting spec.

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
| `rython.scheduler` | Register per-frame callbacks |
| `rython.renderer` | Queue draw commands (text overlays) |
| `rython.time` | Read elapsed engine time |
| `rython.engine` | Engine lifecycle control |
| `rython.Vec3` | 3D vector type |
| `rython.Transform` | Entity transform type |

Sub-modules `physics`, `audio`, `input`, `ui`, `resources`, and `modules` are present as stubs and will raise `ValueError` if accessed.

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

Position, rotation (Euler angles in radians), and uniform scale for an entity.

```python
t = rython.Transform(x=0.0, y=0.0, z=0.0,
                     rot_x=0.0, rot_y=0.0, rot_z=0.0,
                     scale=1.0)
```

All parameters are keyword-optional and default to `0.0` except `scale` which defaults to `1.0`.

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

`spawn()` returns an `Entity` object. The entity is immediately live in the scene.

**Mesh dict form:**

```python
entity = rython.scene.spawn(
    transform=rython.Transform(x=0.0, y=0.0, z=0.0),
    mesh={"mesh_id": "cube", "texture_id": "stone", "visible": True},
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
        """Called when the entity enters the scene."""
        pass

    def on_despawn(self):
        """Called just before the entity is removed."""
        pass

    def on_collision(self, other_entity):
        """Called when this entity collides with another (physics)."""
        pass

    def on_trigger_enter(self, other_entity):
        """Called when this entity enters a trigger volume."""
        pass

    def on_input_action(self, action_name):
        """Called when a mapped input action fires."""
        pass

entity = rython.scene.spawn(transform=rython.Transform(), mesh="cube")
rython.scene.attach_script(entity, Player)
```

The engine instantiates the class with the entity as the first argument and dispatches the appropriate handler when the corresponding event fires.

---

## Hot-Reload (Dev Mode)

In development builds (`--features dev-reload`), the engine watches `--script-dir` for file changes. When a `.py` file is modified, the engine re-imports it without restarting. The `init()` function is called again on reload.

Hot-reload is not available in release builds. Scripts are bundled into the binary at release time.

---

## Complete Example: Spinning Cubes

This is the visual test script from `tests/visual_spinning_cubes.py`. It demonstrates the full scripting API.

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
    --script-dir tests \
    --entry-point visual_spinning_cubes
```

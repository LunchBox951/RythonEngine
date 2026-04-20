# RythonEngine — Game Scripting Guide

Game logic in RythonEngine is written in Python. The engine exposes a `rython` module through PyO3 bindings that gives scripts full access to the scene, camera, renderer, scheduler, input, audio, physics, UI, resources, and engine lifecycle.

**See also:** [`docs/engine/`](../engine/README.md) for Rust implementation details.

---

## Contents

1. [Quick Start](#quick-start)
2. [Entry Point Convention](#entry-point-convention)
3. [The `rython` Module](#the-rython-module)
4. [IDE Support (Stubs)](#ide-support-stubs)
5. [Type Wrappers](#type-wrappers)
6. [Spawning Entities](#spawning-entities)
7. [Entity API](#entity-api)
8. [Camera Control](#camera-control)
9. [Per-Frame Updates](#per-frame-updates)
10. [Custom Events](#custom-events)
11. [Input](#input)
12. [Renderer](#renderer)
13. [Audio](#audio)
14. [Physics](#physics)
15. [UI](#ui)
16. [Resources](#resources)
17. [Time](#time)
18. [Quitting](#quitting)
19. [Script Classes](#script-classes)
20. [@throttle Decorator](#throttle-decorator)
21. [Parallel & Background Tasks](#parallel--background-tasks)
22. [Hot-Reload (Dev Mode)](#hot-reload-dev-mode)
23. [Complete Example: Spinning Cubes](#complete-example-spinning-cubes)

---

## Quick Start

```bash
# Build the engine
make build

# Install Python stubs for IDE autocompletion
make stubs

# Run your game
make run SCRIPT_DIR=game SCRIPT=game.scripts.main
```

Create a file at `game/scripts/main.py`:

```python
import rython

def init():
    rython.camera.set_position(0.0, 5.0, -10.0)
    rython.camera.set_look_at(0.0, 0.0, 0.0)

    cube = rython.scene.spawn(
        transform=rython.Transform(x=0.0, y=0.0, z=0.0),
        mesh="cube",
        tags=["spinning"],
    )

    def on_tick():
        cube.transform.rot_y += 0.02
        rython.renderer.draw_text(f"t={rython.time.elapsed:.2f}s", x=0.02, y=0.02)

    rython.scheduler.register_recurring(on_tick)
```

---

## Entry Point Convention

The engine loads scripts from the directory specified by `--script-dir` (default: `./scripts`). The entry point module is specified by `--entry-point` (default: `main`).

The engine imports the module and calls `init()` once on load:

```python
# game/scripts/main.py
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
| `rython.scene` | Spawn/despawn entities, emit and subscribe to events, attach scripts |
| `rython.camera` | Control the camera position and orientation |
| `rython.scheduler` | Register per-frame callbacks; one-shot timers and events; parallel/background task submission |
| `rython.renderer` | Draw text overlays, control lighting, shadows, and clear color |
| `rython.time` | Read elapsed engine time |
| `rython.engine` | Engine lifecycle control |
| `rython.physics` | Physics world control (gravity) |
| `rython.audio` | Audio playback and volume control |
| `rython.input` | Per-frame input state queries (axis, pressed, held, released) |
| `rython.ui` | UI widget creation, layout, theming |
| `rython.resources` | Asset loading (images, meshes, sounds, fonts, spritesheets) |
| `rython.Vec3` | 3D vector type |
| `rython.Transform` | Entity transform type |

`rython.modules` is a stub and will raise `ValueError` if accessed. All other sub-modules listed above are fully implemented bridges.

---

## IDE Support (Stubs)

The `rython/` directory in the project root contains pure-Python stub files that provide type annotations for IDE autocompletion (Pylance, pyright, etc.). Install them with:

```bash
make stubs
```

This creates a virtual environment at `.venv/` and installs the stub package in editable mode. Your IDE should then provide full autocompletion and type-checking for all `rython.*` APIs.

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
    mesh="cube",
    tags=["player", "solid"],
)
```

| kwarg | Type | Description |
|---|---|---|
| `transform` | `rython.Transform` | Initial position/rotation/scale |
| `mesh` | `str` or `dict` | Mesh to render (see below) |
| `tags` | `list[str]` | Tag strings attached to the entity |
| `rigid_body` | `dict` | Attach a physics body (see below) |
| `collider` | `dict` | Attach a collision shape (see below) |
| `light` | `dict` | Attach a light source (see below) |

`spawn()` returns an `Entity` object. The entity is immediately live in the scene.

### Mesh

The `mesh` parameter accepts either a simple string or a dict for full control:

```python
# Simple form — just a mesh ID:
mesh="cube"

# Dict form — full material control:
mesh={
    "mesh_id": "cube",           # required — mesh asset ID
    "texture_id": "stone",       # diffuse texture asset ID (default: "")
    "visible": True,             # render visibility (default: True)

    # PBR material properties
    "normal_map": "stone_n",     # normal map asset ID
    "specular_map": "stone_s",   # specular map asset ID
    "shininess": 32.0,           # specular exponent (default: 32.0)
    "specular_color": (1.0, 1.0, 1.0),  # RGB tuple [0, 1]
    "metallic": 0.0,             # metalness [0, 1] (default: 0.0)
    "roughness": 0.5,            # roughness [0, 1] (default: 0.5)

    # Emissive properties
    "emissive_map": "lava_e",    # emissive map asset ID
    "emissive_color": (1.0, 0.3, 0.1),  # RGB tuple [0, 1]
    "emissive_intensity": 1.0,   # emission strength (default: 1.0, min: 0.0)
}
```

### Rigid Body

```python
rigid_body={
    "body_type": "dynamic",    # "dynamic", "static", or "kinematic" (default: "dynamic")
    "mass": 1.0,               # float (default: 1.0)
    "gravity_factor": 1.0,     # float (default: 1.0)
}
```

### Collider

```python
collider={
    "shape": "box",            # "box", "sphere", or "capsule" (default: "box")
    "size": [1.0, 1.0, 1.0],  # [x, y, z] dimensions
    "is_trigger": False,       # trigger mode — no physics response, only events (default: False)
}
```

### Light

```python
# Directional light (default)
light={
    "type": "directional",             # default if omitted
    "color": (1.0, 1.0, 1.0),         # RGB tuple [0, 1] (default: white)
    "intensity": 1.0,                  # brightness multiplier (default: 1.0)
    "direction": (0.5, 1.0, 0.5),     # world-space direction (default: (0.5, 1.0, 0.5))
}

# Point light
light={
    "type": "point",
    "color": (1.0, 0.9, 0.7),
    "intensity": 2.0,
    "radius": 10.0,                    # falloff radius (default: 10.0)
}

# Spot light
light={
    "type": "spot",
    "color": (1.0, 1.0, 1.0),
    "intensity": 3.0,
    "direction": (0.0, -1.0, 0.0),    # aim direction (default: straight down)
    "inner_angle": 15.0,              # inner cone angle in degrees (default: 15.0)
    "outer_angle": 30.0,              # outer cone angle in degrees (default: 30.0)
}
```

### Full spawn example

```python
ball = rython.scene.spawn(
    transform=rython.Transform(x=0.0, y=5.0, z=0.0),
    mesh={"mesh_id": "sphere", "metallic": 0.8, "roughness": 0.2},
    rigid_body={"body_type": "dynamic", "mass": 2.0},
    collider={"shape": "sphere", "size": [0.5, 0.5, 0.5]},
    tags=["ball", "physics"],
)

sun = rython.scene.spawn(
    light={"type": "directional", "direction": (0.5, -1.0, 0.3), "intensity": 1.5},
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

# Physics (requires a rigid_body on the entity)
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

## Input

`rython.input` has two layers:

  * **Polling** — per-frame action state that game logic reads inside `tick()`.
  * **Customizable maps** — designer-authored `InputMap` subclasses that declare
    actions, bind them to hardware, and register callbacks.

### Polling

```python
# Scalar axis (or magnitude for 2D/3D)
move_x = rython.input.axis("move_x")
move_z = rython.input.axis("move_z")

# Full 2D / 3D value
x, y       = rython.input.axis2("move")
x, y, z    = rython.input.axis3("fly")

# Typed ActionValue; returns None if unbound
v = rython.input.value("move")
if v is not None and v.kind == "axis2d":
    x, y = v.as_vec2()

# Button state queries
if rython.input.pressed("jump"):     # first frame pressed
    player.apply_impulse(0.0, 10.0, 0.0)

if rython.input.held("sprint"):      # every frame held
    speed *= 2.0

if rython.input.released("fire"):    # first frame released
    stop_charging()
```

| Method | Returns | Description |
|---|---|---|
| `axis(action)` | `float` | 1D value (or magnitude for 2D/3D); 0.0 if unbound |
| `axis2(action)` | `(float, float)` | 2D axis value; (0, 0) if unbound |
| `axis3(action)` | `(float, float, float)` | 3D axis value; (0, 0, 0) if unbound |
| `value(action)` | `ActionValue \| None` | Typed value carrying the action's kind |
| `pressed(action)` | `bool` | True on the first frame the action is pressed |
| `held(action)` | `bool` | True every frame the action is held |
| `released(action)` | `bool` | True on the first frame the action is released |

### Custom InputMaps

Designers create their own input layout by subclassing `rython.InputMap`, and
push an instance onto a priority-ordered context stack at runtime. Higher-
priority maps evaluate first; an action actuated by a higher-priority map
**consumes** it for lower maps (so a pause-menu map cleanly shadows gameplay
input).

```python
import rython
from rython import (
    InputMap, KeyCode, GamepadButton, GamepadStick, Modifiers, Triggers,
)

class MovementMap(InputMap):
    def __init__(self, *args, **kwargs):
        # Construction args (name/priority) were consumed by __new__.
        # Declare actions and bindings here.

        # Button with a rising-edge trigger
        self.jump = self.action("jump", kind="button")
        self.jump.bind(KeyCode.Space, triggers=[Triggers.Pressed()])
        self.jump.bind(GamepadButton.South, triggers=[Triggers.Pressed()])
        self.jump.on_started(self.handle_jump)

        # 2D composite move + gamepad stick
        self.move = self.action("move", kind="axis2d")
        self.move.bind_composite_2d(
            up=KeyCode.W, down=KeyCode.S, left=KeyCode.A, right=KeyCode.D,
        )
        self.move.bind(GamepadStick.LeftStick, modifiers=[
            Modifiers.DeadZone(0.15, radial=True),
        ])
        self.move.on_triggered(self.handle_move)

        # Hold trigger with Ongoing + Triggered progress reporting
        self.charge = self.action("charge", kind="button")
        self.charge.bind(KeyCode.F, triggers=[Triggers.Hold(0.5)])
        self.charge.on_ongoing(lambda v: print("charging…"))
        self.charge.on_triggered(lambda v: print("charged!"))

    def handle_jump(self, value):
        ...

    def handle_move(self, value):
        x, y = value.as_vec2()
        ...

def init():
    rython.input.push_map(MovementMap(name="gameplay", priority=10))
```

The `rython/input/default.py` module reinstates the old hardcoded bindings
(`move_x / move_z / jump / pause`) on this API — call
`rython.input.push_map(build_default_map())` to keep the old names working.

#### Modifiers

Stateless per-binding transforms applied in declaration order to the raw
hardware sample.

| Factory | Purpose |
|---|---|
| `Modifiers.Negate(x=False, y=False, z=False)` | Flip the sign of selected axes |
| `Modifiers.Scale(x=1.0, y=1.0, z=1.0)` | Component-wise multiply |
| `Modifiers.DeadZone(lower, upper=1.0, radial=False)` | Axial or radial deadzone; below `lower` → 0, between `lower` and `upper` → linear rescale to `[0, 1]` |
| `Modifiers.Swizzle(order)` | Axis reorder; `order` is `"XYZ"` / `"YXZ"` / `"ZXY"` / `"YZX"` |

#### Triggers

Stateful per-binding state machines reporting one of `None`, `Ongoing`,
`Triggered`, or `Canceled` per frame. A binding with no explicit trigger
behaves like `Triggers.Down()`.

| Factory | Semantics |
|---|---|
| `Triggers.Down()` | Fires every frame the input is actuated |
| `Triggers.Pressed()` | Fires on the rising edge (frame of actuation) |
| `Triggers.Released()` | Fires on the falling edge (frame of release) |
| `Triggers.Hold(seconds)` | `Ongoing` while charging; `Triggered` after `seconds` of continuous hold; `Canceled` if released early |
| `Triggers.Tap(max_seconds=0.25)` | Fires once on release if total hold stayed under `max_seconds`; `Canceled` if held longer |
| `Triggers.Pulse(interval_seconds)` | Fires on initial press and every `interval_seconds` while held |
| `Triggers.Chorded(partner)` | Fires only when actuated AND `partner` action is actuated this frame (partner must be declared earlier in the same map) |

#### Callback phases

Attach handlers per phase. Each is called with an `ActionValue`.

| Phase | Handler | Meaning |
|---|---|---|
| `Started` | `on_started(cb)` | First frame the action becomes active |
| `Ongoing` | `on_ongoing(cb)` | In-progress (e.g. Hold still charging) |
| `Triggered` | `on_triggered(cb)` | Action is actuating this frame |
| `Completed` | `on_completed(cb)` | Clean falling edge after `Triggered` |
| `Canceled` | `on_canceled(cb)` | Aborted (e.g. Tap held past `max_seconds`) |

#### Context stack

| Call | Effect |
|---|---|
| `rython.input.push_map(m)` | Push *m* onto the stack; higher priority wins on conflicts |
| `rython.input.pop_map(id)` | Remove the pushed map with `name == id` |
| `rython.input.clear_maps()` | Remove every pushed map |
| `rython.input.active_maps()` | List of active map ids, priority-descending |

When a higher-priority map actuates an action, that action id is **consumed**
for the frame — lower-priority maps skip their binding for it. Pop the menu
context to restore gameplay input.

#### Rebinding

```python
rython.input.rebind("gameplay", "jump", 0, KeyCode.Enter)
```

Replaces the hardware key at `(map_id, action_id, binding_index)`. Useful for
in-game settings UIs. Combine with a "press any key" UI that polls
`rython.scene.subscribe("input:…:started", …)` to build a rebind capture.

#### Scene-bus events

Every phase change is also emitted on the scene bus under
`input:{action}:{phase}` (e.g. `input:jump:started`), with payload
`{"value": <...>, "phase": "...", "elapsed_seconds": <float>}`. Subscribing
to this pattern works for ad-hoc event wiring without a full InputMap
subclass.

---

## Renderer

`rython.renderer` manages text overlays, scene lighting, and shadow configuration.

### Drawing Text

Queue a text overlay draw command for the current frame:

```python
rython.renderer.draw_text(
    "Hello World",
    font_id="default",   # font asset ID (default: "default")
    x=0.5,               # normalized screen X (0.0 = left, 1.0 = right)
    y=0.1,               # normalized screen Y (0.0 = top, 1.0 = bottom)
    size=16,             # font size in pixels
    r=255,               # red channel (0-255)
    g=255,               # green channel
    b=255,               # blue channel
    z=0.0,               # depth sort order (higher = on top)
)
```

All parameters except `text` are optional and use the defaults shown above.

### Clear Color

Set the framebuffer clear color (linear RGBA, each component [0, 1]):

```python
rython.renderer.set_clear_color(0.1, 0.1, 0.15, 1.0)
```

### Directional Light

Configure the primary directional light:

```python
rython.renderer.set_light_direction(0.5, -1.0, 0.5)  # world-space direction (auto-normalized)
rython.renderer.set_light_color(1.0, 1.0, 0.9)       # linear RGB [0, 1]
rython.renderer.set_light_intensity(1.2)               # brightness multiplier
```

### Ambient Light

Set scene-wide ambient light (linear RGB [0, 1]):

```python
rython.renderer.set_ambient_light(r=0.15, g=0.15, b=0.2, intensity=1.0)
```

### Shadow Mapping

Configure shadow casting from the primary directional light:

```python
rython.renderer.set_shadow_enabled(True)
rython.renderer.set_shadow_map_size(1024)    # 512, 1024, 2048, or 4096
rython.renderer.set_shadow_bias(0.005)       # prevents shadow acne (default: 0.005)
rython.renderer.set_shadow_pcf(4)            # 1 = no filtering, >= 4 = 3x3 kernel
```

---

## Audio

`rython.audio` manages sound playback with categories and volume control.

```python
# Play a sound — returns an integer handle
handle = rython.audio.play("game/assets/sounds/impact.ogg", category="sfx", looping=False)

# Play looping background music
music = rython.audio.play("game/assets/music/theme.ogg", category="music", looping=True)

# Stop a specific sound by handle (idempotent)
rython.audio.stop(handle)

# Stop all sounds in a category
rython.audio.stop_category("music")

# Set volume per category (0.0 to 1.0)
rython.audio.set_volume("sfx", 0.8)
rython.audio.set_volume("music", 0.5)

# Set master volume (0.0 to 1.0)
rython.audio.set_master_volume(0.9)
```

| Method | Returns | Description |
|---|---|---|
| `play(path, category="sfx", looping=False)` | `int` | Play sound, return handle |
| `stop(handle)` | — | Stop sound by handle (idempotent) |
| `stop_category(category)` | — | Stop all sounds in a category |
| `set_volume(category, volume)` | — | Set category volume [0.0, 1.0] |
| `set_master_volume(volume)` | — | Set master volume [0.0, 1.0] |

---

## Physics

`rython.physics` controls the physics simulation. Individual entity physics are managed through the [Entity API](#entity-api).

```python
# Set the gravity vector (default: 0.0, -9.81, 0.0)
rython.physics.set_gravity(0.0, -20.0, 0.0)

# Zero gravity
rython.physics.set_gravity(0.0, 0.0, 0.0)
```

Per-entity physics operations (`apply_force`, `apply_impulse`, `set_velocity`, `velocity`) are on the `Entity` object — see [Entity API](#entity-api).

---

## UI

`rython.ui` provides a widget system for in-game UI. All coordinates are normalized screen space [0, 1].

### Creating Widgets

```python
# Create widgets — all return an integer widget ID
label  = rython.ui.create_label("Score: 0", x=0.3, y=0.3, w=0.4, h=0.1)
button = rython.ui.create_button("Play", x=0.35, y=0.5, w=0.3, h=0.1)
panel  = rython.ui.create_panel(x=0.2, y=0.2, w=0.6, h=0.6)
text_input = rython.ui.create_text_input("Enter name...", x=0.3, y=0.4, w=0.4, h=0.08)
```

### Widget Hierarchy

```python
rython.ui.add_child(panel, label)
rython.ui.add_child(panel, button)
```

### Layout

```python
# direction: "none", "vertical", or "horizontal"
rython.ui.set_layout(panel, direction="vertical", spacing=10.0, padding=8.0)
```

### Visibility

```python
rython.ui.show(panel)               # make visible (children inherit)
rython.ui.hide(panel)               # hide
visible = rython.ui.is_visible(panel)  # True if widget and all ancestors are visible
```

### Content and Events

```python
rython.ui.set_text(label, "Score: 1500")
rython.ui.on_click(button, lambda: rython.scene.emit("start_game"))
```

### Theming

```python
rython.ui.set_theme(
    button_color=(60, 60, 80),      # RGB 0-255
    text_color=(220, 220, 220),
    panel_color=(30, 30, 40),
    border_color=(100, 100, 120),
    font_size=18,
)
```

All parameters are optional — unspecified fields keep their current value.

### Loading Layouts from JSON

Load UI layouts exported from the editor:

```python
widgets = rython.ui.load_layout("game/ui/main_menu.json")
# widgets is a dict mapping widget name → runtime widget ID
rython.ui.on_click(widgets["play_button"], start_game)
rython.ui.on_click(widgets["quit_button"], rython.engine.request_quit)
```

---

## Resources

`rython.resources` provides streaming asset loading with status tracking.

### Loading Assets

All load methods return an `AssetHandle`:

```python
img    = rython.resources.load_image("game/assets/textures/player.png")
mesh   = rython.resources.load_mesh("game/assets/models/tree.gltf")
sound  = rython.resources.load_sound("game/assets/sounds/hit.wav")
font   = rython.resources.load_font("game/assets/fonts/mono.ttf", size=20.0)
sheet  = rython.resources.load_spritesheet("game/assets/sprites/walk.png", cols=4, rows=2)
```

### AssetHandle

```python
handle = rython.resources.load_image("game/assets/textures/player.png")

handle.is_ready    # bool — loaded successfully
handle.is_pending  # bool — still loading
handle.is_failed   # bool — load failed
handle.error       # str | None — error message if failed
```

### Memory

```python
used   = rython.resources.memory_used_mb()    # current asset memory usage (MB)
budget = rython.resources.memory_budget_mb()   # configured LRU eviction budget (MB)
```

---

## Time

```python
t = rython.time.elapsed   # float — seconds since engine start (monotonically increasing)
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
        """Called when this entity collides with another (requires rigid_body + collider).
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

## @throttle Decorator

`rython.throttle(hz)` is a decorator factory that limits how often a function executes:

```python
import rython

@rython.throttle(hz=30)
def update():
    # runs at most 30 times per second
    ...
```

| Parameter | Type | Description |
|---|---|---|
| `hz` | `float` | Maximum invocations per second. Must be > 0. |

**Behaviour:**

- The first call always executes.
- Subsequent calls that arrive before `1/hz` seconds have elapsed are silently skipped (returns `None`).
- Uses `rython.time.elapsed` as the clock (engine time, not wall-clock time).
- If the engine clock resets (e.g. scene reload), tracking state is cleared and the next call executes immediately.

**When to use:** on per-frame callbacks that don't need to run at full frame rate — AI ticks, camera smoothing, UI refreshes:

```python
@rython.throttle(hz=30)
def camera_follow():
    px = player.transform.x
    py = player.transform.y
    pz = player.transform.z
    rython.camera.set_position(px, py + 8.0, pz - 12.0)
    rython.camera.set_look_at(px, py + 1.0, pz)

rython.scheduler.register_recurring(camera_follow)
```

Prefer `rython.scheduler.on_timer` for logic that fires once after a delay. For CPU-intensive work, use `submit_parallel` or `submit_background` (see below).

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
def heavy_work():
    result = compute_something_expensive()
    rython.scene.emit("heavy_done", value=result)

handle = rython.scheduler.submit_background(heavy_work)
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

| Attribute / Method | Type | Description |
|---|---|---|
| `handle.is_done` | `bool` | `True` once the task has finished (success or failure) |
| `handle.is_pending` | `bool` | `True` while the task is still queued or running |
| `handle.is_failed` | `bool` | `True` if the callable raised an exception |
| `handle.error` | `str \| None` | Error message when `is_failed`, otherwise `None` |
| `handle.on_complete(cb)` | — | Register a zero-argument callback. If the task is already done, `cb` fires immediately. |

**Checking completion in a recurring callback:**

```python
handle = None

def init():
    global handle
    handle = rython.scheduler.submit_background(load_assets)
    rython.scheduler.register_recurring(on_tick)

def load_assets():
    pass  # runs off-thread

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

---

## Hot-Reload (Dev Mode)

In development builds (`--features dev-reload`), the engine watches `--script-dir` for file changes. When a `.py` file is modified, the engine re-imports it without restarting. The `init()` function is called again on reload.

Hot-reload is not available in release builds. Scripts are bundled into the binary at release time.

---

## Complete Example: Spinning Cubes

This example demonstrates the core scripting API: spawning entities, per-frame updates, camera setup, lighting, and HUD text.

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
    # Configure rendering
    rython.renderer.set_clear_color(0.1, 0.1, 0.15, 1.0)
    rython.renderer.set_ambient_light(r=0.15, g=0.15, b=0.2, intensity=1.0)
    rython.renderer.set_shadow_enabled(True)

    # Position camera above and behind the ring
    rython.camera.set_position(0.0, 6.0, -14.0)
    rython.camera.set_look_at(0.0, 0.0, 0.0)

    # Add a directional light
    rython.scene.spawn(
        light={"type": "directional", "direction": (0.5, -1.0, 0.3), "intensity": 1.2},
    )

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
make run SCRIPT_DIR=game SCRIPT=game.scripts.main
```

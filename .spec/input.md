# Input

The input system provides a unified abstraction over keyboard, mouse, and gamepad input. It converts raw hardware events from winit and gilrs into logical game actions, so game scripts never need to know which physical device the player is using.


## Architecture

The input system has three layers:

1. **InputBackend**: Low-level hardware polling. Two implementations: keyboard/mouse (via winit events) and gamepad (via gilrs). The system auto-detects gamepads and switches backends when a gamepad connects or disconnects.

2. **InputMap**: Logical binding layer. Maps hardware inputs to named game actions like "move_x", "jump", "attack". Multiple maps can be swapped at runtime (e.g., ground movement vs. menu navigation).

3. **PlayerController**: High-level module that game scripts interact with. Provides per-frame input snapshots and fires input events on the Scene's event bus.


## PlayerController

The PlayerController is a Module that runs a recurring task at PRE_UPDATE priority. Each frame, it:

1. Polls the active InputBackend for raw state
2. Applies the active InputMap to produce logical action values
3. Builds an InputSnapshot containing all action states
4. Emits InputActionEvents for any actions that changed this frame

```python
import rython

# Read current input state
snapshot = rython.input.get_snapshot()

# Axis values are floats from -1.0 to 1.0
move_x = snapshot.axis("move_x")
move_y = snapshot.axis("move_y")

# Button states
if snapshot.pressed("jump"):
    # Jump was just pressed this frame
    pass

if snapshot.held("sprint"):
    # Sprint is being held down
    pass

if snapshot.released("attack"):
    # Attack was just released this frame
    pass
```


## Input Map

An InputMap defines bindings between hardware inputs and logical actions. There are two binding types:

### Axis Bindings
Map a logical axis to one of:
- **KBAxis**: Two keyboard keys (negative and positive). Example: A/D for horizontal movement.
- **MouseAxis**: Mouse movement on X or Y axis.
- **GamepadAxis**: A gamepad stick axis (left stick X, right stick Y, triggers).

```python
# Define an input map
movement_map = rython.InputMap("movement")

# Keyboard axis: A = negative, D = positive
movement_map.bind_axis("move_x", keyboard=("A", "D"))
movement_map.bind_axis("move_y", keyboard=("S", "W"))

# Gamepad axis: left stick
movement_map.bind_axis("move_x", gamepad="left_stick_x")
movement_map.bind_axis("move_y", gamepad="left_stick_y")

# Mouse look
movement_map.bind_axis("look_x", mouse="x")
movement_map.bind_axis("look_y", mouse="y")
```

### Button Bindings
Map a logical button to one or more hardware buttons. A binding can list alternatives for different devices.

```python
movement_map.bind_button("jump", keyboard="SPACE", gamepad="south")
movement_map.bind_button("attack", keyboard="MOUSE_LEFT", gamepad="right_trigger")
movement_map.bind_button("sprint", keyboard="LEFT_SHIFT", gamepad="left_stick_press")
```

### Map Switching
Multiple maps can be registered. Only one is active at a time. Swap maps to change control schemes.

```python
menu_map = rython.InputMap("menu")
menu_map.bind_button("confirm", keyboard="ENTER", gamepad="south")
menu_map.bind_button("back", keyboard="ESCAPE", gamepad="east")
menu_map.bind_axis("navigate_y", keyboard=("DOWN", "UP"), gamepad="left_stick_y")

# Switch to menu controls
rython.input.set_active_map("menu")

# Switch back to gameplay controls
rython.input.set_active_map("movement")
```


## Event-Driven Input

In addition to polling via snapshots, the input system fires events on the Scene's event bus. Scripts can subscribe to input events instead of polling:

```python
class PlayerScript:
    def on_input_action(self, action, value):
        if action == "jump" and value > 0:
            rython.physics.apply_impulse(self.entity, (0, 50, 0))

        if action == "move_x":
            # value is -1.0 to 1.0
            vel = rython.physics.get_linear_velocity(self.entity)
            rython.physics.set_linear_velocity(self.entity, (value * 10, vel.y, vel.z))
```


## Input Locking

Input can be locked to prevent game scripts from receiving input events. This is used during pause menus, dialogue, or cutscenes. When locked, the PlayerController still polls hardware (so the unlock input works), but does not emit events or update the snapshot for game scripts.

```python
# Lock all input (e.g., when opening pause menu)
rython.input.lock()

# Unlock
rython.input.unlock()

# Check if locked
if rython.input.is_locked():
    pass
```


## Ownership Transfer

The PlayerController is a single-owner Module. Only the owning module can read input and change the active map. This prevents multiple systems from fighting over input state.

During gameplay, the game module owns input. During a cutscene, ownership transfers to the cutscene module (which may lock input or switch to a cinematic input map).

```python
rython.modules.transfer_ownership("PlayerController", new_owner=cutscene_module)
```


## Gamepad Detection

The system uses gilrs to detect gamepads. When a gamepad connects, the backend switches from keyboard/mouse to gamepad automatically. When the gamepad disconnects, it falls back to keyboard/mouse.

Supported gamepads include PlayStation (DualSense/DualShock) and Xbox controllers. The InputMap supports device-specific button names for both.

```python
# Check what backend is active
backend = rython.input.active_backend()  # "keyboard_mouse" or "gamepad"

# Get gamepad info if connected
gamepad = rython.input.gamepad_info()
# { "name": "DualSense", "vendor": "Sony", "type": "ps5" }
```


## Acceptance Tests

### T-INP-01: Keyboard Axis — Positive Key
Bind axis "move_x" to keyboard ("A", "D"). Simulate pressing D (positive key).
- Expected: `snapshot.axis("move_x")` returns 1.0

### T-INP-02: Keyboard Axis — Negative Key
Same binding. Simulate pressing A (negative key).
- Expected: `snapshot.axis("move_x")` returns -1.0

### T-INP-03: Keyboard Axis — Both Keys
Simulate pressing both A and D simultaneously.
- Expected: `snapshot.axis("move_x")` returns 0.0 (cancel out)

### T-INP-04: Keyboard Axis — No Keys
Neither A nor D is pressed.
- Expected: `snapshot.axis("move_x")` returns 0.0

### T-INP-05: Button Press/Hold/Release Lifecycle
Bind button "jump" to SPACE. Simulate: frame 1 press SPACE, frame 2 hold SPACE, frame 3 release SPACE, frame 4 nothing.
- Expected: Frame 1: `pressed("jump")` = true, `held("jump")` = true, `released("jump")` = false
- Expected: Frame 2: `pressed("jump")` = false, `held("jump")` = true, `released("jump")` = false
- Expected: Frame 3: `pressed("jump")` = false, `held("jump")` = false, `released("jump")` = true
- Expected: Frame 4: `pressed("jump")` = false, `held("jump")` = false, `released("jump")` = false

### T-INP-06: Input Map Switching
Create two maps: "gameplay" with "jump" bound to SPACE, "menu" with "confirm" bound to ENTER. Activate "gameplay".
- Expected: `snapshot.pressed("jump")` works when SPACE is pressed
- Expected: `snapshot.pressed("confirm")` returns false (not in active map)
- Expected: Switch to "menu". Now `snapshot.pressed("confirm")` works with ENTER
- Expected: `snapshot.pressed("jump")` returns false (no longer active)

### T-INP-07: Unbound Action Returns Default
Query axis "nonexistent_action" from the snapshot.
- Expected: Returns 0.0 (not an error or panic)
- Expected: Query button "nonexistent" returns false for pressed/held/released

### T-INP-08: Input Locking — Events Suppressed
Lock input. Simulate pressing SPACE (bound to "jump").
- Expected: `snapshot.pressed("jump")` returns false (input is locked)
- Expected: No InputActionEvent is emitted on the event bus

### T-INP-09: Input Locking — Unlock Restores
Lock input. Unlock input. Simulate pressing SPACE.
- Expected: `snapshot.pressed("jump")` returns true (input unlocked)
- Expected: InputActionEvent IS emitted

### T-INP-10: Event-Driven Input Fires Events
Bind "jump" to SPACE. Subscribe a handler to InputActionEvent. Simulate pressing SPACE.
- Expected: Handler is called with action="jump", value=1.0
- Expected: Handler is called exactly once per press (not per frame)

### T-INP-11: Ownership Transfer — Non-Owner Rejected
Owner A owns PlayerController. Owner B attempts to call `get_snapshot()`.
- Expected: Owner B's call returns an error or empty snapshot
- Expected: Owner A's calls continue to work

### T-INP-12: Ownership Transfer — Transfer Succeeds
Owner A transfers PlayerController to Owner B.
- Expected: Owner B can now call `get_snapshot()` and `set_active_map()`
- Expected: Owner A's subsequent calls are rejected

### T-INP-13: Gamepad Axis Range
Simulate a gamepad left stick at full deflection on X axis.
- Expected: `snapshot.axis("move_x")` returns exactly 1.0 (or -1.0 for opposite)
- Expected: Stick at rest returns 0.0 (within a small deadzone)

### T-INP-14: Multiple Bindings Same Action
Bind "move_x" to both keyboard (A/D) AND gamepad (left_stick_x). Press D on keyboard.
- Expected: `snapshot.axis("move_x")` returns 1.0 (keyboard binding active)
- Expected: If gamepad stick is also deflected, the higher absolute value wins (or the active backend's value is used)

# Physics

The physics system integrates rapier3d for 3D rigid-body simulation, replacing PythonEngine's PyBullet. It runs as a Module that submits a recurring sequential task to the scheduler at GAME_UPDATE priority.


## Per-Frame Sync Cycle

Each frame, the physics system executes a fixed pipeline:

1. **Register/Unregister**: Check for entities that gained or lost RigidBodyComponent/ColliderComponent. Create or destroy corresponding rapier bodies.
2. **Push transforms**: For STATIC and KINEMATIC bodies, copy the entity's world transform (from the TransformSystem) into the rapier world. This lets game code move kinematic platforms by modifying TransformComponent.
3. **Step simulation**: Call rapier's `step()` with the frame timestep.
4. **Pull transforms**: For DYNAMIC bodies, copy rapier's computed positions/rotations back into the entity's TransformComponent. Physics-driven objects update their position this way.
5. **Query collisions**: Check rapier's contact and intersection events. Emit CollisionEvent and TriggerEvent on the Scene's event bus.

```
ECS (TransformComponent)  --(push)-->  Rapier World  --(step)-->  Rapier World  --(pull)-->  ECS
      [static/kinematic]                                              [dynamic]
```


## Body Types

- **Static**: Does not move. Used for walls, floors, terrain. Transforms are pushed from ECS but never pulled.
- **Dynamic**: Driven by physics forces. Transforms are pulled from rapier after each step.
- **Kinematic**: Moved by game code, not by forces. Transforms are pushed from ECS. Kinematic bodies push dynamic bodies on collision but are not affected themselves.

```python
import rython

# Create a dynamic physics body
scene.attach(entity, rython.RigidBodyComponent(
    body_type="dynamic",
    mass=10.0,
    gravity_factor=1.0,
))

# Add a box collider
scene.attach(entity, rython.ColliderComponent(
    shape="box",
    size=(1.0, 1.0, 1.0),
))
```


## Collision Layers

Bodies are assigned to collision layers (groups) and masks. A body only collides with bodies whose layer matches its mask. This allows efficient filtering: projectiles can pass through allies, triggers can detect only the player, etc.

```python
# Layer definitions (game-defined constants)
LAYER_PLAYER = 1
LAYER_ENEMY = 2
LAYER_PROJECTILE = 4
LAYER_ENVIRONMENT = 8

scene.attach(entity, rython.RigidBodyComponent(
    body_type="dynamic",
    mass=1.0,
    collision_layer=LAYER_PLAYER,
    collision_mask=LAYER_ENEMY | LAYER_ENVIRONMENT,
))
```


## Trigger Volumes

A trigger volume is a collider with `is_trigger=True`. Triggers detect overlap but do not produce contact forces. They are used for gameplay zones: checkpoints, damage areas, spawn triggers.

Triggers emit TriggerEvent with enter/exit flags when entities enter or leave the volume.

```python
# Create a trigger zone
scene.attach(zone_entity, rython.ColliderComponent(
    shape="box",
    size=(5.0, 3.0, 5.0),
    is_trigger=True,
))

# React to triggers in a script
class CheckpointScript:
    def on_trigger_enter(self, other):
        if other.has_tag("player"):
            rython.audio.play("checkpoint_ding", category="sfx")
            # Save progress...

    def on_trigger_exit(self, other):
        pass
```


## Collision Events

When two non-trigger bodies collide, a CollisionEvent is emitted on the Scene's event bus. The event contains:
- The two entity IDs involved
- The collision normal (direction of impact)
- The contact point (world-space position where bodies met)

```python
class CrateScript:
    def on_collision(self, other, normal):
        # Break the crate if hit hard enough
        velocity = self.entity.get_linear_velocity()
        if velocity.length() > 10.0:
            self.entity.despawn()
            rython.audio.play("crate_break", category="sfx")
```


## 2D Locking

For 2D games, the physics system supports locking bodies to a plane:

- **XZ plane lock** (top-down): Bodies cannot move or rotate out of the XZ plane. Useful for top-down games.
- **XY plane lock** (side-scroller): Bodies cannot move or rotate out of the XY plane. Useful for platformers.

```python
rython.physics.set_2d_mode("xz")  # Top-down
rython.physics.set_2d_mode("xy")  # Side-scroller
rython.physics.set_2d_mode(None)  # Full 3D (default)
```


## Forces and Impulses

Game scripts can apply forces and impulses to dynamic bodies:

```python
# Apply a continuous force (e.g., thrust)
rython.physics.apply_force(entity, force=(0, 100, 0))

# Apply an instant impulse (e.g., jump)
rython.physics.apply_impulse(entity, impulse=(0, 50, 0))

# Set velocity directly
rython.physics.set_linear_velocity(entity, velocity=(5, 0, 0))

# Get current velocity
vel = rython.physics.get_linear_velocity(entity)
```


## Configuration

```json
{
    "physics": {
        "gravity": [0.0, -9.81, 0.0],
        "fixed_timestep": 0.016666,
        "max_substeps": 4,
        "lock_2d": null
    }
}
```

- `gravity`: World gravity vector (default: Earth gravity downward)
- `fixed_timestep`: Physics step size in seconds (default: ~60 Hz)
- `max_substeps`: Maximum sub-steps per frame if the frame took longer than expected
- `lock_2d`: "xz", "xy", or null for full 3D


## Acceptance Tests

### T-PHYS-01: Gravity — Free Fall
Spawn a dynamic body at position (0, 100, 0) with mass=1.0, gravity_factor=1.0. No colliders below. Step the simulation for 1 second (60 steps at 16.667ms).
- Expected: After 1 second, the body's Y position is approximately 100 - 0.5 * 9.81 * 1.0^2 = 95.095
- Expected: Tolerance: ± 0.5 (accounting for discrete timestep)
- Expected: X and Z positions remain 0

### T-PHYS-02: Gravity Factor
Spawn two dynamic bodies at (0, 100, 0). Body A has gravity_factor=1.0, Body B has gravity_factor=0.5. Step for 60 frames.
- Expected: Body A falls approximately twice as far as Body B
- Expected: Body B's Y displacement is approximately half of Body A's

### T-PHYS-03: Zero Gravity
Set world gravity to (0, 0, 0). Spawn a dynamic body at (0, 10, 0). Step for 60 frames.
- Expected: Body remains at (0, 10, 0) — no movement

### T-PHYS-04: Static Body Does Not Move
Spawn a static body at (5, 5, 5). Apply a force of (1000, 1000, 1000). Step for 60 frames.
- Expected: Body remains at (5, 5, 5) — forces have no effect on static bodies

### T-PHYS-05: Kinematic Body Push from ECS
Spawn a kinematic body at (0, 0, 0). Set its TransformComponent position to (10, 0, 0). Run the physics sync cycle.
- Expected: The rapier body's position is (10, 0, 0) after the push phase
- Expected: The body did not move due to physics forces — only the ECS push

### T-PHYS-06: Dynamic Body Pull to ECS
Spawn a dynamic body at (0, 10, 0) above a static floor at (0, 0, 0). Let it fall for 30 frames.
- Expected: The entity's TransformComponent Y position decreases each frame
- Expected: The TransformComponent matches the rapier body's position (within floating-point tolerance)

### T-PHYS-07: Collision Detection — Two Dynamic Bodies
Spawn body A at (0, 0, 0) and body B at (0.5, 0, 0), both with box colliders of size (1, 1, 1). They overlap.
- Expected: A CollisionEvent is emitted within the first few frames
- Expected: The event contains both entity IDs
- Expected: The collision normal is approximately along the X axis

### T-PHYS-08: Collision Layers — Matching Mask
Body A: layer=1, mask=2. Body B: layer=2, mask=1. Place them overlapping.
- Expected: CollisionEvent is emitted (layers match masks)

### T-PHYS-09: Collision Layers — Non-Matching Mask
Body A: layer=1, mask=4. Body B: layer=2, mask=4. Place them overlapping.
- Expected: No CollisionEvent is emitted (A's mask=4 does not match B's layer=2)
- Expected: Bodies pass through each other

### T-PHYS-10: Trigger Volume — Enter Event
Spawn a trigger volume at (0, 0, 0) with size (2, 2, 2). Spawn a dynamic body at (0, 5, 0) above it. Let it fall into the trigger.
- Expected: A TriggerEvent with type "enter" is emitted when the body overlaps the trigger
- Expected: No contact forces are applied (the body passes through)

### T-PHYS-11: Trigger Volume — Exit Event
Continuing from T-PHYS-10, the body falls through the trigger and exits below.
- Expected: A TriggerEvent with type "exit" is emitted when the body leaves the trigger volume

### T-PHYS-12: Apply Impulse
Spawn a dynamic body at rest at (0, 0, 0). Apply impulse (0, 100, 0). Step for 1 frame.
- Expected: Body's velocity.y > 0 after the impulse
- Expected: Body's position.y > 0 after the first step

### T-PHYS-13: Set Linear Velocity
Spawn a dynamic body. Set velocity to (5, 0, 0). Step for 60 frames (1 second). Ignore gravity.
- Expected: Body moves approximately 5 units along X (5.0 * 1.0s = 5.0)
- Expected: Tolerance: ± 0.1

### T-PHYS-14: 2D Lock — XZ Plane
Enable XZ plane lock. Spawn a dynamic body at (0, 0, 0). Apply impulse (1, 1, 1). Step for 30 frames.
- Expected: Y position remains 0 (locked out of Y movement)
- Expected: X and Z positions change (movement allowed in XZ)

### T-PHYS-15: 2D Lock — XY Plane
Enable XY plane lock. Spawn a dynamic body. Apply impulse (1, 1, 1). Step for 30 frames.
- Expected: Z position remains 0 (locked out of Z movement)
- Expected: X and Y positions change

### T-PHYS-16: Body Registration on Component Attach
Spawn an entity with no physics components. Attach a RigidBodyComponent and ColliderComponent via scene commands. Drain commands. Run physics sync.
- Expected: A rapier body now exists for this entity
- Expected: The body type matches the RigidBodyComponent config

### T-PHYS-17: Body Removal on Component Detach
Detach the RigidBodyComponent from an entity that has one. Drain commands. Run physics sync.
- Expected: The rapier body is destroyed
- Expected: No further physics events reference this entity

### T-PHYS-18: NaN Resilience
Manually set a dynamic body's position to (NaN, 0, 0). Step the simulation.
- Expected: The physics system detects NaN
- Expected: An error is logged
- Expected: The body is reset to its last valid position
- Expected: The simulation continues without crashing

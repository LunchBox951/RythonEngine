# Entity Component System

RythonEngine uses a custom ECS designed around the task-driven architecture. Unlike traditional ECS frameworks where systems run in a fixed tick loop, RythonEngine's systems submit their work as tasks to the scheduler. The ECS provides the data model (entities, components, hierarchy) and the coordination layer (event bus, command queues), but execution timing is entirely controlled by the scheduler.


## Scene

The Scene is the central registry. It holds all entities, their components, the parent-child hierarchy, and the event bus. There is one Scene per game world.

The Scene is shared across threads via a read-write lock. Most access is read-only (querying entities and components). Writes happen at defined points: command draining, entity spawn/despawn, component attach/detach.

```python
import rython

# Game scripts interact with the scene through the rython module
scene = rython.scene

# Spawn an entity with components
entity = scene.spawn(
    transform=rython.Transform(x=0, y=5, z=0),
    mesh=rython.MeshComponent(mesh_id="player_mesh", texture_id="player_tex"),
    tag=rython.TagComponent(tags=["player", "damageable"]),
)

# Query entities
for entity in scene.query("Transform", "MeshComponent"):
    pos = entity.get("Transform")
    # ...
```


## Entities

An entity is a lightweight handle — just a numeric ID. Entities have no behavior of their own. They are containers for components.

Entity IDs are generated from a monotonic counter. Despawned entity IDs are never reused within the same session to avoid dangling reference bugs.

```python
# Spawn returns an entity handle
player = scene.spawn(
    transform=rython.Transform(x=0, y=0, z=0),
)

# Despawn removes the entity and all its components
scene.despawn(player)
```


## Components

Components are pure data. They carry no behavior — just fields. In Rust, they are plain structs. In Python, they appear as objects with readable/writable properties.

The engine defines these built-in component types:

### TransformComponent
Local-space position, rotation, and scale. World-space transforms are computed by the TransformSystem from the parent hierarchy.

Fields: `x, y, z` (position), `rot_x, rot_y, rot_z` (rotation in degrees), `scale` (uniform scale factor)

```python
entity.transform.x = 10.0
entity.transform.rot_y = 90.0
entity.transform.scale = 2.0
```

### MeshComponent
References a 3D mesh and texture for rendering. Controls visibility.

Fields: `mesh_id, texture_id, yaw_offset, shininess, visible`

### BillboardComponent
A camera-facing 2D sprite in 3D space. Used for particles, distant objects, UI elements in the world.

Fields: `asset_id, width, height, uv_rect, alpha`

### ScriptComponent
Binds a Python script class to an entity. Contains a list of (event_type, handler_method) pairs. The ScriptSystem wires these handlers to the event bus when the entity spawns.

```python
class PlayerScript:
    def on_collision(self, other, normal):
        if other.has_tag("enemy"):
            self.entity.despawn()

    def on_input_action(self, action, value):
        if action == "move_forward":
            self.entity.transform.z += 5.0 * value

# Attach script to entity
scene.attach_script(player, PlayerScript)
```

### TagComponent
String tags for entity classification and querying.

Fields: `tags` (list of strings)

```python
entity.add_tag("enemy")
entity.has_tag("damageable")  # True/False
```

### RigidBodyComponent
Physics body configuration. See [physics.md](physics.md).

Fields: `body_type` (static/dynamic/kinematic), `mass, gravity_factor, collision_layer, collision_mask`

### ColliderComponent
Collision shape attached to a physics body. See [physics.md](physics.md).

Fields: `shape` (box/sphere/capsule/mesh), `size, is_trigger`


## Component Storage

Components are stored in typed maps: one map per component type, keyed by entity ID. This gives cache-friendly iteration when a system processes all components of a single type.

Each typed store has its own read-write lock, allowing systems that operate on disjoint component sets to run in parallel without contention. For example, the TransformSystem (which reads/writes TransformComponents) does not block the RenderSystem (which reads MeshComponents and BillboardComponents) as long as they are in separate scheduler phases.


## Entity Hierarchy

Entities can have parent-child relationships. A child's world transform is computed relative to its parent.

```python
# Set parent
scene.set_parent(child_entity, parent_entity)

# Remove parent (child becomes root-level)
scene.clear_parent(child_entity)

# Get children
children = scene.get_children(parent_entity)
```

The hierarchy is stored as two maps: `parent_map` (child -> parent) and `children_map` (parent -> list of children). The TransformSystem walks the hierarchy each frame to compute world transforms.

A cycle guard caps parent-chain depth at 64 to prevent infinite loops from accidental circular parenting.


## Command Queues

All mutations to the Scene go through command queues. Scripts and systems do not modify entities directly — they submit commands that are drained at a deterministic point in the frame.

Command types:
- **SpawnEntityCmd**: Create a new entity with initial components
- **DespawnEntityCmd**: Remove an entity and all its components
- **AttachComponentCmd**: Add a component to an existing entity
- **DetachComponentCmd**: Remove a component from an entity
- **SetParentCmd**: Set an entity's parent
- **ClearParentCmd**: Remove an entity's parent

Commands are queued in a thread-safe structure. The Scene drains and applies all pending commands once per frame, during the GAME_UPDATE priority phase. This ensures all observers see a consistent state within a single frame.

```python
# These don't take effect immediately — they are queued
scene.spawn(transform=rython.Transform(x=0, y=0, z=0))
scene.despawn(entity)
scene.attach(entity, rython.MeshComponent(mesh_id="sword"))
```


## Event Bus

The Scene owns an event bus for decoupled communication. Systems and scripts subscribe to event types and receive callbacks when events are emitted.

Events are plain data objects — they carry information about what happened, not instructions about what to do.

Built-in event types:
- **EntitySpawnedEvent**: Fired after an entity is spawned. Contains the entity ID.
- **EntityDespawnedEvent**: Fired before an entity is despawned. Contains the entity ID.
- **CollisionEvent**: Two physics bodies collided. Contains both entity IDs and collision normal.
- **TriggerEvent**: An entity entered or exited a trigger volume. Contains both entities and enter/exit flag.

```python
# Subscribe to events in a script
class EnemyScript:
    def on_collision(self, other, normal):
        rython.audio.play("hit_sound", category="sfx")

    def on_trigger_enter(self, other):
        if other.has_tag("player"):
            self.entity.get("MeshComponent").visible = True
```

Custom game events can also be defined and emitted from Python:

```python
# Define a custom event
scene.emit("PlayerDied", entity_id=player.id, cause="fall_damage")

# Subscribe to it
scene.subscribe("PlayerDied", self.on_player_died)
```

Event dispatch happens during the sequential phase of the scheduler. When an event is emitted, all subscriber callbacks are collected and submitted as sequential tasks. This ensures event handlers don't run concurrently with the code that emitted the event.


## Systems

Systems are the behavioral layer. They read and write components to implement engine functionality. Each system runs as a task submitted to the scheduler — they do not have their own update loops.

### TransformSystem
Runs at GAME_EARLY priority. Walks the entity hierarchy and computes world-space transforms for every entity with a TransformComponent. Results are cached in a separate world-transform lookup (not stored on the TransformComponent itself, which holds local-space data only).

Parent chains are walked from leaf to root. A visited set prevents redundant recomputation when multiple children share the same ancestor chain.

### RenderSystem
Runs at RENDER_ENQUEUE priority. Iterates all entities with MeshComponent or BillboardComponent. For each visible entity, builds a DrawCommand using the entity's world transform and component data. DrawCommands are submitted to the Renderer's command buffer.

### ScriptSystem
Event-driven, not tick-driven. Subscribes to EntitySpawnedEvent and EntityDespawnedEvent. When an entity with a ScriptComponent spawns, the ScriptSystem wires its event handlers to the event bus. When the entity despawns, handlers are unwired.

The ScriptSystem does not run per-frame. It only activates when entity lifecycle or game events fire.

See [scripting.md](scripting.md) for the full scripting architecture.


## Save/Load

The Scene can serialize to and deserialize from JSON. Serializable component types: TransformComponent, MeshComponent, BillboardComponent, TagComponent, RigidBodyComponent, ColliderComponent. ScriptComponent is runtime-only and is not serialized — scripts are re-attached on load based on entity tags or a script mapping table.

```python
# Save current scene
scene.save("saves/checkpoint.json")

# Load a scene (replaces current)
scene.load("saves/checkpoint.json")
```


## Acceptance Tests

### T-ECS-01: Entity Spawn and ID Uniqueness
Spawn 10,000 entities in a single frame (via command queue).
- Expected: All 10,000 entity IDs are unique
- Expected: Entity IDs are monotonically increasing
- Expected: All entities are queryable after the command queue drains

### T-ECS-02: Entity Despawn Removes All Components
Spawn an entity with Transform, Mesh, Tag, and RigidBody components. Despawn the entity.
- Expected: After command drain, the entity ID returns no results for any component query
- Expected: Component storage for each type no longer contains an entry for that entity ID

### T-ECS-03: Entity ID Non-Reuse
Spawn entity A (ID=1). Despawn entity A. Spawn entity B.
- Expected: Entity B's ID is NOT 1. It is a new unique ID (e.g., 2)
- Expected: Querying ID=1 returns nothing (no stale reference bugs)

### T-ECS-04: Component Attach and Detach
Spawn an entity with no components. Attach a TransformComponent. Verify it is present. Detach it. Verify it is absent.
- Expected: After attach + drain, querying the entity for TransformComponent returns the component
- Expected: After detach + drain, querying the entity for TransformComponent returns None

### T-ECS-05: Component Data Integrity
Spawn an entity with Transform(x=1.5, y=2.5, z=3.5, rot_y=90.0, scale=2.0). Read the component back.
- Expected: x == 1.5, y == 2.5, z == 3.5 (exact floating-point equality)
- Expected: rot_y == 90.0, scale == 2.0

### T-ECS-06: Component Mutation
Spawn an entity with Transform(x=0). Modify x to 10.0. Read it back.
- Expected: x == 10.0
- Expected: Other fields remain at their default values (unchanged)

### T-ECS-07: Entity Hierarchy — Parent-Child
Spawn parent P and child C. Set C's parent to P.
- Expected: `get_parent(C)` returns P
- Expected: `get_children(P)` contains C
- Expected: `get_children(C)` is empty

### T-ECS-08: Entity Hierarchy — Multi-Level
Create chain: A -> B -> C -> D (A is root, D is deepest child). Query parent chain from D.
- Expected: D's parent is C, C's parent is B, B's parent is A, A has no parent
- Expected: A's children = [B], B's children = [C], C's children = [D]

### T-ECS-09: Entity Hierarchy — Depth Guard
Create a parent chain of depth 65 (exceeding the 64-depth cap). Attach a TransformComponent to the deepest entity.
- Expected: TransformSystem logs a warning about depth exceeded
- Expected: The engine does not hang or crash
- Expected: World transform computation terminates for the capped entity

### T-ECS-10: Entity Hierarchy — Clear Parent
Set C's parent to P. Then clear C's parent.
- Expected: `get_parent(C)` returns None
- Expected: `get_children(P)` no longer contains C

### T-ECS-11: Entity Hierarchy — Despawn Parent Orphans Children
Spawn parent P with children C1 and C2. Despawn P.
- Expected: C1 and C2 still exist (they are not cascade-despawned)
- Expected: C1 and C2 become root-level entities (no parent)

### T-ECS-12: Command Queue Determinism
Submit 100 SpawnEntityCmds in a specific order. Drain the command queue.
- Expected: Entities are created in the exact order commands were submitted
- Expected: Entity IDs reflect submission order (ID N was the Nth command)

### T-ECS-13: Commands Are Deferred
Submit a SpawnEntityCmd. Immediately query the scene for the entity (before drain).
- Expected: The entity does NOT exist yet (command is deferred)
- Expected: After drain, the entity exists

### T-ECS-14: Event Bus — Subscribe and Emit
Subscribe a handler to EntitySpawnedEvent. Spawn an entity (triggering the event after drain).
- Expected: The handler is called exactly once
- Expected: The handler receives the correct entity ID

### T-ECS-15: Event Bus — Multiple Subscribers
Subscribe 3 handlers to the same event type. Emit one event.
- Expected: All 3 handlers are called
- Expected: Each handler receives the same event data

### T-ECS-16: Event Bus — Unsubscribe
Subscribe a handler. Unsubscribe it. Emit the event.
- Expected: The handler is NOT called

### T-ECS-17: Event Bus — Custom Events
Emit a custom event "GameOver" with data `{score: 1500}`. A subscribed handler reads the score.
- Expected: The handler receives score == 1500

### T-ECS-18: TransformSystem — World Transform Computation
Create hierarchy: Parent at (10, 0, 0), Child at local (5, 0, 0). Run TransformSystem.
- Expected: Child's world position is (15, 0, 0)
- Expected: Parent's world position is (10, 0, 0)

### T-ECS-19: TransformSystem — Rotation Propagation
Create hierarchy: Parent rotated 90 degrees around Y. Child at local (1, 0, 0). Run TransformSystem.
- Expected: Child's world position is approximately (0, 0, -1) (rotated by parent)

### T-ECS-20: TransformSystem — Scale Propagation
Parent with scale=2.0. Child at local (1, 0, 0) with scale=1.0. Run TransformSystem.
- Expected: Child's world position is (2, 0, 0) (position scaled by parent)
- Expected: Child's effective world scale is 2.0 (inherited from parent)

### T-ECS-21: Scene Save/Load Round-Trip
Spawn 5 entities with Transform, Mesh, and Tag components. Save to JSON. Clear the scene. Load from JSON.
- Expected: 5 entities exist after load
- Expected: All component values match the originals (floating-point exact)
- Expected: Entity hierarchy is preserved

### T-ECS-22: Query Performance
Spawn 100,000 entities with TransformComponent. Query all entities with TransformComponent.
- Expected: Query completes in under 10ms
- Expected: Query returns exactly 100,000 results

### T-ECS-23: RenderSystem — Visible Entity Produces DrawCommand
Spawn an entity with MeshComponent(visible=True) and TransformComponent. Run RenderSystem.
- Expected: Exactly 1 DrawMeshCmd is submitted to the renderer's command buffer
- Expected: The DrawMeshCmd's transform matches the entity's world transform

### T-ECS-24: RenderSystem — Invisible Entity Produces No DrawCommand
Spawn an entity with MeshComponent(visible=False). Run RenderSystem.
- Expected: Zero DrawCommands are submitted for this entity

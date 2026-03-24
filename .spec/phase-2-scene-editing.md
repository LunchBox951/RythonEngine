# Phase 2: Scene Editing

**Goal:** A functional scene editor that can create, modify, and save entities with full
undo/redo and a persistent project format.

**Result:** Users can create a project, add/remove/edit entities and their components,
organize entity hierarchies, save/load scenes, and undo/redo all edits.

**Depends on:** Phase 1 (viewport and crate scaffolding)

---

## 1. Engine Change: `EntityId::ensure_counter_past()`

**File:** `crates/rython-ecs/src/entity.rs`

After `Scene::load_json()`, newly spawned entities must not collide with loaded IDs. Add:

```rust
/// Advance the global counter to be at least `val + 1`.
/// Called after loading a scene to prevent ID collisions.
pub fn ensure_counter_past(val: u64) {
    NEXT_ID.fetch_max(val + 1, Ordering::SeqCst);
}
```

The editor calls this after every `load_json()`, passing the maximum entity ID found in
the loaded data.

---

## 2. Project Format

### `src/project/format.rs` — `ProjectConfig`

```rust
#[derive(Serialize, Deserialize, Default)]
pub struct ProjectConfig {
    pub name: String,
    pub version: String,
    pub default_scene: Option<String>,   // filename without extension
    pub entry_point: Option<String>,     // Python entry point module name
    pub engine_config: EngineConfig,     // from rython-core
}
```

### `src/project/io.rs` — Load / Save

**Project structure on disk:**

```
<root>/
  project.json
  scenes/<name>.json
  ui/<name>.json
  scripts/<name>.py
  assets/{meshes,textures,sounds}/
```

**Operations:**

| Function | Description |
|---|---|
| `create_project(root: &Path, name: &str)` | Create directory structure + default `project.json` |
| `open_project(root: &Path) -> ProjectConfig` | Read and parse `project.json` |
| `save_project(root: &Path, config: &ProjectConfig)` | Write `project.json` |
| `list_scenes(root: &Path) -> Vec<String>` | Scan `scenes/` directory for `.json` files |
| `save_scene(root: &Path, name: &str, scene: &Scene)` | Call `scene.save_json()` and write to `scenes/<name>.json` |
| `load_scene(root: &Path, name: &str, scene: &Scene)` | Read file, call `scene.load_json()`, then `EntityId::ensure_counter_past()` |

### `src/state/project.rs` — `ProjectState`

```rust
pub struct ProjectState {
    pub root_dir: Option<PathBuf>,
    pub config: ProjectConfig,
    pub open_scene_name: Option<String>,
    pub dirty: bool,
}
```

The `dirty` flag is set by any scene mutation and cleared on save. The editor title bar
shows `*` when dirty.

---

## 3. Scene Hierarchy Panel

### `src/panels/scene_hierarchy.rs`

Displays all entities as a tree based on the `Hierarchy` parent-child relationships.

**Display:**
- Root entities (no parent) are top-level tree nodes
- Children are nested under their parent
- Entity label: first tag from `TagComponent` if present, otherwise `"Entity {id}"`
- Selected entity is highlighted

**Interactions:**

| Action | Behavior |
|---|---|
| Click entity | Select it (update `SelectionState`) |
| Right-click entity | Context menu: Rename, Duplicate, Delete, Add Child |
| Right-click empty area | Context menu: Add Entity |
| Drag entity onto another | Reparent (via `ReparentEntity` command) |
| Drag entity to empty area | Clear parent (make root) |

**"Add Entity" flow:**
1. Push a `SpawnEntity` editor command
2. The command creates an entity with a default `TransformComponent` at origin
3. Optionally adds a `TagComponent` with `["New Entity"]`
4. Select the new entity

**"Delete" flow:**
1. Push a `DespawnEntity` editor command (stores a full snapshot of the entity for undo)
2. Also recursively captures and despawns all children

---

## 4. Selection State

### `src/state/selection.rs`

```rust
pub enum Selection {
    None,
    Entity(EntityId),
    Widget(WidgetId),
    Asset(PathBuf),
}

pub struct SelectionState {
    pub current: Selection,
}
```

Selection changes when:
- An entity is clicked in the hierarchy panel
- An entity is picked in the viewport (ray-cast — see below)
- A widget is clicked in the UI editor (Phase 4)
- An asset is clicked in the asset browser (Phase 3)

---

## 5. Component Inspector

### `src/panels/component_inspector.rs`

When `Selection::Entity(id)` is active, the inspector shows all components attached to
that entity with editable fields.

**Component discovery:** Call `scene.components.snapshot_entity(entity_id)` to get a list
of `(type_name, serde_json::Value)` pairs.

**Per-component editors:**

#### TransformComponent
- Position: three `DragValue` fields (x, y, z)
- Rotation: three `DragValue` fields in degrees (convert to/from radians)
- Scale: three `DragValue` fields (scale_x, scale_y, scale_z) + a "uniform" checkbox that
  locks all three to the same value

#### MeshComponent
- `mesh_id`: text input (or dropdown of known meshes)
- `texture_id`: text input (or dropdown of known textures)
- `visible`: checkbox
- `shininess`: slider (0.0 to 128.0)
- `yaw_offset`: drag value

#### TagComponent
- List of tag strings with add/remove buttons
- Inline text input for new tags

#### RigidBodyComponent
- `body_type`: dropdown (`"dynamic"`, `"static"`, `"kinematic"`)
- `mass`: drag value
- `gravity_factor`: drag value

#### ColliderComponent
- `shape`: dropdown (`"box"`, `"sphere"`, `"capsule"`)
- `size`: three drag values (x, y, z)
- `is_trigger`: checkbox

#### BillboardComponent
- `asset_id`: text input
- `width`, `height`: drag values
- `uv_rect`: four drag values
- `alpha`: slider (0.0 to 1.0)

**"Add Component" button:** A dropdown listing all component types not yet present on the
entity. Selecting one pushes an `AttachComponent` editor command with default values.

**"Remove Component" button:** Per-component header has a small `X` button that pushes a
`DetachComponent` editor command (stores the component JSON for undo).

**Value change flow:**
1. User modifies a field (e.g., drags the X position slider)
2. On drag start: snapshot the component JSON (old value)
3. On drag end: snapshot again (new value)
4. Push a `ModifyComponent` editor command with both snapshots
5. During drag (before release): apply the change live to the scene for immediate viewport
   feedback, but don't push the command until release

---

## 6. Undo/Redo System

### `src/state/undo.rs`

```rust
pub trait EditorCommand: Send + Sync {
    fn execute(&self, scene: &Scene);
    fn undo(&self, scene: &Scene);
    fn description(&self) -> &str;
}

pub struct UndoStack {
    history: Vec<Box<dyn EditorCommand>>,
    position: usize,     // index past the last executed command
    max_history: usize,  // cap (e.g., 200)
}
```

**Operations:**

| Method | Behavior |
|---|---|
| `push(cmd)` | Execute the command, append to history, truncate any redo tail |
| `undo()` | Decrement position, call `undo()` on the command at that index |
| `redo()` | Call `execute()` on the command at current position, increment |
| `can_undo()` / `can_redo()` | Check bounds |

### Editor Commands

#### `SpawnEntity`
- **execute:** Create entity via `scene.spawn_immediate()` with stored components
- **undo:** `scene.despawn_immediate(entity_id)`
- Stores: `EntityId`, component list, parent

#### `DespawnEntity`
- **execute:** Snapshot all components + children, then `scene.despawn_immediate()`
- **undo:** Recreate entity with stored snapshot via `spawn_immediate()`, restore hierarchy
- Stores: `EntityId`, full component snapshot, children snapshots, parent

#### `ModifyComponent`
- **execute:** Deserialize `new_value` JSON and `scene.components.insert()` it
- **undo:** Deserialize `old_value` JSON and `scene.components.insert()` it
- Stores: `EntityId`, component type name, old JSON, new JSON
- Uses `Scene::load_component()` pattern for deserialization

#### `ReparentEntity`
- **execute:** `scene.hierarchy.set_parent(child, new_parent)` or `clear_parent()`
- **undo:** Restore old parent
- Stores: `EntityId`, old parent, new parent

#### `AttachComponent`
- **execute:** Insert component with default values
- **undo:** Remove the component by type
- Stores: `EntityId`, type name, default JSON

#### `DetachComponent`
- **execute:** Remove component, store its JSON
- **undo:** Reattach from stored JSON
- Stores: `EntityId`, type name, component JSON

---

## 7. Ray-Cast Picking in Viewport

### `src/viewport/picking.rs`

When the user clicks in the viewport (and no gizmo is active):

1. Convert the click position from viewport-local pixels to normalized device coordinates
2. Unproject through `camera.view_projection().inverse()` to get a world-space ray
   (origin + direction)
3. For each entity with a `TransformComponent` + `MeshComponent`:
   - Compute an axis-aligned bounding box from the world transform (for "cube" mesh: unit
     box scaled by transform)
   - Ray-AABB intersection test
4. Select the closest hit entity
5. If no hit, clear selection

This is sufficient for MVP. Phase 3 may upgrade to color-buffer picking for precision.

---

## 8. File Menu

Wire up the egui top menu bar:

| Menu Item | Action |
|---|---|
| File > New Project | `rfd::FileDialog` to choose directory, then `create_project()` |
| File > Open Project | `rfd::FileDialog` to choose `project.json`, then `open_project()` |
| File > Save Scene (Ctrl+S) | `save_scene()` with current scene name |
| File > Save Scene As | `rfd::FileDialog` for filename, then `save_scene()` |
| Edit > Undo (Ctrl+Z) | `undo_stack.undo()` |
| Edit > Redo (Ctrl+Shift+Z) | `undo_stack.redo()` |

---

## 9. Verification

1. File > New Project creates the directory structure with a valid `project.json`
2. Entities can be added via the hierarchy panel context menu
3. Selecting an entity in the hierarchy highlights it and shows its components in the inspector
4. Editing a component field (e.g., position X) updates the viewport in real-time
5. Ctrl+Z undoes the last edit, Ctrl+Shift+Z redoes it
6. File > Save Scene writes a valid JSON file that `Scene::load_json()` can parse
7. File > Open Project + selecting a scene loads and displays it
8. Clicking in the viewport selects the nearest entity (ray-cast)
9. Deleting an entity and undoing restores it with all components and children

---

## Files Created / Modified

| Action | File |
|---|---|
| **Modify** | `crates/rython-ecs/src/entity.rs` (add `ensure_counter_past`) |
| **Create** | `crates/rython-editor/src/state/mod.rs` |
| **Create** | `crates/rython-editor/src/state/project.rs` |
| **Create** | `crates/rython-editor/src/state/selection.rs` |
| **Create** | `crates/rython-editor/src/state/undo.rs` |
| **Create** | `crates/rython-editor/src/project/mod.rs` |
| **Create** | `crates/rython-editor/src/project/format.rs` |
| **Create** | `crates/rython-editor/src/project/io.rs` |
| **Create** | `crates/rython-editor/src/panels/scene_hierarchy.rs` |
| **Create** | `crates/rython-editor/src/panels/component_inspector.rs` |
| **Create** | `crates/rython-editor/src/viewport/picking.rs` |
| **Modify** | `crates/rython-editor/src/app.rs` (integrate all panels + state) |

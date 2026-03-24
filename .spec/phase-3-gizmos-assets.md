# Phase 3: Gizmos + Asset Browser

**Goal:** Visual manipulation of entities via viewport gizmos and a project asset browser
with drag-and-drop.

**Result:** Users can translate, rotate, and scale entities by dragging gizmo handles in
the viewport, and browse/import/assign assets through a dedicated panel.

**Depends on:** Phase 2 (scene editing, undo/redo, project I/O)

---

## 1. Transform Gizmos

### `src/viewport/gizmo.rs`

Gizmos are drawn as overlays in the viewport and provide interactive manipulation of the
selected entity's `TransformComponent`.

### Gizmo Modes

```rust
pub enum GizmoMode {
    Translate,
    Rotate,
    Scale,
}

pub enum GizmoSpace {
    World,
    Local,
}
```

Keyboard shortcuts to switch modes:
- `W` — Translate
- `E` — Rotate
- `R` — Scale
- `X` — Toggle World / Local space

### `src/state/viewport.rs` — `ViewportState`

```rust
pub struct ViewportState {
    pub camera: Camera,
    pub camera_controller: CameraController,
    pub gizmo_mode: GizmoMode,
    pub gizmo_space: GizmoSpace,
    pub show_grid: bool,
    pub show_wireframe: bool,
    pub active_drag: Option<GizmoDrag>,
}
```

### Visual Representation

#### Translate Gizmo
- Three colored arrows along X (red), Y (green), Z (blue) axes
- Arrows are rendered as thin cone-tipped lines emanating from the entity's world position
- Each arrow has a small hit-test region (cylinder + cone tip)
- Dragging an arrow constrains movement to that axis

#### Rotate Gizmo
- Three colored circles (tori) around X (red), Y (green), Z (blue) axes
- Each circle lies in the plane perpendicular to its axis
- Dragging along a circle rotates around that axis

#### Scale Gizmo
- Three colored lines with cube-shaped end caps along each axis
- Dragging scales along that axis
- Center cube scales uniformly

### Rendering Approach

**Option A (recommended for MVP): egui Painter overlay**

Project gizmo geometry (arrow endpoints, circle sample points) from world space to screen
space using the camera's view-projection matrix. Draw them using `egui::Painter` calls
(`line_segment`, `circle_filled`) on top of the viewport image.

Pros: No extra mesh creation, no additional wgpu render passes, works immediately.
Cons: No depth testing against scene geometry (gizmos always on top — acceptable for an
editor).

**Option B (future): Rendered gizmo meshes**

Create actual 3D meshes for arrows/tori/cubes and render them as `DrawMesh` commands with
a special shader that disables depth testing. More visually polished but more work.

### Interaction Flow

1. **Hit test:** On mouse press in viewport, project each gizmo handle to screen space.
   Check if the click is within a threshold distance of any handle.
2. **Drag start:** Record the initial entity transform, the active axis, and the initial
   mouse position.
3. **Drag update:** Each frame during drag:
   - For translate: compute a world-space offset along the constrained axis from the mouse
     delta, apply it to the entity's position live
   - For rotate: compute an angle delta from the mouse movement projected onto the rotation
     circle, apply it live
   - For scale: compute a scale factor from the mouse delta, apply it live
4. **Drag end:** Snapshot the final transform. Push a `ModifyComponent` command with the
   old and new `TransformComponent` JSON.
5. **Cancel (Escape):** Restore the initial transform, don't push a command.

### Axis Constraint Math

**Translate along axis `a` (world space):**
```
ray = unproject(mouse_pos)
// Project the ray onto the axis line through the entity position
t = dot(ray.origin - entity_pos, a) / dot(ray.direction, a)
new_pos = entity_pos + (t - t_initial) * a
```

**Rotate around axis `a`:**
```
// Project mouse movement onto the screen-space tangent of the rotation circle
angle_delta = atan2(screen_delta projected onto tangent)
```

**Scale along axis `a`:**
```
// Scale factor proportional to mouse drag distance along the screen-space axis projection
factor = 1.0 + (screen_delta dot screen_axis) / reference_length
new_scale_component = initial_scale_component * factor
```

---

## 2. Asset Browser Panel

### `src/panels/asset_browser.rs`

Displays the contents of the project's `assets/` directory in a browseable grid.

### Layout

```
┌─ Asset Browser ──────────────────────────────────────────┐
│ [meshes] [textures] [sounds] [fonts] [all]   [Import]   │
│                                                          │
│ ┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐            │
│ │ thumb  │ │ thumb  │ │ icon   │ │ icon   │            │
│ │        │ │        │ │  .wav  │ │  .glb  │            │
│ │ sky.png│ │ stone  │ │ bgm    │ │ ship   │            │
│ └────────┘ └────────┘ └────────┘ └────────┘            │
└──────────────────────────────────────────────────────────┘
```

### Features

| Feature | Description |
|---|---|
| **Category tabs** | Filter by asset type (meshes, textures, sounds, fonts, or all) |
| **Thumbnail grid** | Image assets show a thumbnail; other types show a type icon |
| **Search/filter** | Text input to filter by filename |
| **Import** | Button opens a native file dialog (`rfd`), copies selected files into the appropriate `assets/` subdirectory |
| **Selection** | Click to select an asset (updates `SelectionState::Asset`) |
| **Preview** | Double-click an image to show it in a popup; double-click a mesh to render it in a small preview |

### File Scanning

On project open and on explicit refresh:
1. Walk the `assets/` directory recursively
2. Categorize files by extension:
   - meshes: `.glb`, `.gltf`, `.obj`
   - textures: `.png`, `.jpg`, `.jpeg`, `.bmp`, `.tga`
   - sounds: `.wav`, `.ogg`, `.mp3`, `.flac`
   - fonts: `.ttf`, `.otf`
3. Store as a flat list of `AssetEntry { path, category, filename }`
4. For image files, load thumbnails asynchronously (downscaled to ~64x64)

### Drag-and-Drop to Inspector

When the user drags an asset from the browser over a compatible inspector field:

1. Start drag: store the asset path in a drag payload
2. Over inspector: highlight compatible fields (e.g., texture fields accept image assets,
   mesh fields accept mesh assets)
3. Drop: set the field value to the asset's ID (filename without extension) and push a
   `ModifyComponent` command

Implementation uses `egui::DragAndDrop` or manual `memory().is_being_dragged()`.

---

## 3. ResourceManager Integration

Wire up `rython-resources::ResourceManager` so that assets referenced in scene components
are actually loaded and renderable in the viewport:

1. On project open: create a `ResourceManager` pointing at the project's `assets/` dir
2. When the scene references a `mesh_id` or `texture_id`, call `resource_manager.load()`
3. Each frame: call `resource_manager.poll_completions()` to process decoded assets
4. On GPU upload: use `renderer.gpu.process_uploads()` to create GPU textures
5. Mesh data feeds into `renderer.mesh_cache` via `renderer.upload_mesh()`

This means the viewport shows textured meshes instead of just untextured cubes.

---

## 4. Asset Import Flow

When the user clicks "Import":

1. `rfd::FileDialog::new().add_filter(...)` opens a native file picker
2. The user selects one or more files
3. For each file:
   - Determine its category from extension
   - Copy it to the appropriate `assets/<category>/` subdirectory
   - If a file with the same name exists, prompt to overwrite or rename
4. Refresh the asset list
5. The imported assets are immediately available in the browser

---

## 5. Verification

1. Selecting an entity and pressing W/E/R switches between translate/rotate/scale gizmos
2. Dragging a gizmo axis moves/rotates/scales the entity along that axis only
3. Releasing the gizmo creates an undo-able command
4. Pressing Escape during a drag cancels the operation
5. The asset browser shows all files in the project's `assets/` directory
6. Image thumbnails load and display correctly
7. Clicking "Import" copies a file into the correct subdirectory
8. Dragging a texture from the asset browser onto a MeshComponent's `texture_id` field
   updates the component and the viewport shows the texture
9. Meshes loaded via ResourceManager render correctly in the viewport

---

## Files Created / Modified

| Action | File |
|---|---|
| **Create** | `crates/rython-editor/src/viewport/gizmo.rs` |
| **Create** | `crates/rython-editor/src/state/viewport.rs` |
| **Create** | `crates/rython-editor/src/panels/asset_browser.rs` |
| **Modify** | `crates/rython-editor/src/app.rs` (integrate gizmos, asset browser, ResourceManager) |
| **Modify** | `crates/rython-editor/src/panels/viewport_panel.rs` (gizmo rendering + interaction) |
| **Modify** | `crates/rython-editor/src/panels/component_inspector.rs` (drag-drop targets) |

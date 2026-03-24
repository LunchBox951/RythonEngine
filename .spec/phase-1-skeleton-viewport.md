# Phase 1: Skeleton + Scene Viewport

**Goal:** An eframe window with egui panels and a 3D scene viewport that renders a loaded
scene file with orbit/pan/zoom camera controls.

**Result:** The editor launches, loads a scene JSON, and displays it in a live 3D viewport.

---

## 1. Crate Setup

### Workspace registration

Add `"crates/rython-editor"` to the `members` list in the root `Cargo.toml`. Add new
workspace dependencies:

```toml
egui = "0.31"
egui-wgpu = "0.31"
egui-winit = "0.31"
rfd = "0.15"
```

### `crates/rython-editor/Cargo.toml`

```toml
[package]
name = "rython-editor"
version.workspace = true
edition.workspace = true

[[bin]]
name = "rython-editor"
path = "src/main.rs"

[dependencies]
rython-core.workspace = true
rython-ecs.workspace = true
rython-renderer.workspace = true
rython-resources.workspace = true
rython-ui.workspace = true

egui.workspace = true
egui-wgpu.workspace = true
egui-winit.workspace = true

winit.workspace = true
wgpu.workspace = true

serde.workspace = true
serde_json.workspace = true
glam.workspace = true
log.workspace = true
env_logger.workspace = true
parking_lot.workspace = true
rfd.workspace = true
```

---

## 2. Engine Change: `GpuContext::from_existing()`

**File:** `crates/rython-renderer/src/gpu.rs`

Add a new constructor that accepts externally-owned wgpu handles. This mirrors
`new_headless()` (line 88) but skips adapter/device creation:

```rust
pub fn from_existing(
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface_format: wgpu::TextureFormat,
    sample_count: u32,
) -> Result<Self, RendererError> {
    let info = adapter.get_info();
    log::info!("wgpu adapter (shared): {} ({:?})", info.name, info.backend);

    let (pipelines, bind_group_layouts) =
        Self::create_pipelines(&device, surface_format, sample_count)?;

    Ok(Self {
        instance,
        adapter,
        device,
        queue,
        pipelines,
        bind_group_layouts,
        surface_format,
        sample_count,
    })
}
```

This is the only change to existing engine code in Phase 1.

---

## 3. eframe Application Shell

### `src/main.rs`

Initialize `env_logger`, configure eframe options (wgpu backend, window size, title), and
launch `EditorApp`:

```rust
fn main() -> eframe::Result<()> {
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1600.0, 900.0])
            .with_title("RythonEditor"),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        "RythonEditor",
        options,
        Box::new(|cc| Ok(Box::new(EditorApp::new(cc)))),
    )
}
```

### `src/app.rs` — `EditorApp`

```rust
pub struct EditorApp {
    // Engine rendering (initialized in new() from eframe's wgpu state)
    renderer: Option<RendererState>,

    // Scene data
    scene: Arc<Scene>,

    // Viewport
    viewport_texture: Option<ViewportTexture>,
    viewport_camera: Camera,
    camera_controller: CameraController,

    // Placeholder panel state
    show_hierarchy: bool,
    show_inspector: bool,
}
```

**`EditorApp::new(cc: &CreationContext)`:**
1. Extract `wgpu::Device`, `Queue`, `Adapter` from `cc.wgpu_render_state`
2. Build `GpuContext::from_existing(...)` using those handles
3. Construct `RendererState::new(gpu, RendererConfig::default())`
4. Create an empty `Scene`
5. Initialize `Camera` at a sensible default position (e.g., `(0, 5, -10)` looking at origin)

**`EditorApp::update(ctx, frame)` — implements `eframe::App`:**
1. Draw top menu bar (placeholder: File, Edit, View)
2. Draw left panel (hierarchy placeholder)
3. Draw right panel (inspector placeholder)
4. Draw central panel — the viewport (see section 4)
5. Draw bottom panel (asset browser placeholder)

---

## 4. Offscreen Viewport Rendering

### `src/viewport/offscreen.rs` — `ViewportTexture`

Manages the offscreen render target that the scene is rendered into:

```rust
pub struct ViewportTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub egui_texture_id: egui::TextureId,
    pub width: u32,
    pub height: u32,
}
```

**Lifecycle:**
- Created (or recreated) when the viewport panel size changes
- The texture format is RGBA8UnormSrgb with usages `RENDER_ATTACHMENT | TEXTURE_BINDING`
- Registered with egui-wgpu's `Renderer` to get an `egui::TextureId`
- On resize: destroy old texture, create new, re-register

### `src/panels/viewport_panel.rs`

Each frame:

1. Allocate an `egui::CentralPanel` (or `egui::Panel`)
2. Get the available rect in physical pixels
3. If `viewport_texture` is `None` or size changed, recreate it
4. Run `TransformSystem::run(&scene)` to compute world transforms
5. Run `RenderSystem::run(&scene, &world_transforms)` to generate `DrawCommand`s
6. Extract `DrawMesh` commands from the draw command list
7. Call `renderer.ensure_depth_texture(width, height)` (existing method)
8. Clear the offscreen texture (via a render pass with a clear color)
9. Call `renderer.render_meshes(&draw_meshes, &camera, &viewport_texture.view)`
10. Display the texture: `ui.image(egui::ImageSource::Texture(SizedTexture::new(tex_id, size)))`

### Viewport clear pass

Before `render_meshes`, run a small clear pass on the offscreen texture:

```rust
let mut encoder = device.create_command_encoder(&Default::default());
{
    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &viewport_texture.view,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.15, g: 0.15, b: 0.15, a: 1.0 }),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        ..Default::default()
    });
}
queue.submit(std::iter::once(encoder.finish()));
```

---

## 5. Camera Controller

### `src/viewport/camera_controller.rs`

An orbit camera that responds to mouse input within the viewport panel:

| Input | Action |
|---|---|
| Middle mouse drag | Orbit: rotate around the look-at point |
| Alt + Left mouse drag | Orbit (alternative) |
| Shift + Middle mouse drag | Pan: translate camera perpendicular to view |
| Right mouse drag | Pan (alternative) |
| Scroll wheel | Zoom: move closer/farther from look-at target |

**State:**

```rust
pub struct CameraController {
    pub target: Vec3,      // Look-at point (orbit center)
    pub distance: f32,     // Distance from target
    pub yaw: f32,          // Horizontal angle (radians)
    pub pitch: f32,        // Vertical angle (radians, clamped to avoid flip)
}
```

**`update(response: &egui::Response, camera: &mut Camera)`:**
- Read `response.dragged_by()`, `response.hover_pos()`, `response.ctx.input().scroll_delta`
- Apply orbit/pan/zoom deltas
- Clamp pitch to `(-89deg, 89deg)` to avoid gimbal lock
- Recompute camera position from spherical coordinates: `target + spherical(distance, yaw, pitch)`
- Call `camera.set_position(pos)` and `camera.set_look_at(target)`

---

## 6. Grid Plane

Render a ground-plane grid as visual reference. Two approaches (choose the simpler one that
works with the existing renderer):

**Option A — DrawLine commands:** Generate a set of `DrawLine` commands forming a grid on the
XZ plane at Y=0. This uses the existing primitive pipeline.

**Option B — Dedicated grid mesh:** Generate a flat grid mesh (similar to `generate_cube()`)
and render it as a `DrawMesh`. This leverages the mesh pipeline which already handles 3D
transforms.

The grid should span a reasonable area (e.g., 20x20 units) with 1-unit spacing and a subtle
color (e.g., gray 0.3).

---

## 7. Verification

1. `cargo build -p rython-editor` compiles without errors
2. `cargo run --bin rython-editor` opens a window with:
   - A dark viewport area in the center
   - Placeholder panels on left, right, and bottom
   - A menu bar at the top
3. If a scene JSON file is loaded (hardcoded path for now), entities with `MeshComponent`
   are visible in the viewport
4. Mouse drag in the viewport orbits the camera around the scene
5. Scroll wheel zooms in and out
6. A grid is visible on the ground plane
7. The viewport resizes correctly when the window is resized

---

## Files Created / Modified

| Action | File |
|---|---|
| **Modify** | `Cargo.toml` (workspace members + deps) |
| **Modify** | `crates/rython-renderer/src/gpu.rs` (add `from_existing()`) |
| **Create** | `crates/rython-editor/Cargo.toml` |
| **Create** | `crates/rython-editor/src/main.rs` |
| **Create** | `crates/rython-editor/src/app.rs` |
| **Create** | `crates/rython-editor/src/viewport/mod.rs` |
| **Create** | `crates/rython-editor/src/viewport/offscreen.rs` |
| **Create** | `crates/rython-editor/src/viewport/camera_controller.rs` |
| **Create** | `crates/rython-editor/src/panels/mod.rs` |
| **Create** | `crates/rython-editor/src/panels/viewport_panel.rs` |

# RythonEditor — Visual Game Module Editor

A standalone GUI application for creating and editing RythonEngine game modules. The editor
provides visual scene editing, UI layout design, asset management, and script scaffolding —
outputting a standard project directory that the existing `rython` CLI can run directly.

---

## Goals

1. **Lower the barrier to game module creation.** Authors should be able to place entities,
   configure components, and preview scenes without writing boilerplate code.
2. **Accurate preview.** The editor uses the engine's actual renderer (`RendererState`,
   `TransformSystem`, `RenderSystem`) so what you see in the editor matches what the game
   renders at runtime.
3. **Clean project output.** The editor saves to a well-defined directory structure of JSON
   files and Python scripts that the `rython` CLI consumes without any editor-specific
   tooling at runtime.
4. **No runtime dependency on the editor.** The editor is a development tool only. Game
   modules it produces are self-contained and run on the existing engine binary.
5. **Iterative delivery.** The editor is built in five phases, each producing a usable
   increment. Phase 1 delivers a scene viewport; Phase 4 completes the full MVP.

---

## Features (MVP)

| Feature | Description |
|---|---|
| **Scene Editor** | 3D viewport with orbit camera, entity hierarchy tree, component inspector. Spawn, transform, and delete entities with live preview. |
| **UI Editor** | Visual widget tree editor for `rython-ui`. Place widgets, edit properties, preview layout and themes. |
| **Asset Browser** | Browse, import, and preview assets (meshes, textures, sounds, fonts). Drag assets into component fields. |
| **Script Scaffolding** | Generate Python script boilerplate (`init()`, script classes, event handlers). Associate scripts with entities. Editing is done in an external IDE. |

---

## Architecture

### Layer Placement

The editor sits at **Layer 5** — above the engine (Layer 4). It is a consumer of engine
crates, not a component of them.

```
Layer 0   rython-core
Layer 1   rython-scheduler    rython-modules
Layer 2   rython-ecs          rython-window      rython-input
          rython-renderer     rython-physics     rython-audio
          rython-resources
Layer 3   rython-ui           rython-scripting
Layer 4   rython-engine
Layer 5   rython-editor  (NEW)
Binary    rython-cli
```

### What the Editor Uses

| Engine Crate | Usage |
|---|---|
| `rython-core` | ID types, error types, configs, glam math |
| `rython-ecs` | `Scene`, `ComponentStorage`, `TransformSystem`, `RenderSystem`, all component types, `save_json()` / `load_json()` |
| `rython-renderer` | `GpuContext`, `RendererState`, `Camera`, draw commands — renders the scene viewport |
| `rython-resources` | `ResourceManager` — loads meshes, textures, fonts with LRU caching |
| `rython-ui` | `UIManager` — instantiated for UI layout preview in the UI editor panel |

### What the Editor Does NOT Use

| Excluded | Reason |
|---|---|
| `rython-engine` / `EngineBuilder` | Editor does not need the scheduler, module loader, or full engine lifecycle |
| `rython-scripting` / PyO3 | No Python runtime. Scripts are generated as text files |
| `rython-physics` | Not needed for scene preview (could be added later for play-in-editor) |
| `rython-audio` | Not needed for editing |
| `rython-scheduler` | Editor uses eframe's own event loop |

### GUI Framework: egui via eframe

The editor uses **eframe** (egui's built-in framework) which owns the winit event loop and
wgpu surface. The scene viewport is rendered to an offscreen wgpu texture and displayed as
an `egui::Image` inside a panel.

**Why egui:** Immediate-mode, fast iteration, strong Rust game editor ecosystem (used by
Bevy, Fyrox), native feel, good inspector/property-panel ergonomics.

**Why render-to-texture:** Cleanly isolates the engine's render pass (MSAA, depth buffer)
from egui's own rendering. `RendererState::render_meshes()` already accepts any
`wgpu::TextureView` as its color target.

### Sharing the wgpu Device

eframe owns the wgpu device. The editor extracts it from `CreationContext::wgpu_render_state`
and constructs a `GpuContext` wrapping those same handles via a new `GpuContext::from_existing()`
constructor. Since wgpu 24's `Device` and `Queue` are `Arc`-backed internally, this shares
the actual GPU resources without duplication.

---

## New Crate: `crates/rython-editor/`

### Dependencies

```toml
[dependencies]
# Engine crates
rython-core.workspace = true
rython-ecs.workspace = true
rython-renderer.workspace = true
rython-resources.workspace = true
rython-ui.workspace = true

# GUI
egui = "0.31"
egui-wgpu = "0.31"
egui-winit = "0.31"

# Shared with engine
winit.workspace = true
wgpu.workspace = true

# Serialization & utilities
serde.workspace = true
serde_json.workspace = true
glam.workspace = true
log.workspace = true
env_logger.workspace = true
parking_lot.workspace = true
rfd = "0.15"                   # native file dialogs
```

### Source Layout

```
src/
  main.rs                        # eframe entry point
  app.rs                         # EditorApp (implements eframe::App)

  state/
    mod.rs
    project.rs                   # ProjectState: open project, paths, dirty flag
    selection.rs                 # SelectionState: entity / widget / asset
    viewport.rs                  # ViewportState: camera, gizmo mode, grid toggle
    undo.rs                      # UndoStack: command pattern with JSON snapshots

  project/
    mod.rs
    format.rs                    # ProjectConfig serde struct, path helpers
    io.rs                        # Load/save project.json, scenes/*.json, ui/*.json
    scaffold.rs                  # Python script template generator

  panels/
    mod.rs
    scene_hierarchy.rs           # Entity tree (egui TreeNode, drag-to-reparent)
    component_inspector.rs       # Property editors for all 6 component types
    viewport_panel.rs            # 3D viewport with offscreen texture
    asset_browser.rs             # File grid with thumbnails / icons
    ui_editor.rs                 # Widget tree + visual preview
    script_panel.rs              # Script list, scaffolding, "Open in IDE"

  viewport/
    mod.rs
    camera_controller.rs         # Orbit / pan / zoom
    gizmo.rs                     # Translate / rotate / scale handles
    picking.rs                   # Ray-cast entity selection
    offscreen.rs                 # Offscreen wgpu texture management
```

---

## Project Output Format

The editor saves game modules as a directory:

```
my_game/
  project.json                   # project metadata and engine config
  scenes/
    main_menu.json               # entity/component data (Scene::save_json() format)
    level_1.json
  ui/
    hud.json                     # widget tree definitions
  scripts/
    main.py                      # generated Python boilerplate
    player.py
  assets/
    meshes/
    textures/
    sounds/
```

### project.json

```json
{
  "name": "My Game",
  "version": "0.1.0",
  "default_scene": "main_menu",
  "entry_point": "main",
  "engine_config": {
    "window": { "title": "My Game", "width": 1280, "height": 720 },
    "scheduler": { "target_fps": 60 }
  }
}
```

### Scene JSON

Uses the existing `Scene::save_json()` format — an array of entity records, each with an
ID, optional parent ID, and a list of typed component data objects. No new format invention.

---

## Required Changes to Existing Engine Crates

Only three small additions are needed across all phases:

| Change | Crate | Phase | Description |
|---|---|---|---|
| `GpuContext::from_existing()` | `rython-renderer` | 1 | Constructor accepting external wgpu handles from eframe |
| `EntityId::ensure_counter_past()` | `rython-ecs` | 2 | Advance the global ID counter past loaded IDs to prevent collisions |
| `UIManager::save_json()` / `load_json()` | `rython-ui` | 4 | Serialize widget trees for the UI editor |

---

## Undo/Redo

Command pattern using JSON snapshots. Components already implement `serialize_json()` and
`Scene::load_component()` handles type-name-based deserialization, so undo/redo stores
before/after JSON values per component and replays them.

Commands: `ModifyComponent`, `SpawnEntity`, `DespawnEntity`, `ReparentEntity`.

---

## Build Phases

| Phase | Deliverable | Key Spec |
|---|---|---|
| **1** | Window with 3D scene viewport and camera controls | `.spec/phase-1-skeleton-viewport.md` |
| **2** | Scene editing: hierarchy, inspector, project I/O, undo/redo | `.spec/phase-2-scene-editing.md` |
| **3** | Viewport gizmos and asset browser | `.spec/phase-3-gizmos-assets.md` |
| **4** | UI editor and script scaffolding (MVP complete) | `.spec/phase-4-ui-scripts.md` |
| **5** | Polish: docking, shortcuts, multi-select, play button | `.spec/phase-5-polish.md` |

---

## Risks

| Risk | Mitigation |
|---|---|
| egui-wgpu 0.31 / wgpu 24 version compatibility | Pin versions and verify compilation before feature work |
| Sharing eframe's wgpu device with RendererState | `from_existing()` wraps Arc-backed handles; validate in Phase 1 |
| Offscreen texture adds one copy per frame | Acceptable for editor; skip re-render when scene is clean |
| EntityId counter collisions after load | `ensure_counter_past()` called after every `load_json()` |
| UIManager lacks serialization | Add `save_json()` / `load_json()` in Phase 4 — contained change |
| No play-in-editor | Users run `rython` CLI separately; Phase 5 adds a "Play" button that spawns the CLI |

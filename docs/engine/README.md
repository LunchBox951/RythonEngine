# RythonEngine — Engine Internals

This document covers the Rust implementation of RythonEngine: how crates are layered, how modules
are registered and lifecycle-managed, how the task scheduler drives each frame, and how to add a
new engine module.

For the Python scripting API, see [`docs/game/README.md`](../game/README.md).
---

## Contents

1. [Architecture: Layered Crate System](#1-architecture-layered-crate-system)
2. [Module System](#2-module-system)
3. [Engine Construction: EngineBuilder](#3-engine-construction-enginebuilder)
4. [Engine Lifecycle](#4-engine-lifecycle)
5. [Scheduler: Task-Driven Execution](#5-scheduler-task-driven-execution)
6. [How to Create a New Engine Module](#6-how-to-create-a-new-engine-module)
7. [Crate Reference](#7-crate-reference)

---

## 1. Architecture: Layered Crate System

RythonEngine is a Cargo workspace of layered crates. **Lower layers never depend on higher
layers.** Each layer may only depend on crates in the same or lower layer.

```
Layer 0  rython-core
Layer 1  rython-scheduler   rython-modules
Layer 2  rython-ecs   rython-window   rython-input   rython-renderer
         rython-physics   rython-audio   rython-resources
Layer 3  rython-ui   rython-scripting
Layer 4  rython-engine
Binary   rython-cli   rython-editor
```

### Dependency DAG

```
rython-core (Layer 0)
  ├── rython-scheduler (Layer 1)
  ├── rython-modules   (Layer 1)
  ├── rython-ecs       (Layer 2)
  ├── rython-window    (Layer 2)
  ├── rython-input     (Layer 2) ──→ rython-window
  ├── rython-renderer  (Layer 2) ──→ rython-ecs, rython-window
  ├── rython-physics   (Layer 2) ──→ rython-ecs
  ├── rython-audio     (Layer 2)
  ├── rython-resources (Layer 2)
  ├── rython-ui        (Layer 3) ──→ rython-renderer, rython-input
  ├── rython-scripting (Layer 3) ──→ rython-core, rython-scheduler, rython-modules,
  │                                   rython-ecs, rython-renderer, rython-input,
  │                                   rython-window, rython-audio, rython-ui,
  │                                   rython-physics, rython-resources
  └── rython-engine    (Layer 4) ──→ ALL crates
```

`rython-cli` is a binary crate that depends on `rython-engine` and all Layer 2–3 modules it
registers.

`rython-editor` is a standalone binary crate that depends directly on `rython-core`, `rython-ecs`,
`rython-renderer`, `rython-resources`, and `rython-ui` without going through `rython-engine`. It
has no PyO3 dependency and does not embed a Python interpreter.

### Layer Descriptions

**Layer 0 — `rython-core`:** Foundation types used everywhere: `EntityId`, `OwnerId`, `Priority`,
`TaskId`, `GroupId`; the error hierarchy (`EngineError`, `TaskError`); configuration structs
(`EngineConfig`, `SchedulerConfig`, `WindowConfig`); the `SchedulerHandle` trait; and re-exports
of `glam` math types. No internal engine dependencies.

**Layer 1 — `rython-scheduler`:** `TaskScheduler` and `FramePacer`. All engine work flows through
`tick()`. Also exposes `RemoteSender` for cross-thread task submission.

**Layer 1 — `rython-modules`:** The `Module` trait and `ModuleLoader`. Manages dependency-ordered
loading, reference-counted shared dependencies, and exclusive (single-owner) module constraints.

**Layer 2 — domain crates:** Independent feature crates for ECS, windowing, input, rendering,
physics, audio, and resources. Each may depend on Layer 0–1 and on peer Layer 2 crates only where
the DAG above permits.

**Layer 3 — `rython-ui`:** Widget tree, layout, theme, and animation built on the renderer and
input crates.

**Layer 3 — `rython-scripting`:** PyO3 0.28 bridge that exposes the `rython` Python module.
Manages the `ACTIVE_SCENE` global singleton, flush of recurring Python callbacks, hot-reload in dev
builds, and draw-command draining. The bridge is split across a `bridge/` directory (14 files)
rather than a monolithic `bridge.rs`.

**Layer 4 — `rython-engine`:** `Engine` and `EngineBuilder` — assembles `TaskScheduler`,
`ModuleLoader`, and `Arc<Scene>` into a single runnable unit.

**Binary — `rython-cli`:** Parses CLI flags, builds and boots the engine, then drives either a
winit 0.30 `ApplicationHandler` windowed loop or a simple headless tick loop.

**Binary — `rython-editor`:** Visual editor built with egui 0.31 / eframe 0.31 and an embedded
wgpu viewport. Provides a docked panel layout (scene hierarchy, inspector, asset browser, viewport
with gizmos, UI layout editor), undo/redo, project open/save, and a play-in-editor session via a
child process. Reads and writes the same `game/scenes/*.json` ECS scene format used by the
runtime.

---

## 2. Module System

### The `Module` Trait

Every engine system implements `Module` (defined in `crates/rython-modules/src/module.rs`):

```rust
pub trait Module: Downcast + Send + Sync + 'static {
    /// Registry key — must be unique across all registered modules.
    fn name(&self) -> &str;

    /// Names of modules that must be loaded before this one.
    fn dependencies(&self) -> Vec<String> {
        Vec::new()
    }

    /// Called once when the module is loading.
    /// May submit initialization tasks to the scheduler.
    fn on_load(&mut self, scheduler: &dyn SchedulerHandle) -> Result<(), EngineError>;

    /// Called once when the module is unloading.
    /// May submit cleanup tasks; owned tasks are auto-cancelled by the scheduler.
    fn on_unload(&mut self, scheduler: &dyn SchedulerHandle) -> Result<(), EngineError>;

    /// When true, only one owner may hold this module at a time.
    fn is_exclusive(&self) -> bool {
        false
    }
}

impl_downcast!(Module);
```

`Module` is object-safe and downcasting is provided by `downcast-rs`, so callers can recover the
concrete type when needed.

### ModuleLoader Lifecycle

The `ModuleLoader` orchestrates loading and unloading in four steps:

1. **`register(module, owner)`** — Record the module and its dependency declarations. Does not
   load anything.
2. **`load_all()`** — Build a topological sort of the dependency graph; call `on_load()` in
   post-order (dependencies first). Each module transitions: _unregistered_ → **LOADING** →
   **LOADED**.
3. **`unload_all()`** — Call `on_unload()` in the exact reverse of load order (dependents first).
   Each module transitions: **LOADED** → **UNLOADING** → _removed_.
4. **Reference counting** — When multiple modules share a dependency, that dependency loads once
   and its refcount increments per dependent. It unloads only when its refcount reaches zero.

**Exclusive modules** (`is_exclusive() == true`) permit a single owner at a time. The owner may
transfer or relinquish ownership; non-owners cannot drive the module.

---

## 3. Engine Construction: EngineBuilder

`EngineBuilder` (in `crates/rython-engine/src/builder.rs`) provides a fluent API for assembling
an `Engine`:

```rust
let scene = Arc::new(Scene::new());

// AudioManager, UIManager, and PlayerController are created as Arc<Mutex<...>>
// and wired to the scripting bridge via set_active_audio / set_active_ui /
// set_active_input — they are NOT registered as EngineBuilder modules.
let engine = EngineBuilder::new()
    .with_config(config)                        // programmatic EngineConfig
    // .with_config_file("engine.json")         // OR load from JSON file
    .with_scene(Arc::clone(&scene))             // share scene with scripting module
    .add_module(Box::new(WindowModule::new(window_config)))
    .add_module(Box::new(ScriptingModule::new(scripting_config, Arc::clone(&scene))))
    .add_module(Box::new(PhysicsModule::new(Default::default())))
    .add_module(Box::new(ResourceManager::new(Default::default())))
    .build()?;
```

**Builder methods:**

| Method | Effect |
|---|---|
| `with_config(EngineConfig)` | Override config programmatically |
| `with_config_file(&str)` | Load config from JSON; falls back to defaults on error |
| `add_module(Box<dyn Module>)` | Register a module — this is the feature-flag mechanism |
| `with_scene(Arc<Scene>)` | Share a pre-created scene (required when scripting module needs the same `Arc<Scene>`) |
| `build() -> Result<Engine>` | Create `TaskScheduler` and `ModuleLoader`; does not boot |

**Key point:** Omitting an `add_module()` call is the mechanism for disabling a feature. There are
no separate feature flags — if a module is not registered, it never loads.

---

## 4. Engine Lifecycle

```
EngineBuilder::build()          // create scheduler + loader; modules not yet active
  │
  └── engine.boot()             // ModuleLoader::load_all()
        └── on_load() per module, in dependency post-order
              (LOADING → LOADED for each)

  ┌── [windowed] winit EventLoop::run_app()
  │     RedrawRequested  →  tick_and_render()
  │       ├── set_elapsed_secs(…)              // advance Python time
  │       ├── flush_python_bg_completions(py)  // fire on_complete callbacks for finished bg tasks
  │       ├── flush_python_seq_tasks(py)       // run queued sequential Python callables (main thread)
  │       ├── flush_python_par_tasks(py)       // run queued parallel tasks synchronously under GIL
  │       ├── flush_recurring_callbacks(py)    // run Python per-frame callbacks
  │       ├── flush_timers(py)                 // fire pending on_timer callbacks
  │       ├── flush_python_bg_tasks()          // dispatch background tasks to rayon (releases GIL)
  │       ├── scene.drain_commands()           // apply ECS mutations atomically
  │       ├── PhysicsWorld::sync_step(…)       // rapier3d step + sync to ECS transforms
  │       ├── PlayerController::tick(…)        // process raw events → input snapshot + action events
  │       ├── UIManager::on_mouse_*(…)         // route cursor/click events to widgets
  │       ├── TransformSystem::run(…)          // propagate world transforms
  │       ├── RenderSystem::run(…)             // collect DrawMesh commands from ECS
  │       ├── UIManager::compute_layout()      // finalize widget positions
  │       ├── drain_draw_commands()            // collect script + UI overlay commands
  │       ├── renderer.render_meshes(…)        // GPU mesh draw call
  │       ├── renderer.render_rects(…)         // solid-color UI rect overlays
  │       ├── renderer.render_text(…)          // text overlays (scripts + UI)
  │       ├── frame.present()
  │       └── engine.tick()                    // TaskScheduler pipeline (module tasks)
  │     CloseRequested → engine.shutdown() + exit
  │
  └── [headless] loop {
        set_elapsed_secs(…)
        flush_python_bg_completions(py) + flush_python_seq_tasks(py) + flush_python_par_tasks(py)
        flush_recurring_callbacks(py) + flush_timers(py)
        flush_python_bg_tasks()
        scene.drain_commands() + PhysicsWorld::sync_step(…)
        PlayerController::tick(…) + set_active_input(…) + emit input events
        engine.tick()
        if quit_requested { break }
      }

engine.shutdown()               // ModuleLoader::unload_all()
  └── on_unload() per module, in reverse load order
        (LOADED → UNLOADING → removed for each)
```

**`Engine` API summary:**

```rust
engine.boot()?;              // load all registered modules
engine.tick()?;              // one scheduler frame
engine.run_headless(n)?;     // run n headless ticks (no window)
engine.shutdown()?;          // unload all modules
engine.scene();              // &Arc<Scene>
engine.scheduler();          // &mut TaskScheduler
engine.remote_sender();      // RemoteSender for cross-thread submission
```

---

## 5. Scheduler: Task-Driven Execution

`TaskScheduler` (in `crates/rython-scheduler/src/scheduler.rs`) is the central frame driver. All
engine work — rendering, physics, input, scripting — flows through it. Modules do not tick
themselves; they submit tasks at declared priorities.

### `tick()` Phases

Each call to `scheduler.tick()` runs in this order:

```
1. Drain remote queue   — move cross-thread submissions (RemoteSender) into seq_queue
2. Sequential phase     — sort one-shot tasks by priority; run; then run recurring sequential tasks
3. Parallel phase       — dispatch one-shot parallel tasks via pool.install(); run recurring parallel tasks
4. Background phase     — poll completion channel; fire per-task callbacks; fire task-group callbacks
5. Frame pacing         — FramePacer: sleep + busy-spin to hit target FPS
```

Panics inside tasks are caught via `catch_unwind`; the scheduler continues rather than crashing the
process.

### Task Types

| Submission method | Runs on | Lifetime |
|---|---|---|
| `submit_sequential(f, priority, owner)` | Main thread | One-shot |
| `submit_parallel(f, priority, owner)` | rayon pool | One-shot |
| `submit_background(f, callback, priority, owner)` | rayon pool | Fire-and-forget; result via callback |
| `register_recurring_sequential(f, priority, owner)` | Main thread | Every tick until `f` returns `false` |
| `register_recurring_parallel(f, priority, owner)` | rayon pool | Every tick until `f` returns `false` |

**Task groups** let you fan out background work and collect results in one callback:

```rust
let gid = scheduler.create_group(Box::new(|results| { /* all done */ Ok(()) }), owner_id);
scheduler.group_add_background(gid, || load_mesh("ship.glb"));
scheduler.group_add_background(gid, || load_texture("ship.png"));
scheduler.group_seal(gid);   // callback fires when both members complete
```

**Cancellation:** `scheduler.cancel_owned(owner_id)` removes all pending tasks for a given owner.
The `ModuleLoader` calls this automatically when a module unloads — no orphaned work survives
module teardown.

**Cross-thread submission:**

```rust
let sender: RemoteSender = engine.remote_sender(); // Clone freely
sender.submit(Box::new(|| Ok(())), Priority(20), owner_id);
```

### Priority Phases (target 60 FPS ≈ 16.67 ms/frame)

| Priority | Phase | Typical users |
|---|---|---|
| 0 | ModuleLifecycle | Hot-reload checks, module state transitions |
| 5 | EngineSetup | One-time initialization tasks |
| 10 | PreUpdate | Input polling (`PlayerController`) |
| 15 | GameEarly | `TransformSystem` world-transform propagation |
| 20 | GameUpdate | Physics step, `Scene::drain_commands()`, script events |
| 25 | GameLate | Camera, lights, UI command processing |
| 30 | RenderEnqueue | `RenderSystem` builds draw list |
| 35 | RenderExecute | Renderer sorts and executes draw commands |
| 40 | Idle | Deferred maintenance, streaming loads, LRU eviction |

---

## 6. How to Create a New Engine Module

### Step 1 — Create the crate

Add a new crate at `crates/rython-mymodule/` with a `Cargo.toml` that declares the correct layer
dependencies. Register it in the workspace `Cargo.toml` under `[workspace.members]`. Add any new
third-party dependencies to `[workspace.dependencies]` in the root `Cargo.toml`.

### Step 2 — Implement the `Module` trait

```rust
use rython_core::{EngineError, SchedulerHandle};
use rython_modules::Module;

pub struct MyModule {
    // internal state
}

impl MyModule {
    pub fn new() -> Self {
        Self {}
    }
}

impl Module for MyModule {
    fn name(&self) -> &str {
        "MyModule"
    }

    fn dependencies(&self) -> Vec<String> {
        // List modules that must be loaded before this one.
        vec!["Scene".into()]
    }

    fn on_load(&mut self, scheduler: &dyn SchedulerHandle) -> Result<(), EngineError> {
        // Submit initialization work. Use submit_sequential or register_recurring_sequential.
        scheduler.submit_sequential(
            Box::new(|| {
                // initialization logic
                Ok(())
            }),
            rython_core::Priority(20),
            rython_core::OwnerId(0), // use a unique owner ID
        );
        Ok(())
    }

    fn on_unload(&mut self, scheduler: &dyn SchedulerHandle) -> Result<(), EngineError> {
        // Submit cleanup work if needed.
        // The scheduler cancels all tasks owned by this module automatically.
        let _ = scheduler;
        Ok(())
    }
}
```

### Step 3 — Register with `EngineBuilder`

In `crates/rython-cli/src/main.rs` (or your own binary), add:

```rust
.add_module(Box::new(MyModule::new()))
```

The order of `add_module()` calls does not matter; the `ModuleLoader` determines load order from
the `dependencies()` declarations.

### Step 4 — Write tests

- Pure-logic tests run in CI without special setup.
- GPU-dependent tests must be marked `#[ignore = "requires hardware"]` so they are skipped in
  headless CI but can be run manually.

---

## 7. Crate Reference

**`rython-core`** — Foundation used by every other crate. Provides primitive ID types (`EntityId`,
`OwnerId`, `TaskId`, `GroupId`, `Priority`), the three-layer error hierarchy (`EngineError` wraps
`TaskError`), configuration structs (`EngineConfig`, `SchedulerConfig`, `WindowConfig`), the
`SchedulerHandle` trait (the interface modules use to submit tasks), and re-exports of `glam` math
types (`Vec3`, `Mat4`, `Quat`).

**`rython-scheduler`** — The central `TaskScheduler`. Owns four task queues (one-shot sequential,
one-shot parallel, recurring sequential, recurring parallel), a rayon `ThreadPool`, a crossbeam
remote-submission channel (`RemoteSender`), a background-completion channel, a task-group registry,
and a `FramePacer`. All engine work is driven by `tick()`.

**`rython-modules`** — The `Module` trait and `ModuleLoader`. At startup, all modules are
registered and the loader builds a topological sort of their dependency graph. `load_all()` calls
`on_load()` in post-order; `unload_all()` calls `on_unload()` in reverse. Shared dependencies are
reference-counted and unload only when their last dependent unloads. Modules that return
`is_exclusive() == true` enforce single-owner control.

**`rython-ecs`** — Entity-Component-System: `Scene`, `ComponentStorage` (one `RwLock<HashMap<EntityId, Box<dyn Component>>>` per component type), a command queue (`SpawnEntity`, `DespawnEntity`, `AttachComponent`, `DetachComponent`, `SetParent`), `TransformSystem` (propagates world transforms through the entity hierarchy), `RenderSystem` (emits `DrawCommand::DrawMesh` for visible entities), and an event bus for custom game events.

**`rython-window`** — Thin winit wrapper. `WindowModule` handles window creation, resize, and
close events. Exposes `WindowConfig` (title, width, height).

**`rython-input`** — `PlayerController` polls winit window events and maps hardware inputs
(`KeyCode`, `MouseButton`, `GamepadButton`, `GamepadAxisType`, `RawInputEvent`) to logical
`InputAction`s through an `InputMap`. Exposes `is_btn_active` and `eval_axis` free functions for
use by game scripts.

**`rython-renderer`** — wgpu 24 render pipeline: `GpuContext` (adapter + device + queue +
`surface_format`), `RendererState` (holds the render pipeline, `MeshBufferCache`, and depth
texture), `Camera` (perspective projection, position, look-at), and a `DoubleBufferedQueue` of
`DrawMesh` commands. Mesh data is uploaded once to GPU buffers keyed by mesh ID; depth texture is
created lazily and re-created on resize.

**`rython-physics`** — rapier3d 0.22 integration: `PhysicsModule` and `PhysicsWorld`. Each frame,
`PhysicsPipeline::step()` advances the simulation; body positions are synced back to ECS
transforms. `CollisionEvent` handling distinguishes sensor triggers
(`CollisionEventFlags::SENSOR`) from solid collisions. Rigid bodies are created with
`RigidBodyBuilder::dynamic()` or `fixed()`.

**`rython-audio`** — kira 0.8 spatial audio: `AudioManager` wraps an optional `KiraInner`
(initialized in `on_load()`). Sounds are loaded as `StaticSoundData` and played via the builder
API (`.volume()`, `.loop_region()`, etc.) because `StaticSoundSettings` is `#[non_exhaustive]`.

**`rython-resources`** — `ResourceManager`: asset handles are `Arc<Mutex<AssetEntry>>`; decoding
runs on rayon threads; results are passed back via crossbeam channels; an LRU cache manages memory
budget. Also exposes `generate_cube()`, which returns a 24-vertex `MeshData` (4 vertices per face,
split normals, CCW winding) with 36 indices.

**`rython-ui`** — `UIManager`: widget tree, layout engine, `Theme`, and animation system built on
top of `rython-renderer` and `rython-input`. Widgets emit events that scripts can subscribe to via
the ECS event bus.

**`rython-scripting`** — PyO3 0.28 bridge: `ScriptingModule` initializes the Python interpreter,
sets the `ACTIVE_SCENE` global (`OnceLock<Arc<Mutex<Option<Arc<Scene>>>>>`), and exposes the
`rython` Python package. Each frame, `flush_recurring_callbacks(py)` runs Python per-frame
callbacks; `drain_draw_commands()` collects text-overlay requests. The `dev-reload` feature adds
file-watcher hot-reload. The bridge is implemented as a `bridge/` directory module (14 files:
`mod.rs`, plus per-class files for `scene`, `entity`, `camera`, `renderer`, `scheduler`, `time`,
`types`, `input`, `physics`, `resources`, `audio`, `ui`, `engine`).

**`rython-engine`** — `Engine` and `EngineBuilder`. `build()` creates a `TaskScheduler` and
`ModuleLoader` but does not boot. `boot()` triggers `load_all()`; `shutdown()` triggers
`unload_all()`. `run_headless(n)` runs `n` ticks without a platform event loop. `remote_sender()`
returns a `RemoteSender` for submitting tasks from worker threads.

**`rython-cli`** — Binary entry point. Parses `--script-dir`, `--entry-point`, `--config`, and
`--headless` CLI flags. Builds and boots the engine with all standard modules registered. In
windowed mode, implements winit 0.30 `ApplicationHandler`: the wgpu surface and `RendererState`
are created in `resumed()`; `RedrawRequested` runs the full tick-and-render cycle; `CloseRequested`
shuts the engine down cleanly. In headless mode, runs a simple `loop { engine.tick() }` until
`rython.engine.request_quit()` is called from Python.

**`rython-editor`** — Visual editor binary. Uses eframe 0.31 with a wgpu backend
(`egui_wgpu::RenderState`). Embeds a custom wgpu viewport panel that renders the scene alongside
egui panels. State is organized into `ViewportState` (camera, gizmos), `SelectionState` (single
and multi-entity selection), `UndoStack` (command pattern with `BatchCommand`), `ProjectState`
(loaded project config and scene), and `Preferences` / `RecentProjects` (persisted to
`~/.config/rython-editor/`). An `EditorTab` enum (Viewport / UiEditor) controls the central panel.
`AssetBrowserPanel` supports drag-and-drop of assets into the scene. A `PlaySession` struct manages
the play-in-editor child process and communicates via stdout/stderr channels.

# RythonEngine — Developer Reference

RythonEngine is a game engine where Rust implements all performance-critical systems (rendering, physics, audio, scheduling, input, ECS) and Python provides game logic through a PyO3 binding layer. The `game/` directory is a demonstration of the scripting API, not the primary development target.

---

## Core Philosophy

**Rust owns performance; Python owns game logic.** The boundary between the two is intentional and should not blur. Rust code in the engine core has no knowledge of Python semantics, and Python scripts have no direct access to engine internals — only the `rython` API surface.

**No panics.** Errors propagate as `Result<_, EngineError>` throughout the engine. `unwrap()` and `expect()` have no place in engine code; surface errors through the three-layer error hierarchy instead.

**Deterministic frame execution.** The 15-step frame pipeline runs in a fixed, documented order. The correctness of physics, rendering, and input depends on this ordering — steps are not interchangeable.

**Command-based mutation.** Systems never mutate shared state directly. They enqueue command enums that are drained atomically at deterministic frame boundaries (step 4). This ensures ECS state is consistent when the physics and render steps run.

**Composition over feature flags.** Engine capabilities are opt-in `Module`s registered via `EngineBuilder`. To disable a feature, omit the `add_module()` call. Cargo feature flags are not used for engine capabilities.

---

## Layered Crate Architecture

The engine is a Cargo workspace. **Lower layers never depend on higher layers.** Each crate may only depend on crates in the same or lower layer.

```
Layer 0   rython-core
Layer 1   rython-scheduler   rython-modules
Layer 2   rython-ecs   rython-window   rython-input   rython-renderer
          rython-physics   rython-audio   rython-resources
Layer 3   rython-ui   rython-scripting
Layer 4   rython-engine
Binary    rython-cli   rython-editor
```

**Layer descriptions:**
- **Layer 0 — `rython-core`:** Foundation types (`EntityId`, `OwnerId`, `Priority`, `TaskId`), error hierarchy (`EngineError`, `TaskError`), config structs, and `glam` math re-exports. No internal engine dependencies.
- **Layer 1 — `rython-scheduler`:** `TaskScheduler` and `FramePacer`. All module-registered engine work flows through `tick()`.
- **Layer 1 — `rython-modules`:** `Module` trait and `ModuleLoader`. Manages dependency-ordered loading, reference-counted shared dependencies, and lifecycle.
- **Layer 2 — domain crates:** Independent feature crates for ECS, windowing, input, rendering, physics, audio, and resources.
- **Layer 3 — `rython-ui`:** Widget tree, layout, theme, and animation built on renderer and input.
- **Layer 3 — `rython-scripting`:** PyO3 0.28 bridge that exposes the `rython` Python module, manages `ACTIVE_SCENE`, and handles hot-reload.
- **Layer 4 — `rython-engine`:** `Engine` and `EngineBuilder` — assembles scheduler, module loader, and `Arc<Scene>` into a runnable unit.
- **Binary — `rython-cli`:** Drives the windowed or headless event loop.
- **Binary — `rython-editor`:** Standalone visual editor (egui + wgpu). Depends on `rython-core`, `rython-ecs`, `rython-renderer`, `rython-resources`, and `rython-ui` directly — **no PyO3 dependency, no embedded Python interpreter**.

New crates belong at the lowest layer that satisfies their dependencies.

---

## Frame Timeline

Each frame runs 15 steps in fixed order (60 FPS target):

```
1.  set_elapsed_secs()           -- advance Python time
2.  flush_recurring_callbacks()  -- run Python per-frame callbacks
3.  flush_timers()               -- fire pending on_timer callbacks
4.  scene.drain_commands()       -- apply queued ECS mutations atomically
5.  PhysicsWorld::sync_step()    -- rapier3d physics step + sync to ECS transforms
6.  PlayerController::tick()     -- process raw input → input snapshot + action events
7.  UIManager: mouse routing     -- route cursor and click events to widgets
8.  TransformSystem::run()       -- propagate world transforms through entity hierarchy
9.  RenderSystem::run()          -- collect DrawMesh commands from visible ECS entities
10. drain_draw_commands()        -- collect script and UI overlay draw commands
11. renderer.render_meshes()     -- GPU mesh draw call
12. renderer.render_rects()      -- solid-color UI rect overlays
13. renderer.render_text()       -- text overlays (scripts + UI)
14. frame.present()              -- present swapchain
15. engine.tick()                -- TaskScheduler pipeline (module-registered tasks)
```

Steps 1–14 run synchronously in the CLI event loop. Step 15 (the `TaskScheduler` pipeline) is where module-registered tasks at priorities 0–40 execute — it is not interleaved with the game loop steps above.

---

## Key Architectural Patterns

### Module Lifecycle
Every engine system implements the `Module` trait with `on_load()` and `on_unload()`. `ModuleLoader` resolves the dependency graph, loads modules in post-order, and unloads in reverse. Shared dependencies are reference-counted. Registering a module via `EngineBuilder::add_module()` is the only supported way to add an engine system.

### TaskScheduler
Module-registered tasks run at priorities 0–40 inside `engine.tick()` (step 15). The core game loop steps (1–14) are not routed through the scheduler — they run inline in the CLI event loop.

### Shared Scene
`Arc<Scene>` is shared between the ECS system and the Python bridge. Python-facing mutations (spawn, despawn, transform updates) are queued as command enums and drained atomically at step 4, before physics and rendering run. Never mutate `ACTIVE_SCENE` directly from the bridge.

### Error Hierarchy
```
Python exception
    └── TaskError      (wraps task-level failures)
            └── EngineError  (engine-level Result type)
```
Rust code returns `Result` throughout. Nothing panics. Python exceptions are caught at the bridge boundary and wrapped into `TaskError`.

### Hot-Reload
Dev builds include a file watcher that reloads modified Python scripts without restarting the engine. Python state is not guaranteed to persist across reloads — scripts should be written assuming `init()` may be called again.

---

## Python Bridge

The PyO3 bridge lives in `crates/rython-scripting/src/bridge/`. The bridge is split across ~16 files rather than a monolithic module — one file per sub-API (audio, camera, entity, input, physics, renderer, resources, scene, scheduler, time, ui, etc.).

To add a new Python-facing API:
1. Add a module file in `bridge/` (e.g., `bridge/myfeature.rs`)
2. Register it in `bridge/mod.rs` and add it to the `rython` `PyModule`
3. Update the Python stubs in `rython/` to keep IDE autocomplete accurate

The `rython/` directory contains **pure-Python stubs** (PEP 561). They are a dev-time IDE aid only — the real `rython` module is injected at runtime by PyO3 and overrides these stubs. Game builds do not include `rython/`.

---

## Key File Locations

| What | Where |
|---|---|
| Foundation types, errors, config | `crates/rython-core/src/` |
| Module trait and loader | `crates/rython-modules/src/` |
| Task scheduler | `crates/rython-scheduler/src/` |
| ECS (entities, components, scene) | `crates/rython-ecs/src/` |
| Rendering (wgpu pipeline) | `crates/rython-renderer/src/` |
| Physics (rapier3d) | `crates/rython-physics/src/` |
| Audio (kira) | `crates/rython-audio/src/` |
| Input (keyboard, mouse, gamepad) | `crates/rython-input/src/` |
| UI widget system | `crates/rython-ui/src/` |
| PyO3 bridge | `crates/rython-scripting/src/bridge/` |
| Python stubs (IDE only) | `rython/` |
| EngineBuilder | `crates/rython-engine/src/builder.rs` |
| CLI entry point | `crates/rython-cli/src/main.rs` |
| Visual editor | `crates/rython-editor/src/` |
| Engine internals docs | `docs/engine/README.md` |
| Python scripting API docs | `docs/game/README.md` |

---

## Build & Test

```bash
make build        # debug build (fast compile, GPU validation layers on)
make release      # optimized build (LTO, single codegen unit, stripped symbols)
make test         # cargo test --workspace
make stubs        # pip install -e . (Python stubs for IDE)
make run          # run the example game (game/scripts/)
make clean        # remove build artifacts
```

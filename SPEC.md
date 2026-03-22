# RythonEngine

A game engine where engine modules are written in Rust and game modules are written in Python.

```
+-------------------------------------+
|         PYTHON (Game Logic)         |  Scripts, game objects, AI, UI logic
+-----------------+-------------------+
|         BINDING LAYER (PyO3)        |  Rust-native Python bindings
+-----------------+-------------------+
|         RUST ENGINE CORE            |  Scheduler, renderer, physics, audio, ECS, windowing
+-------------------------------------+
```

Game developers write Python scripts that interact with the engine through a `rython` module exposed by PyO3. The engine handles all performance-critical work in Rust: rendering, physics, audio, scheduling, input, and resource management.


## Dev vs Release

```
Development Build                    Release Build
---------------------                ---------------------
Scripts loaded from disk         ->  Scripts bundled in binary
Hot-reload on file change        ->  No hot-reload
Full error tracebacks            ->  Friendly error screen
Python installed locally         ->  Python embedded + invisible
GPU validation layers enabled    ->  Validation disabled
Debug logging                    ->  Warn-level logging
```

In development, the engine compiles as a Rust binary that loads Python scripts from a `scripts/` directory on disk. A file watcher detects changes and hot-reloads modified scripts without restarting. In release, a bundler tool packages scripts into an archive embedded alongside the binary. The final release artifact is a single executable with an embedded Python interpreter.

```
Release Binary:
+---------------------------------------------+
|              game.exe                        |
|                                              |
|  +--------------+    +--------------------+  |
|  | Rust Engine  | <- | Embedded CPython   |  |
|  |    Core      |    | (libpython)        |  |
|  +--------------+    +--------------------+  |
|                              |               |
|                     +----------------+       |
|                     |  game scripts  |       |
|                     |  (bundled in   |       |
|                     |   resources)   |       |
|                     +----------------+       |
+---------------------------------------------+
```


## Architecture Principles

These principles are inherited from the PythonEngine and must be preserved:

1. **Task-driven execution** - All work flows through the TaskScheduler. Engine systems do not tick themselves; they submit tasks at declared priorities. The scheduler runs tasks in a fixed per-frame pipeline.

2. **Module lifecycle management** - Engine systems are Modules with dependency-injected lifecycles. The ModuleLoader builds a dependency graph, loads in post-order, unloads in reverse. Shared dependencies are reference-counted.

3. **Event-driven scripting** - Game logic reacts to named events (collisions, input actions, entity spawns), not per-frame ticks. Scripts declare handlers that the engine calls when events fire.

4. **Three-layer error model** - Python exceptions are caught and wrapped into TaskErrors. TaskErrors are wrapped into EngineErrors. Rust uses Result everywhere; nothing panics.

5. **Ownership-based cleanup** - Every task has an owner (a Module). When a module unloads, all its tasks are cancelled automatically. No orphaned work survives module teardown.

6. **Command-based mutation** - Systems expose their API as command enums submitted to queues. Commands are drained and applied at deterministic points in the frame, avoiding race conditions.


## Crate Layout

The engine is organized as a Cargo workspace with layered crates. Lower layers never depend on higher layers.

```
RythonEngine/
+-- Cargo.toml                    # Workspace root
+-- crates/
|   +-- rython-core/              # Layer 0: types, errors, events, config, math
|   +-- rython-scheduler/         # Layer 1: task scheduler, frame pacer
|   +-- rython-modules/           # Layer 1: module trait, loader, registry
|   +-- rython-ecs/               # Layer 2: entities, components, scene, systems
|   +-- rython-window/            # Layer 2: winit window management
|   +-- rython-input/             # Layer 2: keyboard, mouse, gamepad
|   +-- rython-renderer/          # Layer 2: wgpu rendering pipeline
|   +-- rython-physics/           # Layer 2: rapier3d physics
|   +-- rython-audio/             # Layer 2: kira spatial audio
|   +-- rython-resources/         # Layer 2: asset loading and streaming
|   +-- rython-ui/                # Layer 3: widget system
|   +-- rython-scripting/         # Layer 3: PyO3 bridge, hot-reload
|   +-- rython-engine/            # Layer 4: assembles all modules, entry point
+-- scripts/                      # Game scripts (Python)
+-- assets/                       # Game assets
+-- engine/
|   +-- assets/                   # Engine assets (logo, fallback font)
|   +-- data/                     # Engine config files (JSON)
+-- tools/
    +-- bundler/                  # Release build script bundler
```


## Dependency DAG

```
rython-core (Layer 0)
    |
    +---> rython-scheduler (Layer 1)
    |         |
    +---> rython-modules (Layer 1)
    |         |
    +---> rython-ecs (Layer 2)
    +---> rython-window (Layer 2)
    +---> rython-input (Layer 2) -------> rython-window
    +---> rython-renderer (Layer 2) ----> rython-ecs, rython-window
    +---> rython-physics (Layer 2) -----> rython-ecs
    +---> rython-audio (Layer 2)
    +---> rython-resources (Layer 2)
    |
    +---> rython-ui (Layer 3) ----------> rython-renderer, rython-input
    +---> rython-scripting (Layer 3) ---> rython-ecs, rython-scheduler, rython-modules
    |
    +---> rython-engine (Layer 4) ------> ALL crates
```


## Rust Library Stack

| Concern | Crate | Replaces (PythonEngine) |
|---------|-------|------------------------|
| Windowing | winit | glfw |
| GPU rendering | wgpu | ModernGL (OpenGL 3.3) |
| Physics | rapier3d | PyBullet |
| Audio | kira | PyOpenAL |
| Math | glam | PyGLM, numpy |
| Python bindings | PyO3 | native Python |
| Thread pool | rayon | threading, multiprocessing |
| Channels | crossbeam | multiprocessing.Queue |
| Serialization | serde, serde_json | json stdlib |
| Error handling | thiserror | Python exceptions |
| File watching | notify | N/A |
| Gamepad | gilrs | glfw gamepad API |
| Locking | parking_lot | threading.Lock |
| Downcasting | downcast-rs | isinstance / type() |


## Main Loop

The entry point creates the engine, loads config, bootstraps all modules, and runs the winit event loop. Each frame, the scheduler's `tick()` drives all work.

```
Engine Start
|
+-> Load engine.json config
+-> Create TaskScheduler (target FPS from config)
+-> Create ModuleLoader
+-> Register all engine modules
+-> Load modules in dependency order (post-order)
|
+-> Enter winit event loop:
|     |
|     +-> Window events -> forwarded to InputModule
|     +-> AboutToWait -> scheduler.tick()
|     |     |
|     |     +-> Drain remote queue (cross-thread task submissions)
|     |     +-> Sequential phase (main thread, priority-sorted)
|     |     +-> Parallel phase (rayon pool, concurrent)
|     |     +-> Background phase (fire-and-forget)
|     |     +-> Frame pacing (sleep + spin to target FPS)
|     |
|     +-> CloseRequested -> break
|
+-> Unload all modules (reverse order)
+-> Shutdown scheduler
```


## Typical Frame Timeline (60 FPS = ~16.67ms)

```
Priority  0: ModuleLifecycle  - hot-reload check, module state transitions
Priority  5: EngineSetup      - one-time initialization tasks
Priority 10: PreUpdate        - InputModule polls window events, finalizes input state
Priority 15: GameEarly        - TransformSystem propagates world transforms
Priority 20: GameUpdate       - PhysicsModule steps, Scene drains commands, script events
Priority 25: GameLate         - Camera/light/UI command processing, script reactions
Priority 30: RenderEnqueue    - RenderSystem queries visible entities, builds draw list
Priority 35: RenderExecute    - Renderer sorts and executes draw commands, swaps buffers
Priority 40: Idle             - Deferred maintenance, streaming loads, LRU eviction
```


## Module Specs

Each engine module has a detailed spec in `.spec/`:

| Spec | Covers |
|------|--------|
| [task-scheduler](.spec/task-scheduler.md) | Task types, tick pipeline, frame pacing, ownership cancellation |
| [module-loader](.spec/module-loader.md) | Module lifecycle, dependency injection, ref-counting |
| [ecs](.spec/ecs.md) | Scene, components, systems, event bus, entity hierarchy |
| [renderer](.spec/renderer.md) | wgpu pipeline, draw commands, camera, lighting |
| [physics](.spec/physics.md) | rapier3d integration, collision events, triggers |
| [audio](.spec/audio.md) | kira spatial audio, categories, 3D listener |
| [input](.spec/input.md) | PlayerController, InputMap, gamepad support |
| [ui](.spec/ui.md) | Widget tree, layout, theme, animation |
| [resources](.spec/resources.md) | Asset handles, streaming budget, decoders |
| [scripting](.spec/scripting.md) | PyO3 bridge, hot-reload, Python API |
| [errors](.spec/errors.md) | Three-layer error hierarchy |
| [threading](.spec/threading.md) | Thread safety, locking, GIL strategy |


## Acceptance Tests

These tests verify the engine builds, links, and runs correctly as an integrated system.

### T-SPEC-01: Workspace Compilation
Build the entire workspace in debug mode. Every crate must compile without errors or warnings treated as errors. The build must complete within 5 minutes on a machine with 8 cores and 16 GB RAM.
- Expected: `cargo build --workspace` exits with code 0
- Expected: Zero compiler warnings (with `#![deny(warnings)]` in each crate's lib.rs)

### T-SPEC-02: Dependency DAG Acyclicity
Write a build script or test that parses every crate's `Cargo.toml` and constructs the internal dependency graph. Verify that no cycles exist and that no lower-layer crate depends on a higher-layer crate.
- Expected: Layer 0 crates have zero internal dependencies
- Expected: Layer 1 crates depend only on Layer 0
- Expected: Layer 2 crates depend only on Layer 0 and Layer 1
- Expected: Layer 3 crates depend only on Layers 0-2
- Expected: Layer 4 depends on any

### T-SPEC-03: Engine Boot and Shutdown
Start the engine with a minimal config (no game scripts, no assets). The engine must create a window, run at least 60 ticks, and shut down cleanly without leaking resources.
- Expected: Window appears within 2 seconds of launch
- Expected: At least 60 scheduler ticks complete
- Expected: All modules transition through LOADING -> LOADED -> UNLOADING in order
- Expected: Process exits with code 0, no panics, no leaked threads

### T-SPEC-04: Dev vs Release Feature Flags
Build the engine with `--features dev-reload` and verify the file watcher is compiled in. Build without it and verify the file watcher code is absent (binary size difference or feature-gated API unavailability).
- Expected: With `dev-reload`, `ScriptingModule` exposes hot-reload functionality
- Expected: Without `dev-reload`, `ScriptingModule` loads from bundle only
- Expected: Release binary with LTO + strip is under 50 MB (excluding embedded Python)

### T-SPEC-05: Frame Timeline Ordering
Run the engine for 100 ticks with dummy modules that log their priority when executed. Verify the execution order matches the priority table on every tick.
- Expected: For every tick, tasks execute in strict ascending priority order: 0, 5, 10, 15, 20, 25, 30, 35, 40
- Expected: No priority inversion occurs across 100 ticks

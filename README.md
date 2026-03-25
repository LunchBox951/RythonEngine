# RythonEngine

A game engine where engine modules are written in Rust and game logic is written in Python.

```
+-------------------------------------+
|         PYTHON (Game Logic)         |  Scripts, game objects, AI, UI logic
+-----------------+-------------------+
|         BINDING LAYER (PyO3)        |  Rust-native Python bindings
+-----------------+-------------------+
|         RUST ENGINE CORE            |  Scheduler, renderer, physics, audio, ECS, windowing
+-------------------------------------+
```

Game developers write Python scripts that interact with the engine through a `rython` module exposed by PyO3. The engine handles all performance-critical work in Rust: rendering (wgpu), physics (rapier3d), audio (kira), scheduling, input, and resource management.

---

## Prerequisites

- **Rust** — stable toolchain, edition 2021 (`rustup update stable`)
- **Python 3.8+** — with development headers
  - Linux: `sudo apt install python3-dev` (Debian/Ubuntu) or `python3-devel` (Fedora/RHEL)
  - macOS: `brew install python` (headers included); Xcode Command Line Tools required
  - Windows: install via python.org; ensure "Install for all users" is checked so headers are present
- **GPU** — a wgpu-compatible adapter (Vulkan, Metal, DX12, or WebGPU); the engine selects the best available backend automatically

---

## Building

```bash
# Debug build (fast compile, dev validation layers on)
make build
# or: cargo build

# Release build (LTO, optimized, no validation)
make release
# or: cargo build --release

# Run all workspace tests
make test
# or: cargo test --workspace
```

---

## Python IDE Setup

The `rython/` directory at the repo root is a pure-Python stub package. Installing it in editable mode gives Pylance (and any PEP 484-compliant type checker) full autocompletion, inline docs, and type errors for `import rython` in your scripts.

```bash
make stubs
# or:
pip install -e .
```

After the install, restart your language server (VS Code: **Pylance → Restart Language Server**; or reopen the project). The `py.typed` marker in `rython/` tells Pylance to use the stubs directly.

> **Note:** The stub package is a development-time tool only. The real `rython` module is injected at runtime by the Rust engine via PyO3 and overrides these stubs; game builds do not bundle the `rython/` directory.

---

## Quick Start

Create a `game/scripts/main.py` with an `init()` function. The engine imports the entry-point module and calls `init()` on load:

```python
import math
import rython

def init():
    # Position the camera
    rython.camera.set_position(0.0, 4.0, -10.0)
    rython.camera.set_look_at(0.0, 0.0, 0.0)

    # Spawn a cube at the origin
    cube = rython.scene.spawn(
        transform=rython.Transform(x=0.0, y=0.0, z=0.0, scale=1.0),
        mesh="cube",
        tags=["player"],
    )

    # Spin it every frame
    t0 = [0.0]
    def on_tick():
        cube.transform.rot_y = rython.time.elapsed
        rython.renderer.draw_text(
            f"t={rython.time.elapsed:.2f}s",
            font_id="default", x=0.02, y=0.02, size=20,
            r=255, g=255, b=200,
        )
    rython.scheduler.register_recurring(on_tick)
```

Run it:

```bash
make run SCRIPT_DIR=game/scripts
# or:
cargo run -p rython-cli -- --script-dir game/scripts --entry-point main
```

---

## Running

```
Usage: rython [OPTIONS]

Options:
  --script-dir <DIR>      Directory containing Python scripts  [default: ./scripts]
  --entry-point <MODULE>  Python module to import and call init()
  --config <FILE>         Engine config JSON file
  --headless              Run without creating a window
  -h, --help              Print this help
```

```bash
# Windowed with an entry point
cargo run -p rython-cli -- --script-dir game/scripts --entry-point main

# Headless (CI, tests, servers)
cargo run -p rython-cli -- --script-dir game/scripts --headless

# Custom engine config
cargo run -p rython-cli -- --config engine.json --script-dir game/scripts

# Bundle scripts for release distribution
make bundle SCRIPT_DIR=game/scripts OUT=bundle.zip
```

---

## Project Structure

The engine is a Cargo workspace with layered crates. Lower layers never depend on higher ones.

```
RythonEngine/
+-- Cargo.toml                    # Workspace root
+-- Makefile
+-- pyproject.toml                # Python stub package config
+-- docs/
|   +-- engine/                   # Rust implementation docs
|   +-- game/                     # Python scripting docs
+-- crates/
|   +-- rython-core/              # Layer 0 — types, errors, events, config, math
|   +-- rython-scheduler/         # Layer 1 — task scheduler, frame pacer
|   +-- rython-modules/           # Layer 1 — Module trait, loader, registry
|   +-- rython-ecs/               # Layer 2 — entities, components, scene, systems
|   +-- rython-window/            # Layer 2 — winit window management
|   +-- rython-input/             # Layer 2 — keyboard, mouse, gamepad (gilrs)
|   +-- rython-renderer/          # Layer 2 — wgpu rendering pipeline
|   +-- rython-physics/           # Layer 2 — rapier3d physics integration
|   +-- rython-audio/             # Layer 2 — kira spatial audio
|   +-- rython-resources/         # Layer 2 — asset loading and streaming
|   +-- rython-ui/                # Layer 3 — widget system
|   +-- rython-scripting/         # Layer 3 — PyO3 bridge, hot-reload
|   +-- rython-engine/            # Layer 4 — EngineBuilder, integration entry point
|   +-- rython-cli/               # Binary — windowed + headless CLI
|   +-- rython-editor/            # Binary — visual scene/UI editor (egui + wgpu)
+-- rython/                       # Pure-Python stub package (PEP 561, IDE autocompletion)
+-- game/                         # Example game project
|   +-- scripts/                  # Python game scripts
|   +-- assets/                   # Game assets (textures, audio)
```

---

## Architecture Overview

### Design Principles

1. **Task-driven execution** - All work flows through the `TaskScheduler`. Engine systems submit tasks at declared priorities; the scheduler runs them in a fixed per-frame pipeline.
2. **Module lifecycle management** - Engine systems are `Module`s with dependency-injected lifecycles. The `ModuleLoader` builds a dependency graph, loads in post-order, and unloads in reverse.
3. **Flexible scripting model** - Game logic can register per-frame callbacks via `rython.scheduler.register_recurring()` for continuous updates, and react to named events (collisions, input actions, entity spawns) via the event bus. Both patterns are supported.
4. **Three-layer error model** - Python exceptions wrap into `TaskError`, which wraps into `EngineError`. Rust uses `Result` throughout; nothing panics.
5. **Command-based mutation** - Systems expose their API as command enums submitted to queues, drained at deterministic frame boundaries.

### Frame Timeline (60 FPS target)

```
Priority  0  ModuleLifecycle  -- hot-reload check, module state transitions
Priority 10  PreUpdate        -- InputModule polls window events, finalizes input state
Priority 15  GameEarly        -- TransformSystem propagates world transforms
Priority 20  GameUpdate       -- PhysicsModule steps, Scene drains commands, script events
Priority 25  GameLate         -- Camera/light/UI command processing, script reactions
Priority 30  RenderEnqueue    -- RenderSystem queries visible entities, builds draw list
Priority 35  RenderExecute    -- Renderer sorts and executes draw commands, presents frame
Priority 40  Idle             -- Deferred maintenance, streaming loads, LRU eviction
```

---

## Development vs Release

| | Development | Release |
|---|---|---|
| Scripts | Loaded from disk | Bundled in binary |
| Hot-reload | Yes (file watcher) | No |
| Error output | Full Python tracebacks | Friendly error screen |
| Python runtime | Installed locally | Embedded and invisible |
| GPU validation | Enabled | Disabled |
| Logging | Debug | Warn |

---

## Documentation

| Path | Contents |
|------|----------|
| `docs/engine/` | Rust implementation docs: EngineBuilder, Module trait, crate reference, scheduler, how to write engine modules |
| `docs/game/` | Python scripting docs: `rython` API, script classes, entity spawning, camera, events, complete examples |

### Editor

`rython-editor` is a standalone visual editor binary built with egui and wgpu. Launch it with:

```bash
cargo run -p rython-editor
```

The editor provides scene hierarchy inspection, asset browsing, a viewport with gizmos, and a UI layout editor. It reads and writes the same `game/scenes/*.json` ECS scene format used by the runtime.

---

## Technology Stack

| Concern | Crate |
|---------|-------|
| Windowing | winit 0.30 |
| GPU rendering | wgpu 24 |
| Physics | rapier3d 0.22 |
| Audio | kira 0.8 |
| Math | glam 0.29 |
| Python bindings | PyO3 0.28 |
| Thread pool | rayon |
| Channels | crossbeam |
| Gamepad | gilrs 0.10 |
| Locking | parking_lot |
| Asset formats | image, gltf, hound, fontdue |
| Serialization | serde, serde_json |
| Error handling | thiserror |

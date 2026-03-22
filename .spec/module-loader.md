# Module Loader

The Module Loader is a dependency-injection and lifecycle-management framework. Every engine system (renderer, physics, audio, input, etc.) is a Module. The loader orchestrates how modules discover their dependencies, load in the correct order, and unload cleanly.


## Core Concept

Every Module declares its dependencies as a list of other Module types. The loader builds a dependency graph and ensures:

- Dependencies load before their owners (depth-first post-order)
- Owners unload before their dependencies (reverse order)
- Circular ownership is detected and rejected at registration time

```python
# In a game script, modules are accessed through the registry:
import rython

renderer = rython.modules.get("Renderer")
physics = rython.modules.get("Physics")
```


## Module States

Each module transitions through three states:

```
LOADING -> LOADED -> UNLOADING
```

- **LOADING**: `on_load()` has been called. The module is submitting initialization tasks to the scheduler. Not yet ready for use.
- **LOADED**: Initialization is complete. The module is active and accepting commands.
- **UNLOADING**: `on_unload()` has been called. The module is tearing down. Other modules should not submit new work to it.

The `on_load()` and `on_unload()` methods do not block. They submit tasks to the scheduler. A completion callback signals the loader when the transition is finished.


## Dependency Resolution

When modules are registered, the loader inspects each module's declared dependencies and builds a topological sort.

Loading order is **depth-first post-order**: for a module M that depends on A and B, the loader loads A, then B, then M. If A itself depends on C, the order is C, A, B, M.

```
Example dependency graph:

GameModule
+-- RenderSystem
|   +-- Renderer
|   |   +-- Window
|   |   +-- ResourceManager
|   +-- Scene
+-- PhysicsSystem
|   +-- Scene (shared)
+-- AudioManager

Load order: Window, ResourceManager, Renderer, Scene, RenderSystem,
            PhysicsSystem, AudioManager, GameModule

Unload order: (exact reverse)
```


## Reference-Counted Shared Dependencies

When multiple modules depend on the same module, that dependency is loaded once and its reference count incremented for each dependent. The shared module unloads only when all its owners have unloaded (reference count reaches zero).

```
ModuleA -> ResourceManager  (refcount = 1)
ModuleB -> ResourceManager  (refcount = 2)

Unload ModuleA -> refcount = 1 (ResourceManager stays)
Unload ModuleB -> refcount = 0 (ResourceManager unloads)
```

This ensures shared infrastructure (Scene, ResourceManager, Window) stays alive as long as any consumer needs it.


## Single-Owner Modules and Ownership Transfer

Some modules can only have one owner at a time. These are modules where exclusive control is important — for example, the Camera or PlayerController, where only one game system should be driving behavior at a time.

Single-owner modules support ownership transfer: the current owner can hand control to another module. Only the current owner can initiate a transfer. Non-owners cannot call control methods.

```python
# Transfer camera control from the cutscene module to the gameplay module
import rython

# Only the current owner can do this:
rython.modules.transfer_ownership("Camera", new_owner=gameplay_module)

# After transfer, the old owner loses access to control methods.
# The new owner can now move the camera.
```

Ownership can also be relinquished without transferring to anyone, returning the module to an unowned state.


## Module Registration and Bootstrap

At engine startup, all modules are registered with the loader. Registration does not load modules — it just records their types and dependency declarations. Loading happens in a separate step after all registrations are complete.

The bootstrap sequence:

1. Register all engine modules (Window, Input, Renderer, Physics, Audio, etc.)
2. Register the game module (specified in config or passed as argument)
3. Call `load_all()`, which resolves the dependency graph and loads in order
4. Each module's `on_load()` submits tasks to the scheduler
5. The scheduler runs until all loading tasks complete

## Module Access

Once loaded, modules are accessible through a registry keyed by type. Any code (engine systems, tasks, scripts) can look up a module by its type to interact with it.

The registry is wrapped in a read-write lock. Read access (looking up modules) is cheap and non-blocking. Write access (loading/unloading) is rare and serialized.

For Python scripts, the registry is exposed through `rython.modules.get("ModuleName")`, which returns a Python wrapper object with access to the module's public API.


## Module Unloading

When a module unloads:

1. The loader sets its state to UNLOADING
2. The module's `on_unload()` is called, which submits cleanup tasks
3. The scheduler cancels all tasks owned by this module (ownership-based cancellation)
4. Once cleanup tasks complete, the module is removed from the registry
5. Reference counts on dependencies are decremented
6. Dependencies with refcount zero are also unloaded (cascading teardown)

Unloading respects the reverse of the load order: owners unload before their dependencies.


## Configuration

Modules load their configuration from JSON files in `engine/data/`. Each module reads its own config file during `on_load()`. If the file is missing, the module logs a warning and uses defaults.

```
engine/data/
+-- window.json       Window module config
+-- audio.json        Audio module config
+-- camera.json       Camera module config
+-- resources.json    Resource manager config
```


## Acceptance Tests

### T-MOD-01: Post-Order Load Sequence
Register modules with dependencies: D depends on nothing, C depends on D, B depends on D, A depends on B and C. Call `load_all()`.
- Expected: Load order is exactly D, C, B, A (or D, B, C, A — both valid topological sorts, but D must be first and A must be last)
- Expected: Every module's `on_load()` is called exactly once

### T-MOD-02: Reverse Unload Sequence
Using the same dependency graph as T-MOD-01, call `unload_all()` after loading.
- Expected: Unload order is the exact reverse of load order
- Expected: Every module's `on_unload()` is called exactly once

### T-MOD-03: Circular Dependency Detection
Register modules A depends on B, B depends on A. Call `load_all()`.
- Expected: `load_all()` returns an error (not a panic, not a hang)
- Expected: The error message identifies the cycle (mentions both A and B)

### T-MOD-04: Reference Counting — Shared Dependency Stays Alive
Register 3 modules: A depends on Shared, B depends on Shared, Shared depends on nothing. Load all. Unload A.
- Expected: After unloading A, Shared is still in LOADED state (refcount = 1)
- Expected: Shared's `on_unload()` has NOT been called
- Expected: Unload B. Now Shared's refcount reaches 0, and Shared unloads

### T-MOD-05: Reference Counting — Count Accuracy
Register 5 modules that all depend on Shared. Load all. Verify Shared's refcount is exactly 5. Unload them one at a time.
- Expected: After each unload, Shared's refcount decrements by exactly 1
- Expected: Shared unloads only when the 5th dependent unloads

### T-MOD-06: Module State Transitions
Register a module. Observe its state at each lifecycle point.
- Expected: Before `load_all()`, module has no entry in registry (or is in a pre-load state)
- Expected: During `on_load()`, module state is LOADING
- Expected: After `on_load()` completion callback fires, module state is LOADED
- Expected: During `on_unload()`, module state is UNLOADING
- Expected: After `on_unload()` completion, module is removed from registry

### T-MOD-07: Module Registry Lookup
Load 5 modules. Look up each by type.
- Expected: Each lookup returns the correct module instance (verified by name)
- Expected: Looking up an unregistered type returns None (not an error or panic)

### T-MOD-08: Single-Owner — Ownership Transfer
Register a single-owner module M. Owner A loads it. Owner A transfers ownership to Owner B.
- Expected: After transfer, Owner A cannot call M's control methods (returns error or is rejected)
- Expected: Owner B can call M's control methods successfully
- Expected: Only one transfer occurs (no duplication of ownership)

### T-MOD-09: Single-Owner — Non-Owner Cannot Transfer
Register a single-owner module M. Owner A loads it. Owner B (non-owner) attempts to transfer ownership.
- Expected: The transfer attempt returns an error
- Expected: Owner A retains ownership

### T-MOD-10: Single-Owner — Relinquish Ownership
Register a single-owner module M. Owner A relinquishes ownership.
- Expected: M enters an unowned state
- Expected: No owner can call M's control methods until someone claims ownership

### T-MOD-11: Cascading Teardown
Register: A depends on B, B depends on C, C depends on nothing. Only A is a "top-level" module. Unload A.
- Expected: A unloads first, then B (refcount 0), then C (refcount 0)
- Expected: All three modules are removed from the registry

### T-MOD-12: Load Triggers Scheduler Tasks
Register a module whose `on_load()` submits a sequential task that sets a flag. Call `load_all()` and tick the scheduler.
- Expected: The flag is set after the scheduler tick (on_load submitted a task, scheduler ran it)
- Expected: The module transitions to LOADED only after its loading task completes

### T-MOD-13: Missing Config Falls Back to Defaults
Load a module that reads `engine/data/window.json`, but delete the file first.
- Expected: Module loads successfully (no error)
- Expected: A warning is logged indicating the config file was not found
- Expected: Module uses hardcoded default values

# Task Scheduler

The TaskScheduler is the central execution engine. All work in the engine — rendering, physics, input polling, script dispatch, asset loading — is submitted as tasks to the scheduler. Nothing runs outside it.

This is the defining architectural choice of RythonEngine: systems do not own their own loops or call their own update methods. They submit tasks, and the scheduler decides when and how to execute them.


## Core Concept

One global scheduler instance drives the engine. Each `tick()` executes a fixed pipeline:

1. **Remote queue drain** - absorb tasks submitted from worker threads
2. **Sequential phase** - run all sequential tasks by priority (lower number = higher priority)
3. **Parallel phase** - submit all parallel tasks to the thread pool, block until done
4. **Background phase** - submit background tasks (fire-and-forget)
5. **Frame pacing** - sleep to maintain target tick rate


## Task Types

There are three types of tasks, matching distinct execution models:

### Sequential Tasks

Run on the main thread, one at a time, sorted by priority. This is where most engine work happens: input polling, scene command draining, render execution, script dispatch. Sequential tasks have exclusive access to main-thread-only resources (the wgpu surface, the winit window, the Python GIL).

```python
# From a game script: submit work to run on the main thread
import rython

rython.scheduler.submit_sequential(
    fn=self.update_ui,
    priority=rython.GAME_UPDATE,
    owner=self,
)
```

### Parallel Tasks

Run on the rayon thread pool. All parallel tasks submitted in a single tick run concurrently and the scheduler blocks until all complete. Use these for CPU-bound work that can be parallelized: transform propagation, visibility culling, batch processing.

```python
rython.scheduler.submit_parallel(
    fn=compute_navmesh,
    args=(level_data,),
    callback=self.on_navmesh_ready,
    priority=rython.GAME_UPDATE,
    owner=self,
)
```

### Background Tasks

Fire-and-forget tasks that run on the thread pool without blocking the frame. The scheduler submits them and moves on. Use these for I/O-bound work: asset loading, log flushing, network requests.

Background tasks can have an optional callback that runs as a sequential task when the background work completes. The callback receives the return value of the background function.

```python
rython.scheduler.submit_background(
    fn=load_level_data,
    args=("level_02",),
    callback=self.on_level_loaded,
    priority=rython.BACKGROUND,
    owner=self,
)
```


## Priorities

Tasks are sorted by priority within each phase. Lower numbers run first.

```
MODULE_LIFECYCLE =  0   Module load/unload, hot-reload checks
ENGINE_SETUP     =  5   One-time initialization
PRE_UPDATE       = 10   Input polling, window event processing
GAME_EARLY       = 15   Transform propagation, early game logic
GAME_UPDATE      = 20   Physics step, scene command drain, core game logic
GAME_LATE        = 25   Camera/light/UI updates, reactions to game events
RENDER_ENQUEUE   = 30   RenderSystem builds draw command list
RENDER_EXECUTE   = 35   Renderer sorts and executes draw commands
IDLE             = 40   Deferred maintenance, streaming, LRU eviction
```


## Recurring Tasks

Most engine systems need to run every frame. Rather than submitting a new task each tick, modules register recurring tasks during `on_load()`. A recurring task has a function that the scheduler calls every tick at its declared priority. The function returns a boolean: `true` to continue recurring, `false` to stop.

Recurring tasks are stored separately from one-shot tasks. They are not re-submitted each frame — the scheduler maintains them internally and invokes them during the sequential or parallel phase as appropriate.

When a module unloads, all its recurring tasks are automatically removed via ownership-based cancellation.


## Task Groups (Fan-In)

A TaskGroup collects multiple tasks and fires a single callback when all of them complete. This is the fan-in primitive for coordinating parallel or background work.

```python
group = rython.scheduler.create_group(
    callback=self.on_all_assets_loaded,
    owner=self,
)

group.add_background(fn=load_texture, args=("player.png",))
group.add_background(fn=load_texture, args=("enemy.png",))
group.add_parallel(fn=compute_normals, args=(mesh_data,))

group.seal()  # No more members can be added after this
```

The group tracks a remaining count. Each time a member task completes, the count decrements. When it hits zero, the callback is submitted as a sequential task. The callback receives a list of all member results.

Groups must be sealed before any member can complete. This prevents a race where a fast member completes before all members are added.


## Cross-Thread Submission

Tasks can be submitted from any thread. The scheduler exposes a thread-safe sender (backed by a crossbeam channel). Worker threads, background tasks, and even the audio thread can submit tasks back to the main scheduler.

On each tick, the scheduler drains this remote queue first, merging remote submissions into the local task list before executing any phases. This replaces the Python engine's `multiprocessing.Queue` with a more efficient lock-free channel.


## Ownership-Based Cancellation

Every task has an owner, identified by an opaque owner ID. When a module unloads, the ModuleLoader calls `scheduler.cancel_owned(owner_id)`, which:

1. Removes all pending one-shot tasks with that owner
2. Removes all recurring tasks with that owner
3. Sets a cancellation flag on any currently-running background tasks with that owner

Running sequential/parallel tasks cannot be cancelled mid-execution (they are synchronous), but they will not be re-submitted if recurring. Background tasks should check a cancellation token periodically for long-running work.

```python
# Cancellation happens automatically on module unload.
# Manual cancellation is also available:
rython.scheduler.cancel_tasks_for_owner(self)
```


## Frame Pacing

The scheduler maintains a target frame rate (default 60 FPS = 16.667ms per tick). After all phases complete, the frame pacer calculates remaining time and waits.

The wait uses a hybrid strategy:
- If remaining time > 1ms (configurable threshold), use `thread::sleep()` for the bulk of the wait
- For the final sub-millisecond, use a busy-spin loop with `spin_loop_hint()`

This avoids the imprecision of OS sleep (which can overshoot by 1-15ms depending on platform) while not wasting CPU on busy-spinning for the entire wait period.


## Error Handling

Tasks never panic. All task functions return a Result. If a task fails:

- **Sequential task failure**: The error is logged. The scheduler continues to the next task. The module that owns the failed task is notified via an error callback if one was registered.
- **Parallel task failure**: The error is logged. Other parallel tasks in the same batch continue. Failures do not abort the batch.
- **Background task failure**: The error is logged. If the task had a callback, the callback receives the error instead of a result.

The scheduler itself never stops due to a task failure. Only an explicit quit request or a fatal engine error (like losing the GPU device) stops the loop.


## Tick Timeline Diagram

```
Tick Start
|
+-> [Remote Queue Drain]
|       Absorb tasks from worker threads (crossbeam channel)
|
+-> [Sequential Phase]
|       Task A (priority  0) -> tick -> Continue
|       Task B (priority 20) -> tick -> Continue
|       Task C (priority 35) -> tick -> Stop -> cleanup
|                                                     | blocks
+-> [Parallel Phase]
|       Task D (priority 20) --+
|       Task E (priority 50) --+-- all run concurrently on rayon
|       Task F (priority 90) --+
|                                                     | blocks until ALL done
+-> [Background Phase]
|       (non-blocking) submit new fire-and-forget work
|       Check completed background tasks, fire callbacks
|
+-> [Frame Pacing]
|       Sleep bulk + spin final microseconds
|
Tick End

Meanwhile, in background threads:
    Task G: loading level data     (started 14 ticks ago)
    Task H: decoding audio file    (started 2 ticks ago)
```


## Configuration

The scheduler reads its settings from the `scheduler` section of `engine.json`:

```json
{
    "scheduler": {
        "target_fps": 60,
        "parallel_threads": null,
        "spin_threshold_us": 1000
    }
}
```

- `target_fps`: Target frames per second (default 60)
- `parallel_threads`: Number of rayon threads. `null` means use the number of CPU cores.
- `spin_threshold_us`: Microseconds below which the frame pacer switches from sleep to spin-wait (default 1000)


## Acceptance Tests

### T-SCHED-01: Frame Pacing Accuracy at 60 FPS
Create a scheduler with `target_fps=60`. Run 600 ticks with no work submitted (empty frames). Measure the wall-clock time of each tick.
- Expected: Mean tick duration is 16.667ms ± 0.5ms
- Expected: Standard deviation of tick duration is below 1.0ms
- Expected: No individual tick exceeds 20ms (allowing for OS scheduling jitter)
- Expected: No individual tick is shorter than 14ms
- Expected: Total elapsed time for 600 ticks is 10.0s ± 0.3s

### T-SCHED-02: Frame Pacing Accuracy at 30 FPS
Same as T-SCHED-01 but with `target_fps=30`. Run 300 ticks.
- Expected: Mean tick duration is 33.333ms ± 0.5ms
- Expected: Total elapsed time for 300 ticks is 10.0s ± 0.3s

### T-SCHED-03: Sequential Priority Ordering
Submit 5 sequential tasks in random order with priorities 40, 10, 30, 0, 20. Each task appends its priority to a shared `Vec<u8>`.
- Expected: After one tick, the vec contains `[0, 10, 20, 30, 40]` in exact order

### T-SCHED-04: Sequential Before Parallel Before Background
Submit one task of each type. Each task records a timestamp when it starts executing. Run one tick.
- Expected: Sequential task's timestamp < Parallel task's timestamp < Background task's timestamp (within the same tick)

### T-SCHED-05: Parallel Tasks Run Concurrently
Submit 4 parallel tasks. Each task sleeps for 50ms and records its thread ID. Run one tick.
- Expected: The tick completes in under 100ms (proving parallelism, not serial 200ms)
- Expected: At least 2 distinct thread IDs are recorded (proving multi-threading)

### T-SCHED-06: Background Tasks Do Not Block the Frame
Submit a background task that sleeps for 500ms. Measure the tick duration.
- Expected: The tick completes in under 20ms (background work does not block the frame)
- Expected: The background task's callback fires on a subsequent tick after ~500ms

### T-SCHED-07: Background Task Callback Receives Result
Submit a background task that returns the value 42. Provide a callback that stores the received value.
- Expected: After sufficient ticks, the callback has been invoked exactly once
- Expected: The callback received the value 42

### T-SCHED-08: Ownership-Based Cancellation
Create an owner ID. Submit 10 sequential tasks and 5 recurring tasks, all owned by that ID. Call `cancel_owned(owner_id)` before the next tick.
- Expected: None of the 10 sequential tasks execute on the next tick
- Expected: None of the 5 recurring tasks execute on any subsequent tick
- Expected: Tasks belonging to other owners are unaffected

### T-SCHED-09: Recurring Task Persistence
Register a recurring task that increments a counter. Run 100 ticks.
- Expected: The counter equals 100 after 100 ticks (task ran every tick)
- Expected: The task was not re-submitted — the scheduler maintained it internally

### T-SCHED-10: Recurring Task Self-Termination
Register a recurring task that returns `false` on its 10th invocation. Run 50 ticks.
- Expected: The task runs exactly 10 times
- Expected: No execution of the task on tick 11 or later

### T-SCHED-11: Task Group Fan-In
Create a task group with 3 background members. Each member returns a unique value. The group callback collects all results.
- Expected: The callback fires exactly once
- Expected: The callback receives exactly 3 results
- Expected: All 3 result values are present (no lost results)

### T-SCHED-12: Task Group Seal Enforcement
Create a task group. Add 2 members. Do NOT call `seal()`. Let one member complete.
- Expected: The callback does NOT fire (group is not sealed)
- Expected: After calling `seal()` and the remaining member completes, the callback fires

### T-SCHED-13: Cross-Thread Task Submission
From a background task, submit a sequential task via the remote channel. The sequential task sets a flag.
- Expected: The flag is set on the tick after the background task submits it (or within 2 ticks)
- Expected: The remotely-submitted task runs in the sequential phase, not the background phase

### T-SCHED-14: Error Handling — Task Failure Does Not Stop Scheduler
Submit 3 sequential tasks: task A succeeds, task B returns an error, task C succeeds. Run one tick.
- Expected: Task A executes successfully
- Expected: Task B's error is logged
- Expected: Task C executes successfully (scheduler did not stop)
- Expected: The scheduler's tick() returns Ok (not Err)

### T-SCHED-15: Error Handling — Panic Recovery
Submit a sequential task that panics with a message. Run one tick.
- Expected: The panic is caught (scheduler does not crash)
- Expected: A TaskError::Panicked is logged containing the panic message
- Expected: Subsequent ticks continue normally

### T-SCHED-16: Frame Pacing Under Load
Submit sequential work that takes exactly 10ms (busy-spin for 10ms). Run 100 ticks at 60 FPS.
- Expected: Mean tick duration is 16.667ms ± 1.0ms (frame pacer waits the remaining ~6.67ms)
- Expected: No tick is shorter than 16ms

### T-SCHED-17: Spin Threshold Configuration
Create a scheduler with `spin_threshold_us=0` (all sleep, no spin). Run 100 ticks at 60 FPS.
- Expected: Mean tick duration is 16.667ms ± 2.0ms (wider tolerance due to sleep imprecision)
- Expected: CPU usage during idle frames is near zero (no busy-spinning)

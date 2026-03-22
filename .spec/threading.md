# Threading

RythonEngine replaces the PythonEngine's GIL-constrained concurrency with true multi-threaded parallelism. The GIL only applies to Python script execution — all Rust engine work runs without it. This spec defines what is shared, how it is protected, and when the GIL is held.


## Thread Model

The engine runs on four categories of threads:

```
Main Thread         The winit event loop, sequential tasks, Python GIL,
                    GPU commands (wgpu), window events

Rayon Pool          Parallel tasks: transform propagation, visibility
                    culling, batch processing. Sized to CPU core count.

Background Workers  Background tasks on rayon: asset decoding, I/O.
                    Fire-and-forget, results returned via channel.

Audio Thread        Managed by kira internally. Engine communicates
                    via kira's command interface (no direct thread access).
```


## Shared State and Locking Strategy

Each shared resource uses the narrowest synchronization primitive that fits its access pattern.

### Scene (entities + components)
**Protection**: Read-write lock (parking_lot RwLock) around the Scene as a whole, with per-component-type fine-grained locks for hot-path access.

**Access pattern**: Many readers (systems querying components), few writers (command queue draining, physics sync). Reads dominate.

**Fine-grained locking**: Each component type store (all TransformComponents, all MeshComponents, etc.) has its own RwLock. Systems that operate on disjoint component types can read in parallel. For example, the TransformSystem can read/write TransformComponents while the RenderSystem reads MeshComponents, without contention.

**When writes happen**: The Scene drains its command queue once per frame during GAME_UPDATE (a sequential task on the main thread). During draining, the Scene holds write locks. No other system runs concurrently with command draining because sequential tasks run one at a time.

### Event Bus
**Protection**: Read-write lock. Subscribers are added/removed rarely (entity spawn/despawn). Event emission happens frequently but is always on the main thread (sequential tasks).

### Module Registry
**Protection**: Read-write lock. Reads (module lookup) are very frequent. Writes (module load/unload) happen only at startup and shutdown.

### Renderer Command Buffer
**Protection**: The draw command queue is double-buffered. The "back" buffer collects commands from RENDER_ENQUEUE tasks. At the phase boundary, the buffers swap (atomic pointer swap). The "front" buffer is consumed by the renderer during RENDER_EXECUTE. No lock contention because producers and consumer never access the same buffer.

### Task Scheduler
**Protection**: The scheduler is owned by the main thread. Cross-thread task submission goes through a crossbeam channel (lock-free MPSC). The main thread drains the channel at the start of each tick.

### Physics World
**Protection**: Mutex. The physics world is accessed in a single sequential task per frame (GAME_UPDATE). The mutex is held for the duration of the physics step. No other code accesses the physics world during this window.

### Audio
**Protection**: None needed from the engine side. Communication with kira is through its own command-based interface, which is thread-safe by design. The engine submits audio commands; kira processes them on its audio thread.


## Python GIL Strategy

The Python GIL is the most constrained resource. The strategy is to minimize GIL hold time and never hold it during heavy Rust work.

### Rules

1. **The GIL is only acquired on the main thread**, during sequential tasks. Never on rayon workers or background threads.

2. **The GIL is acquired in batches**, not per-event. When the ScriptSystem dispatches events to Python scripts, it acquires the GIL once, dispatches all pending events, then releases it. This amortizes the acquisition cost.

3. **Heavy work runs outside the GIL**. Physics stepping, transform propagation, rendering, asset decoding — all happen in Rust without the GIL. Python scripts submit commands (which are queued) rather than performing work directly.

4. **Script callbacks are fast by convention**. The scripting API encourages scripts to submit commands rather than doing computation. For example, a script handles `on_collision` by queueing a despawn command and a sound play command — it does not perform physics calculations.

### Per-Frame GIL Timeline

```
Frame start
|
+-- [Sequential: PRE_UPDATE]       no GIL (pure Rust: input polling)
+-- [Sequential: GAME_EARLY]       no GIL (pure Rust: transform propagation)
+-- [Sequential: GAME_UPDATE]      no GIL (pure Rust: physics step)
|
+-- [Sequential: GAME_UPDATE]      GIL acquired: ScriptSystem dispatches events
|                                  - on_collision handlers
|                                  - on_trigger_enter/exit handlers
|                                  - on_input_action handlers
|                                  GIL released
|
+-- [Sequential: GAME_LATE]        GIL acquired: ScriptSystem dispatches late events
|                                  GIL released
|
+-- [Sequential: RENDER_ENQUEUE]   no GIL (pure Rust: build draw commands)
+-- [Sequential: RENDER_EXECUTE]   no GIL (pure Rust: GPU commands)
+-- [Parallel phase]               no GIL (pure Rust on rayon pool)
+-- [Background phase]             no GIL (pure Rust on rayon pool)
|
Frame end
```

The GIL is held for at most two short windows per frame: event dispatch during GAME_UPDATE and GAME_LATE. All other work is pure Rust.


## Avoiding Deadlocks

The locking hierarchy (always acquire locks in this order) prevents deadlocks:

```
1. Scene (coarse lock, if needed)
2. Component stores (fine-grained, per-type)
3. Event bus
4. Module registry
5. Physics world
6. Python GIL (always last)
```

If code needs multiple locks, it always acquires them in this order. No code acquires the GIL and then tries to lock the Scene — the GIL is always the innermost lock.

The scheduler enforces additional safety: sequential tasks run one at a time on the main thread, so they naturally have exclusive access. Parallel tasks share no mutable state (they receive read-only references or work on independent data).


## Cross-Thread Communication Patterns

### Task Submission (any thread -> scheduler)
Worker threads submit tasks to the scheduler via a crossbeam MPSC channel. The main thread drains the channel at the start of each tick. This is lock-free and non-blocking for the sender.

### Background Results (background -> main)
Background tasks return results via their callback mechanism. When a background task completes, it sends the result through a channel. The scheduler picks it up and submits the callback as a sequential task.

### Render Commands (any thread -> renderer)
Draw commands are appended to a thread-local collector during parallel tasks, then merged into the back buffer. For sequential tasks, commands go directly into the back buffer.

### Audio Commands (any thread -> audio)
Audio commands are submitted through kira's built-in command channel. No additional synchronization needed.


## Performance Considerations

- **parking_lot** over std locks: parking_lot RwLock is smaller, faster, and does not poison on panic.
- **Crossbeam channels** over std mpsc: crossbeam channels are lock-free and support multiple producers natively.
- **Rayon** over manual thread pools: rayon provides work-stealing, automatic parallelism, and scoped tasks that guarantee completion before scope exit.
- **Double-buffered draw commands**: Eliminates producer/consumer contention on the render queue.
- **Batch GIL acquisition**: One GIL acquire per frame instead of per-event minimizes Python/Rust boundary overhead.


## Acceptance Tests

### T-THR-01: Sequential Tasks Run on Main Thread
Submit 10 sequential tasks. Each records its `std::thread::current().id()`.
- Expected: All 10 thread IDs are identical
- Expected: The thread ID matches the main thread's ID

### T-THR-02: Parallel Tasks Run on Multiple Threads
Submit 8 parallel tasks. Each records its thread ID.
- Expected: At least 2 distinct thread IDs are recorded (on a multi-core machine)
- Expected: None of the thread IDs is the main thread (rayon pool threads)

### T-THR-03: Background Tasks Run Off Main Thread
Submit a background task that records its thread ID.
- Expected: The thread ID is NOT the main thread
- Expected: The thread is from the rayon pool

### T-THR-04: Scene Read Concurrency
From 4 parallel tasks, simultaneously read TransformComponents for different entities. No writes during this window.
- Expected: All 4 reads succeed without blocking each other
- Expected: No deadlock or lock contention (verified by completion within 10ms)

### T-THR-05: Scene Write Exclusivity
From a sequential task, write to TransformComponents (drain commands). Verify no other task can read components during the write.
- Expected: The write completes without error
- Expected: No data race detected (run under ThreadSanitizer or Miri if possible)

### T-THR-06: Fine-Grained Component Locking
From 2 sequential tasks in the same tick (different priorities): Task A reads TransformComponents, Task B reads MeshComponents.
- Expected: Both tasks can hold their respective read locks without contention
- Expected: Task A does NOT block Task B (different component stores)

### T-THR-07: Cross-Thread Task Submission — No Data Race
From 100 background tasks, each submits a sequential task via the remote channel simultaneously.
- Expected: All 100 sequential tasks appear in the next tick's sequential phase
- Expected: No lost tasks, no duplicates
- Expected: No crash, no data race

### T-THR-08: Double-Buffered Render Queue — No Contention
During RENDER_ENQUEUE, submit 10,000 draw commands to the back buffer. Simultaneously read the front buffer for rendering.
- Expected: The renderer reads last frame's commands (front buffer) cleanly
- Expected: The RENDER_ENQUEUE phase writes to the back buffer without interference
- Expected: After buffer swap, the renderer has exactly 10,000 commands

### T-THR-09: Locking Hierarchy — No Deadlock Under Stress
Run a stress test for 10 seconds: multiple parallel tasks randomly acquire Scene read locks, component store locks, and event bus locks, always in hierarchy order.
- Expected: No deadlock occurs (all tasks complete within their timeouts)
- Expected: No thread hangs (verified by a watchdog timer)

### T-THR-10: GIL Acquired Only on Main Thread
Instrument GIL acquisition with a thread ID check. Run for 100 frames with Python scripts active.
- Expected: Every GIL acquisition occurs on the main thread
- Expected: Zero GIL acquisitions on rayon pool threads
- Expected: Zero GIL acquisitions on background threads

### T-THR-11: GIL Not Held During Physics Step
Instrument GIL and physics mutex. Run a frame with both scripting and physics active.
- Expected: The physics mutex is held while the GIL is NOT held
- Expected: The GIL is held while the physics mutex is NOT held
- Expected: The two are never held simultaneously on the same thread

### T-THR-12: GIL Not Held During Rendering
Instrument GIL acquisition. Run a frame with scripts and rendering active.
- Expected: During RENDER_EXECUTE, the GIL is not held
- Expected: During RENDER_ENQUEUE, the GIL is not held

### T-THR-13: GIL Batch Efficiency
Emit 100 events that dispatch to Python handlers. Count GIL acquire/release pairs.
- Expected: GIL is acquired at most 2 times per frame (not 100 times)
- Expected: Total GIL hold time is under 5ms for 100 trivial handlers

### T-THR-14: Module Registry Concurrent Read
From 8 parallel tasks, each calls `registry.get::<RendererModule>()` simultaneously.
- Expected: All 8 reads return the same module reference
- Expected: No contention delay measurable (reads should be sub-microsecond)

### T-THR-15: Audio Command Thread Safety
From 5 different threads (mix of sequential, parallel, background), submit audio play commands simultaneously.
- Expected: All commands are received by the kira audio engine
- Expected: No crash, no data race, no lost commands

### T-THR-16: Rayon Pool Size Configuration
Set parallel_threads=2 in config. Submit 4 parallel tasks that each sleep 100ms.
- Expected: Total parallel phase time is approximately 200ms (2 batches of 2 on 2 threads)
- Expected: Not 100ms (would mean 4 threads) and not 400ms (would mean 1 thread)

### T-THR-17: ThreadSanitizer Clean Run
Compile the engine with ThreadSanitizer enabled. Run a full integration test (100 frames, all modules active, Python scripts dispatching events).
- Expected: Zero data race reports
- Expected: Zero lock-order-inversion reports

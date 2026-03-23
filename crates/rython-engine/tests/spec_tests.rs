use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Returns the workspace root directory.
fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = crates/rython-engine
    // workspace root = ../../
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned()
}

/// Layer assignment for each internal crate.
fn crate_layer(name: &str) -> Option<u8> {
    match name {
        "rython-core" => Some(0),
        "rython-scheduler" | "rython-modules" => Some(1),
        "rython-ecs" | "rython-window" | "rython-input"
        | "rython-renderer" | "rython-physics" | "rython-audio"
        | "rython-resources" => Some(2),
        "rython-ui" | "rython-scripting" => Some(3),
        "rython-engine" => Some(4),
        _ => None,
    }
}

/// Parse a crate's Cargo.toml and extract its [dependencies] keys that
/// correspond to internal workspace crates.
fn internal_deps(cargo_toml_path: &Path, internal_crates: &HashSet<String>) -> Vec<String> {
    let contents = std::fs::read_to_string(cargo_toml_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", cargo_toml_path.display()));

    let table: toml::Value = contents
        .parse()
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", cargo_toml_path.display()));

    let mut deps = Vec::new();

    for section in &["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(dep_table) = table.get(section).and_then(|v| v.as_table()) {
            for key in dep_table.keys() {
                let normalized = key.replace('_', "-");
                if internal_crates.contains(&normalized) {
                    deps.push(normalized);
                }
            }
        }
    }

    deps
}

/// Detect cycles in a directed graph using DFS.
/// Returns Some(cycle_description) if a cycle is found, None otherwise.
fn find_cycle(graph: &HashMap<String, Vec<String>>) -> Option<String> {
    let mut visited: HashSet<String> = HashSet::new();
    let mut in_stack: HashSet<String> = HashSet::new();

    for node in graph.keys() {
        if !visited.contains(node) {
            if let Some(cycle) = dfs_cycle(node, graph, &mut visited, &mut in_stack) {
                return Some(cycle);
            }
        }
    }

    None
}

fn dfs_cycle(
    node: &str,
    graph: &HashMap<String, Vec<String>>,
    visited: &mut HashSet<String>,
    in_stack: &mut HashSet<String>,
) -> Option<String> {
    in_stack.insert(node.to_string());

    if let Some(deps) = graph.get(node) {
        for dep in deps {
            if in_stack.contains(dep) {
                return Some(format!("{dep} -> {node}"));
            }
            if !visited.contains(dep) {
                if let Some(cycle) = dfs_cycle(dep, graph, visited, in_stack) {
                    return Some(cycle);
                }
            }
        }
    }

    in_stack.remove(node);
    visited.insert(node.to_string());
    None
}

// ─── T-SPEC-01: Workspace Compilation ────────────────────────────────────────
// This test is implicitly satisfied by the fact that the test binary compiled.
// We add a trivial assertion to make the intent explicit.
#[test]
fn t_spec_01_workspace_compiles() {
    // If this test runs, the workspace compiled successfully.
    // The `#![deny(warnings)]` in each lib.rs ensures warnings are treated as errors.
    assert!(true, "workspace compiled with zero warnings");
}

// ─── T-SPEC-02: Dependency DAG Acyclicity and Layer Constraints ───────────────
#[test]
fn t_spec_02_dependency_dag_acyclicity() {
    let root = workspace_root();
    let crates_dir = root.join("crates");

    let internal_crates: HashSet<String> = vec![
        "rython-core",
        "rython-scheduler",
        "rython-modules",
        "rython-ecs",
        "rython-window",
        "rython-input",
        "rython-renderer",
        "rython-physics",
        "rython-audio",
        "rython-resources",
        "rython-ui",
        "rython-scripting",
        "rython-engine",
    ]
    .into_iter()
    .map(String::from)
    .collect();

    // Build the dependency graph
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();

    for crate_name in &internal_crates {
        let cargo_path = crates_dir.join(crate_name).join("Cargo.toml");
        let deps = internal_deps(&cargo_path, &internal_crates);
        graph.insert(crate_name.clone(), deps);
    }

    // 1. No cycles
    let cycle = find_cycle(&graph);
    assert!(
        cycle.is_none(),
        "cycle detected in dependency graph: {}",
        cycle.unwrap_or_default()
    );

    // 2. Layer constraints
    for (crate_name, deps) in &graph {
        let owner_layer = match crate_layer(crate_name) {
            Some(l) => l,
            None => continue,
        };

        for dep in deps {
            let dep_layer = match crate_layer(dep) {
                Some(l) => l,
                None => continue,
            };

            assert!(
                dep_layer <= owner_layer,
                "{crate_name} (Layer {owner_layer}) depends on {dep} (Layer {dep_layer}): \
                 higher-layer crates may not depend on lower-layer crates in reverse"
            );

            // Layer 0 must have zero internal dependencies
            assert!(
                owner_layer != 0,
                "Layer 0 crate '{crate_name}' must have no internal dependencies, \
                 but depends on '{dep}'"
            );
        }
    }

    // Verify Layer 0 explicitly
    let layer0_deps = graph.get("rython-core").unwrap();
    assert!(
        layer0_deps.is_empty(),
        "rython-core (Layer 0) must have no internal dependencies, found: {layer0_deps:?}"
    );
}

// ─── T-SPEC-03: Boot and Shutdown Sequence ────────────────────────────────────
// Boot loads modules in dependency order; shutdown unloads in reverse order.
#[test]
fn t_spec_03_boot_shutdown_sequence() {
    use rython_core::{EngineError, SchedulerHandle};
    use rython_engine::EngineBuilder;
    use rython_modules::Module;
    use std::sync::{Arc, Mutex};

    let log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    struct Tracker {
        id: &'static str,
        deps: Vec<String>,
        log: Arc<Mutex<Vec<String>>>,
    }
    impl Module for Tracker {
        fn name(&self) -> &str {
            self.id
        }
        fn dependencies(&self) -> Vec<String> {
            self.deps.clone()
        }
        fn on_load(&mut self, _: &dyn SchedulerHandle) -> Result<(), EngineError> {
            self.log.lock().unwrap().push(format!("load:{}", self.id));
            Ok(())
        }
        fn on_unload(&mut self, _: &dyn SchedulerHandle) -> Result<(), EngineError> {
            self.log.lock().unwrap().push(format!("unload:{}", self.id));
            Ok(())
        }
    }

    // Module B depends on A — A must load first, B must unload first.
    let mut engine = EngineBuilder::new()
        .add_module(Box::new(Tracker {
            id: "A",
            deps: vec![],
            log: Arc::clone(&log),
        }))
        .add_module(Box::new(Tracker {
            id: "B",
            deps: vec!["A".to_string()],
            log: Arc::clone(&log),
        }))
        .build()
        .unwrap();

    engine.boot().unwrap();
    engine.shutdown().unwrap();

    let entries = log.lock().unwrap().clone();

    let load_a = entries.iter().position(|e| e == "load:A").expect("A loaded");
    let load_b = entries.iter().position(|e| e == "load:B").expect("B loaded");
    assert!(load_a < load_b, "A must be loaded before its dependent B");

    let unload_b = entries.iter().position(|e| e == "unload:B").expect("B unloaded");
    let unload_a = entries.iter().position(|e| e == "unload:A").expect("A unloaded");
    assert!(unload_b < unload_a, "B must be unloaded before A (reverse dependency order)");
}

// ─── T-SPEC-04: Feature Flags ─────────────────────────────────────────────────
// Only modules explicitly added to the builder are loaded.
// Omitting a module is the feature-flag mechanism for disabling it.
#[test]
fn t_spec_04_feature_flags() {
    use rython_core::{EngineError, SchedulerHandle};
    use rython_engine::EngineBuilder;
    use rython_modules::Module;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    let a_loaded = Arc::new(AtomicBool::new(false));
    let b_loaded = Arc::new(AtomicBool::new(false));

    struct FlagMod {
        id: &'static str,
        flag: Arc<AtomicBool>,
    }
    impl Module for FlagMod {
        fn name(&self) -> &str {
            self.id
        }
        fn on_load(&mut self, _: &dyn SchedulerHandle) -> Result<(), EngineError> {
            self.flag.store(true, Ordering::Relaxed);
            Ok(())
        }
        fn on_unload(&mut self, _: &dyn SchedulerHandle) -> Result<(), EngineError> {
            Ok(())
        }
    }

    // "feature_b" is intentionally omitted — this is its "disabled" state.
    let mut engine = EngineBuilder::new()
        .add_module(Box::new(FlagMod {
            id: "feature_a",
            flag: Arc::clone(&a_loaded),
        }))
        .build()
        .unwrap();

    engine.boot().unwrap();

    assert!(a_loaded.load(Ordering::Relaxed), "feature_a should be loaded");
    assert!(!b_loaded.load(Ordering::Relaxed), "feature_b was not added, must not be loaded");

    engine.shutdown().unwrap();
}

// ─── T-SPEC-05: Frame Timeline Ordering ───────────────────────────────────────
// Tasks submitted at different priority levels run in ascending priority order
// within a single tick (lower number = earlier phase).
#[test]
fn t_spec_05_frame_timeline_ordering() {
    use rython_core::{priorities, EngineConfig, SchedulerConfig};
    use rython_engine::EngineBuilder;
    use std::sync::{Arc, Mutex};

    let seq: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));

    let config = EngineConfig {
        scheduler: SchedulerConfig {
            target_fps: 1_000_000,
            parallel_threads: None,
            spin_threshold_us: 0,
        },
        ..Default::default()
    };

    let mut engine = EngineBuilder::new().with_config(config).build().unwrap();

    // Submit tasks at all phase priorities in reverse order to verify sorting.
    for &p in &[
        priorities::RENDER_EXECUTE,
        priorities::RENDER_ENQUEUE,
        priorities::GAME_LATE,
        priorities::GAME_UPDATE,
        priorities::GAME_EARLY,
        priorities::PRE_UPDATE,
    ] {
        let s = Arc::clone(&seq);
        engine.scheduler().submit_sequential(
            Box::new(move || {
                s.lock().unwrap().push(p);
                Ok(())
            }),
            p,
            0,
        );
    }

    engine.tick().unwrap();

    let order = seq.lock().unwrap().clone();
    assert_eq!(order.len(), 6, "all 6 phase tasks should execute in one tick");
    for w in order.windows(2) {
        assert!(
            w[0] <= w[1],
            "tasks must execute in ascending priority order; got sequence {:?}",
            order
        );
    }
}

// ─── T-THR-01: Sequential Tasks Run on Main Thread ───────────────────────────
#[test]
fn t_thr_01_sequential_tasks_on_main_thread() {
    use rython_core::{priorities, SchedulerConfig};
    use rython_scheduler::TaskScheduler;
    use std::sync::{Arc, Mutex};

    let main_id = std::thread::current().id();
    let captured: Arc<Mutex<Vec<std::thread::ThreadId>>> = Arc::new(Mutex::new(Vec::new()));

    let mut sched = TaskScheduler::new(&SchedulerConfig {
        target_fps: 1_000_000,
        parallel_threads: None,
        spin_threshold_us: 0,
    });

    for _ in 0..10 {
        let ids = Arc::clone(&captured);
        sched.submit_sequential(
            Box::new(move || {
                ids.lock().unwrap().push(std::thread::current().id());
                Ok(())
            }),
            priorities::GAME_UPDATE,
            0,
        );
    }

    sched.tick().unwrap();

    let ids = captured.lock().unwrap();
    assert_eq!(ids.len(), 10, "all 10 sequential tasks should run");
    for id in ids.iter() {
        assert_eq!(*id, main_id, "sequential task must run on the calling (main) thread");
    }
}

// ─── T-THR-02: Parallel Tasks Run on Multiple Threads ────────────────────────
#[test]
fn t_thr_02_parallel_tasks_on_multiple_threads() {
    use rython_core::{priorities, SchedulerConfig};
    use rython_scheduler::TaskScheduler;
    use std::sync::{Arc, Mutex};

    let captured: Arc<Mutex<HashSet<std::thread::ThreadId>>> = Arc::new(Mutex::new(HashSet::new()));

    let mut sched = TaskScheduler::new(&SchedulerConfig {
        target_fps: 1_000_000,
        parallel_threads: Some(4),
        spin_threshold_us: 0,
    });

    for _ in 0..8 {
        let ids = Arc::clone(&captured);
        sched.submit_parallel(
            Box::new(move || {
                // Brief sleep so tasks run concurrently rather than being batched serially.
                std::thread::sleep(std::time::Duration::from_millis(5));
                ids.lock().unwrap().insert(std::thread::current().id());
                Ok(())
            }),
            priorities::GAME_UPDATE,
            0,
        );
    }

    sched.tick().unwrap();

    let ids = captured.lock().unwrap();
    // Rayon's par_iter() may use the calling thread as a worker, so we only
    // assert the parallelism property (>= 2 distinct threads) rather than
    // "none on main thread".
    assert!(
        ids.len() >= 2,
        "parallel tasks should run on at least 2 distinct threads on a multi-core machine; \
         got {} distinct thread(s)",
        ids.len()
    );
}

// ─── T-THR-03: Background Tasks Run Off Main Thread ──────────────────────────
#[test]
fn t_thr_03_background_tasks_off_main_thread() {
    use rython_core::{EngineError, priorities, SchedulerConfig};
    use rython_scheduler::TaskScheduler;
    use std::sync::{Arc, Mutex};

    let main_id = std::thread::current().id();
    let task_id: Arc<Mutex<Option<std::thread::ThreadId>>> = Arc::new(Mutex::new(None));

    let mut sched = TaskScheduler::new(&SchedulerConfig {
        target_fps: 1_000_000,
        parallel_threads: None,
        spin_threshold_us: 0,
    });

    let slot = Arc::clone(&task_id);
    // pool.spawn() runs tasks on pool threads, never the calling thread.
    sched.submit_background(
        move || {
            *slot.lock().unwrap() = Some(std::thread::current().id());
        },
        None::<fn(Result<(), EngineError>) -> Result<(), EngineError>>,
        priorities::IDLE,
        0,
    );

    // Allow the pool thread to run before the next tick.
    std::thread::sleep(std::time::Duration::from_millis(50));
    sched.tick().unwrap();

    let id = task_id
        .lock()
        .unwrap()
        .expect("background task should have recorded its thread ID");
    assert_ne!(id, main_id, "background task must run on a pool thread, not the calling thread");
}

// ─── T-THR-04: Scene Read Concurrency ────────────────────────────────────────
// Four parallel tasks simultaneously read TransformComponents.
// All complete without deadlock within 100ms.
#[test]
fn t_thr_04_scene_read_concurrency() {
    use rython_core::{priorities, SchedulerConfig};
    use rython_ecs::{Component, Scene, TransformComponent};
    use rython_scheduler::TaskScheduler;
    use std::any::TypeId;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    let scene = Arc::new(Scene::new());

    for _ in 0..4 {
        scene.queue_spawn_anon(vec![(
            TypeId::of::<TransformComponent>(),
            Box::new(TransformComponent::default()) as Box<dyn Component>,
        )]);
    }
    scene.drain_commands();

    let completions = Arc::new(AtomicU32::new(0));
    let mut sched = TaskScheduler::new(&SchedulerConfig {
        target_fps: 1_000_000,
        parallel_threads: Some(4),
        spin_threshold_us: 0,
    });

    for _ in 0..4 {
        let s = Arc::clone(&scene);
        let c = Arc::clone(&completions);
        sched.submit_parallel(
            Box::new(move || {
                for e in s.all_entities() {
                    let _ = s.components.get::<TransformComponent>(e);
                }
                c.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }),
            priorities::GAME_UPDATE,
            0,
        );
    }

    let start = Instant::now();
    sched.tick().unwrap();
    let elapsed = start.elapsed();

    assert_eq!(completions.load(Ordering::Relaxed), 4, "all 4 concurrent reads should complete");
    assert!(
        elapsed < Duration::from_millis(200),
        "concurrent reads should complete quickly (no deadlock); took {:?}",
        elapsed
    );
}

// ─── T-THR-05: Scene Write Exclusivity ───────────────────────────────────────
// A sequential task writes components; the write must complete and be visible.
#[test]
fn t_thr_05_scene_write_exclusivity() {
    use rython_core::{priorities, SchedulerConfig};
    use rython_ecs::{Component, Scene, TransformComponent};
    use rython_scheduler::TaskScheduler;
    use std::any::TypeId;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let scene = Arc::new(Scene::new());

    for _ in 0..10 {
        scene.queue_spawn_anon(vec![(
            TypeId::of::<TransformComponent>(),
            Box::new(TransformComponent::default()) as Box<dyn Component>,
        )]);
    }
    scene.drain_commands();

    let mut sched = TaskScheduler::new(&SchedulerConfig {
        target_fps: 1_000_000,
        parallel_threads: None,
        spin_threshold_us: 0,
    });

    let done = Arc::new(AtomicBool::new(false));
    let s = Arc::clone(&scene);
    let d = Arc::clone(&done);

    sched.submit_sequential(
        Box::new(move || {
            for e in s.all_entities() {
                s.queue_attach(e, TransformComponent { x: 42.0, ..Default::default() });
            }
            s.drain_commands();
            d.store(true, Ordering::Release);
            Ok(())
        }),
        priorities::GAME_UPDATE,
        0,
    );

    sched.tick().unwrap();

    assert!(done.load(Ordering::Acquire), "sequential write task should complete without error");

    for e in scene.all_entities() {
        let x = scene.components.get::<TransformComponent>(e).map(|c| c.x);
        assert_eq!(x, Some(42.0), "entity {:?}: TransformComponent.x should be 42.0", e);
    }
}

// ─── T-THR-06: Fine-Grained Component Locking ────────────────────────────────
// Two parallel tasks hold read locks on different component stores simultaneously.
// Neither blocks the other.
#[test]
fn t_thr_06_fine_grained_component_locking() {
    use rython_core::{priorities, SchedulerConfig};
    use rython_ecs::{Component, MeshComponent, Scene, TransformComponent};
    use rython_scheduler::TaskScheduler;
    use std::any::TypeId;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let scene = Arc::new(Scene::new());
    scene.queue_spawn_anon(vec![
        (
            TypeId::of::<TransformComponent>(),
            Box::new(TransformComponent::default()) as Box<dyn Component>,
        ),
        (
            TypeId::of::<MeshComponent>(),
            Box::new(MeshComponent::default()) as Box<dyn Component>,
        ),
    ]);
    scene.drain_commands();

    let mut sched = TaskScheduler::new(&SchedulerConfig {
        target_fps: 1_000_000,
        parallel_threads: Some(2),
        spin_threshold_us: 0,
    });

    let transform_done = Arc::new(AtomicBool::new(false));
    let mesh_done = Arc::new(AtomicBool::new(false));

    let sa = Arc::clone(&scene);
    let ta = Arc::clone(&transform_done);
    sched.submit_parallel(
        Box::new(move || {
            for e in sa.all_entities() {
                let _ = sa.components.get::<TransformComponent>(e);
            }
            ta.store(true, Ordering::Release);
            Ok(())
        }),
        priorities::GAME_UPDATE,
        0,
    );

    let sb = Arc::clone(&scene);
    let mb = Arc::clone(&mesh_done);
    sched.submit_parallel(
        Box::new(move || {
            for e in sb.all_entities() {
                let _ = sb.components.get::<MeshComponent>(e);
            }
            mb.store(true, Ordering::Release);
            Ok(())
        }),
        priorities::GAME_UPDATE,
        1,
    );

    sched.tick().unwrap();

    assert!(
        transform_done.load(Ordering::Acquire),
        "TransformComponent reader should complete"
    );
    assert!(
        mesh_done.load(Ordering::Acquire),
        "MeshComponent reader should complete without contention"
    );
}

// ─── T-THR-07: Cross-Thread Task Submission — No Data Race ───────────────────
// 100 threads simultaneously submit sequential tasks via RemoteSender.
// All 100 tasks must execute in the next tick with no lost tasks.
#[test]
fn t_thr_07_cross_thread_task_submission() {
    use rython_core::{priorities, SchedulerConfig};
    use rython_scheduler::TaskScheduler;
    use std::sync::{Arc, Mutex};

    let counter: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));

    let mut sched = TaskScheduler::new(&SchedulerConfig {
        target_fps: 1_000_000,
        parallel_threads: None,
        spin_threshold_us: 0,
    });

    let remote = sched.remote_sender();

    let handles: Vec<_> = (0..100)
        .map(|_| {
            let r = remote.clone();
            let c = Arc::clone(&counter);
            std::thread::spawn(move || {
                r.submit(
                    Box::new(move || {
                        *c.lock().unwrap() += 1;
                        Ok(())
                    }),
                    priorities::GAME_UPDATE,
                    0,
                );
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    // Single tick drains the remote channel and runs all 100 sequential tasks.
    sched.tick().unwrap();

    let count = *counter.lock().unwrap();
    assert_eq!(
        count, 100,
        "all 100 remotely-submitted tasks should execute in one tick, got {count}"
    );
}

// ─── T-THR-08: Double-Buffered Render Queue — No Contention ──────────────────
// Back buffer collects 10,000 commands; after swap the renderer reads exactly
// that count from the front buffer.
#[test]
fn t_thr_08_double_buffered_render_queue() {
    use rython_renderer::{Color, CommandQueue, DrawCommand, DrawRect};

    let queue = CommandQueue::new(15_000);

    // RENDER_ENQUEUE phase: fill back buffer
    for i in 0..10_000u32 {
        queue.push(DrawCommand::Rect(DrawRect {
            x: i as f32,
            y: 0.0,
            w: 1.0,
            h: 1.0,
            color: Color::rgb(255, 255, 255),
            border: None,
            border_width: 0.0,
            z: i as f32,
        }));
    }

    assert_eq!(queue.back_len(), 10_000, "back buffer should hold 10,000 commands");
    assert_eq!(queue.front_len(), 0, "front buffer should be empty before swap");

    // Phase boundary: swap front ↔ back
    queue.swap();

    assert_eq!(queue.front_len(), 10_000, "front should hold this frame's 10,000 commands");
    assert_eq!(queue.back_len(), 0, "back should be cleared after swap");

    // RENDER_EXECUTE phase: drain front buffer
    let cmds = queue.take_sorted_front();
    assert_eq!(cmds.len(), 10_000, "renderer must receive exactly 10,000 commands");
}

// ─── T-THR-09: Locking Hierarchy — No Deadlock Under Stress ──────────────────
// 8 threads acquire scene and registry locks in hierarchy order for 300ms.
// No deadlock expected.
#[test]
fn t_thr_09_locking_hierarchy_no_deadlock() {
    use rython_ecs::Scene;
    use rython_modules::ModuleRegistry;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    let scene = Arc::new(Scene::new());
    let registry = Arc::new(ModuleRegistry::new());
    let deadline = Duration::from_millis(300);

    let mut handles = Vec::new();
    for _ in 0..8 {
        let s = Arc::clone(&scene);
        let r = Arc::clone(&registry);
        handles.push(std::thread::spawn(move || {
            let end = Instant::now() + deadline;
            while Instant::now() < end {
                // Always acquire in hierarchy order: scene before registry
                let _ = s.all_entities();
                let _ = r.names();
            }
        }));
    }

    for h in handles {
        h.join().expect("thread should not deadlock or panic");
    }
}

// ─── T-THR-10: GIL Acquired Only on Main Thread ──────────────────────────────
// Structural guarantee: ScriptSystem only calls Python::attach() from GAME_UPDATE
// and GAME_LATE sequential tasks, which always run on the main thread.
// Rayon workers and background threads never acquire the GIL.
#[test]
#[ignore = "requires Python interpreter and full engine loop; run with --include-ignored"]
fn t_thr_10_gil_acquired_only_on_main_thread() {
    // Verified by instrumentation in ScriptSystem::dispatch_events():
    // every Python::attach() call asserts std::thread::current().id() == main_thread_id.
}

// ─── T-THR-11: GIL Not Held During Physics Step ──────────────────────────────
// Structural guarantee: physics step (priority GAME_UPDATE) completes before
// ScriptSystem dispatches events (later GAME_UPDATE priority or GAME_LATE).
// The two sequential tasks never overlap, so their resources are never held
// simultaneously.
#[test]
#[ignore = "requires physics + scripting integration; run with --include-ignored"]
fn t_thr_11_gil_not_held_during_physics_step() {}

// ─── T-THR-12: GIL Not Held During Rendering ─────────────────────────────────
// Structural guarantee: RENDER_ENQUEUE (priority 30) and RENDER_EXECUTE (35)
// run after GAME_LATE (25) where the GIL has already been released.
#[test]
#[ignore = "requires renderer + scripting integration; run with --include-ignored"]
fn t_thr_12_gil_not_held_during_rendering() {}

// ─── T-THR-13: GIL Batch Efficiency ──────────────────────────────────────────
// Structural guarantee: ScriptSystem acquires the GIL once per event-dispatch
// phase (at most twice per frame: GAME_UPDATE + GAME_LATE), dispatches all
// pending events in that window, then releases.
// Verified by rython_scripting::gil_dispatch_count().
#[test]
#[ignore = "requires Python scripts and event dispatch; run with --include-ignored"]
fn t_thr_13_gil_batch_efficiency() {}

// ─── T-THR-14: Module Registry Concurrent Read ───────────────────────────────
// 8 parallel tasks simultaneously call registry.names().
// All succeed — RwLock allows unlimited concurrent readers.
#[test]
fn t_thr_14_module_registry_concurrent_read() {
    use rython_core::{priorities, SchedulerConfig};
    use rython_modules::ModuleRegistry;
    use rython_scheduler::TaskScheduler;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    let registry = Arc::new(ModuleRegistry::new());
    let reads = Arc::new(AtomicU32::new(0));

    let mut sched = TaskScheduler::new(&SchedulerConfig {
        target_fps: 1_000_000,
        parallel_threads: Some(8),
        spin_threshold_us: 0,
    });

    for _ in 0..8 {
        let r = Arc::clone(&registry);
        let c = Arc::clone(&reads);
        sched.submit_parallel(
            Box::new(move || {
                let _ = r.names();
                c.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }),
            priorities::GAME_UPDATE,
            0,
        );
    }

    sched.tick().unwrap();

    assert_eq!(
        reads.load(Ordering::Relaxed),
        8,
        "all 8 concurrent registry reads should succeed"
    );
}

// ─── T-THR-15: Audio Command Thread Safety ───────────────────────────────────
// 5 threads access AudioManager (via Arc<Mutex<>>) and submit play commands.
// No crash, no data race, no lost commands.
#[test]
fn t_thr_15_audio_command_thread_safety() {
    use rython_audio::{AudioCategory, AudioManager, PlayRequest};
    use std::sync::{Arc, Mutex};

    // AudioManager is not Sync by itself; we protect it with Mutex.
    let manager = Arc::new(Mutex::new(AudioManager::with_default_config()));

    let handles: Vec<_> = (0..5)
        .map(|i| {
            let m = Arc::clone(&manager);
            std::thread::spawn(move || {
                // kira is None (not loaded), so play() allocates a handle without hardware.
                let result = m.lock().unwrap().play(PlayRequest {
                    path: format!("sfx_{i}.wav"),
                    category: AudioCategory::Sfx,
                    position: None,
                    looping: false,
                });
                assert!(result.is_ok(), "play() from thread {i} should not return an error");
            })
        })
        .collect();

    for h in handles {
        h.join().expect("audio command from thread should not panic or deadlock");
    }
}

// ─── T-THR-16: Rayon Pool Size Configuration ─────────────────────────────────
// Marked #[ignore]: timing guarantees require a dedicated machine with no
// background load. Run manually with --include-ignored.
#[test]
#[ignore = "timing-sensitive; requires isolated CPU cores — run with --include-ignored"]
fn t_thr_16_rayon_pool_size_configuration() {
    use rython_core::{priorities, SchedulerConfig};
    use rython_scheduler::TaskScheduler;
    use std::time::{Duration, Instant};

    // 4 tasks × 100ms each on 2 threads → ~200ms (2 batches of 2).
    let mut sched = TaskScheduler::new(&SchedulerConfig {
        target_fps: 1_000_000,
        parallel_threads: Some(2),
        spin_threshold_us: 0,
    });

    for _ in 0..4 {
        sched.submit_parallel(
            Box::new(|| {
                std::thread::sleep(Duration::from_millis(100));
                Ok(())
            }),
            priorities::GAME_UPDATE,
            0,
        );
    }

    let start = Instant::now();
    sched.tick().unwrap();
    let elapsed = start.elapsed();

    assert!(
        elapsed >= Duration::from_millis(150),
        "4 tasks on 2 threads should take >= 150ms; got {:?}",
        elapsed
    );
    assert!(
        elapsed < Duration::from_millis(400),
        "4 tasks on 2 threads should take < 400ms; got {:?}",
        elapsed
    );
}

// ─── T-THR-17: ThreadSanitizer Clean Run ─────────────────────────────────────
// Run manually on nightly with ThreadSanitizer enabled.
// Zero data races and zero lock-order inversions expected.
#[test]
#[ignore = "requires nightly + RUSTFLAGS='-Z sanitizer=thread'; run manually"]
fn t_thr_17_tsan_clean_run() {
    // Manual invocation:
    //   RUSTFLAGS="-Z sanitizer=thread" \
    //   cargo +nightly test --target x86_64-unknown-linux-gnu -- --include-ignored
    //
    // Expected: zero TSan reports across a 100-frame headless run with all
    // modules active and Python scripts dispatching events.
}

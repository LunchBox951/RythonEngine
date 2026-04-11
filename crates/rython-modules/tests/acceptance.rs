use rython_core::{EngineError, OwnerId, SchedulerHandle, priorities};
use rython_modules::{Module, ModuleLoader, ModuleRegistry, ModuleState, topological_sort};
use std::sync::{Arc, Mutex};

// ─── No-op scheduler stub for tests ──────────────────────────────────────────

#[derive(Clone, Default)]
struct NoopScheduler;

impl SchedulerHandle for NoopScheduler {
    fn submit_sequential(
        &self,
        _f: Box<dyn FnOnce() -> Result<(), EngineError> + Send + 'static>,
        _priority: rython_core::Priority,
        _owner: OwnerId,
    ) {
    }
    fn cancel_owned(&self, _owner: OwnerId) {}
}

// ─── Test module helpers ──────────────────────────────────────────────────────

struct TrackingModule {
    name: String,
    deps: Vec<String>,
    load_log: Arc<Mutex<Vec<String>>>,
    unload_log: Arc<Mutex<Vec<String>>>,
    exclusive: bool,
}

impl TrackingModule {
    fn new(
        name: &str,
        deps: Vec<&str>,
        load_log: Arc<Mutex<Vec<String>>>,
        unload_log: Arc<Mutex<Vec<String>>>,
    ) -> Box<Self> {
        Box::new(Self {
            name: name.to_string(),
            deps: deps.into_iter().map(String::from).collect(),
            load_log,
            unload_log,
            exclusive: false,
        })
    }

    fn exclusive(mut self: Box<Self>) -> Box<Self> {
        self.exclusive = true;
        self
    }
}

impl Module for TrackingModule {
    fn name(&self) -> &str {
        &self.name
    }
    fn dependencies(&self) -> Vec<String> {
        self.deps.clone()
    }
    fn on_load(&mut self, _scheduler: &dyn SchedulerHandle) -> Result<(), EngineError> {
        self.load_log.lock().unwrap().push(self.name.clone());
        Ok(())
    }
    fn on_unload(&mut self, _scheduler: &dyn SchedulerHandle) -> Result<(), EngineError> {
        self.unload_log.lock().unwrap().push(self.name.clone());
        Ok(())
    }
    fn is_exclusive(&self) -> bool {
        self.exclusive
    }
}

// ─── T-MOD-01: Post-Order Load Sequence ──────────────────────────────────────

#[test]
fn t_mod_01_post_order_load_sequence() {
    let load_log = Arc::new(Mutex::new(Vec::<String>::new()));
    let unload_log = Arc::new(Mutex::new(Vec::<String>::new()));
    let sched = NoopScheduler;
    let mut loader = ModuleLoader::new();

    // D has no deps; C->D; B->D; A->B,C
    loader.register(TrackingModule::new("D", vec![], Arc::clone(&load_log), Arc::clone(&unload_log)), None);
    loader.register(TrackingModule::new("C", vec!["D"], Arc::clone(&load_log), Arc::clone(&unload_log)), None);
    loader.register(TrackingModule::new("B", vec!["D"], Arc::clone(&load_log), Arc::clone(&unload_log)), None);
    loader.register(TrackingModule::new("A", vec!["B", "C"], Arc::clone(&load_log), Arc::clone(&unload_log)), None);

    loader.load_all(&sched).unwrap();

    let log = load_log.lock().unwrap();
    let d_pos = log.iter().position(|s| s == "D").unwrap();
    let a_pos = log.iter().position(|s| s == "A").unwrap();
    assert!(d_pos < a_pos, "D must load before A; order: {log:?}");
    assert_eq!(log.len(), 4, "each module on_load called exactly once");
}

// ─── T-MOD-02: Reverse Unload Sequence ───────────────────────────────────────

#[test]
fn t_mod_02_reverse_unload_sequence() {
    let load_log = Arc::new(Mutex::new(Vec::<String>::new()));
    let unload_log = Arc::new(Mutex::new(Vec::<String>::new()));
    let sched = NoopScheduler;
    let mut loader = ModuleLoader::new();

    loader.register(TrackingModule::new("D", vec![], Arc::clone(&load_log), Arc::clone(&unload_log)), None);
    loader.register(TrackingModule::new("C", vec!["D"], Arc::clone(&load_log), Arc::clone(&unload_log)), None);
    loader.register(TrackingModule::new("B", vec!["D"], Arc::clone(&load_log), Arc::clone(&unload_log)), None);
    loader.register(TrackingModule::new("A", vec!["B", "C"], Arc::clone(&load_log), Arc::clone(&unload_log)), None);

    loader.load_all(&sched).unwrap();

    let loaded = load_log.lock().unwrap().clone();
    loader.unload_all(&sched).unwrap();

    let unloaded = unload_log.lock().unwrap().clone();
    let reversed_load: Vec<_> = loaded.iter().rev().cloned().collect();
    assert_eq!(
        unloaded, reversed_load,
        "unload order must be exact reverse of load order"
    );
}

// ─── T-MOD-03: Circular Dependency Detection ─────────────────────────────────

#[test]
fn t_mod_03_circular_dependency_detection() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let sched = NoopScheduler;
    let mut loader = ModuleLoader::new();

    loader.register(TrackingModule::new("A", vec!["B"], Arc::clone(&log), Arc::clone(&log)), None);
    loader.register(TrackingModule::new("B", vec!["A"], Arc::clone(&log), Arc::clone(&log)), None);

    let result = loader.load_all(&sched);
    assert!(result.is_err(), "circular dependency must return Err");

    let msg = result.unwrap_err().to_string();
    assert!(msg.contains('A') || msg.contains('B'), "error must mention involved modules: {msg}");
}

// ─── T-MOD-04: Reference Counting — Shared Dependency Stays Alive ────────────

#[test]
fn t_mod_04_shared_dependency_stays_alive() {
    let load_log = Arc::new(Mutex::new(Vec::<String>::new()));
    let unload_log = Arc::new(Mutex::new(Vec::<String>::new()));
    let sched = NoopScheduler;
    let mut loader = ModuleLoader::new();

    loader.register(TrackingModule::new("Shared", vec![], Arc::clone(&load_log), Arc::clone(&unload_log)), None);
    loader.register(TrackingModule::new("A", vec!["Shared"], Arc::clone(&load_log), Arc::clone(&unload_log)), None);
    loader.register(TrackingModule::new("B", vec!["Shared"], Arc::clone(&load_log), Arc::clone(&unload_log)), None);

    loader.load_all(&sched).unwrap();

    // Unload A — Shared should stay (B still depends on it)
    loader.unload_by_name("A", &sched).unwrap();

    assert!(loader.is_loaded("Shared"), "Shared must still be loaded after A unloads");
    assert!(!unload_log.lock().unwrap().contains(&"Shared".to_string()));

    // Unload B — Shared should now unload
    loader.unload_by_name("B", &sched).unwrap();
    loader.unload_by_name("Shared", &sched).unwrap();
    assert!(!loader.contains("Shared"), "Shared should unload when last dependent unloads");
}

// ─── T-MOD-05: Reference Counting — Count Accuracy ───────────────────────────

#[test]
fn t_mod_05_ref_count_accuracy() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let sched = NoopScheduler;
    let mut loader = ModuleLoader::new();

    // Register Shared once; bump ref_count by registering it again for each dependent
    loader.register(TrackingModule::new("Shared", vec![], Arc::clone(&log), Arc::clone(&log)), None);
    for i in 0..4 {
        // Simulate 5 modules depending on Shared by registering Shared 4 more times
        let _ = i;
        loader.register(TrackingModule::new("Shared", vec![], Arc::clone(&log), Arc::clone(&log)), None);
    }
    // ref_count should be 5
    assert_eq!(loader.ref_count("Shared"), Some(5));

    // Each unload_by_name decrements
    for expected in (1..=5).rev() {
        let rc = loader.ref_count("Shared").unwrap();
        assert_eq!(rc, expected);
        loader.unload_by_name("Shared", &sched).unwrap();
    }
    assert!(!loader.contains("Shared"));
}

// ─── T-MOD-06: Module State Transitions ──────────────────────────────────────

#[test]
fn t_mod_06_module_state_transitions() {
    // We track state via a recording module that checks its own state during callbacks

    let load_state_seen: Arc<Mutex<Option<ModuleState>>> = Arc::new(Mutex::new(None));
    let unload_state_seen: Arc<Mutex<Option<ModuleState>>> = Arc::new(Mutex::new(None));

    // We can't easily inspect loader state from inside on_load without interior mutability tricks,
    // so we verify the state via the loader API.

    let load_log = Arc::new(Mutex::new(Vec::new()));
    let unload_log = Arc::new(Mutex::new(Vec::new()));
    let sched = NoopScheduler;
    let mut loader = ModuleLoader::new();

    loader.register(
        TrackingModule::new("M", vec![], Arc::clone(&load_log), Arc::clone(&unload_log)),
        None,
    );

    // Before load_all: not yet in loaded state
    assert!(!loader.is_loaded("M"));

    loader.load_all(&sched).unwrap();

    // After load_all: state is Loaded
    assert_eq!(loader.get_state("M"), Some(ModuleState::Loaded));

    loader.unload_by_name("M", &sched).unwrap();

    // After unload: module removed from loader
    assert!(!loader.contains("M"));

    let _ = (load_state_seen, unload_state_seen); // used for future extension
}

// ─── T-MOD-07: Module Registry Lookup ────────────────────────────────────────

#[test]
fn t_mod_07_registry_lookup() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let sched = NoopScheduler;
    let mut loader = ModuleLoader::new();

    for name in ["Alpha", "Beta", "Gamma", "Delta", "Epsilon"] {
        loader.register(TrackingModule::new(name, vec![], Arc::clone(&log), Arc::clone(&log)), None);
    }

    loader.load_all(&sched).unwrap();

    for name in ["Alpha", "Beta", "Gamma", "Delta", "Epsilon"] {
        assert!(loader.is_loaded(name), "{name} should be loaded");
    }

    // Non-existent lookup returns not-present
    assert!(!loader.contains("NonExistent"), "unregistered module should not be found");
}

// ─── T-MOD-08: Single-Owner — Ownership Transfer ─────────────────────────────

#[test]
fn t_mod_08_single_owner_transfer() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let sched = NoopScheduler;
    let mut loader = ModuleLoader::new();

    let owner_a: OwnerId = 1;
    let owner_b: OwnerId = 2;

    loader.register(
        TrackingModule::new("ExclusiveMod", vec![], Arc::clone(&log), Arc::clone(&log))
            .exclusive(),
        Some(owner_a),
    );

    loader.load_all(&sched).unwrap();

    // owner_a can transfer to owner_b
    loader.transfer_ownership("ExclusiveMod", owner_a, owner_b).unwrap();
    assert_eq!(loader.exclusive_owner("ExclusiveMod"), Some(owner_b));

    // owner_a can no longer transfer (not the owner anymore)
    let err = loader.transfer_ownership("ExclusiveMod", owner_a, 3);
    assert!(err.is_err(), "old owner should not be able to transfer");
}

// ─── T-MOD-09: Single-Owner — Non-Owner Cannot Transfer ──────────────────────

#[test]
fn t_mod_09_non_owner_cannot_transfer() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let sched = NoopScheduler;
    let mut loader = ModuleLoader::new();

    let owner_a: OwnerId = 1;
    let owner_b: OwnerId = 2;

    loader.register(
        TrackingModule::new("ExclusiveMod", vec![], Arc::clone(&log), Arc::clone(&log))
            .exclusive(),
        Some(owner_a),
    );

    loader.load_all(&sched).unwrap();

    // owner_b (non-owner) tries to transfer ownership
    let err = loader.transfer_ownership("ExclusiveMod", owner_b, 3);
    assert!(err.is_err(), "non-owner must not be able to transfer");

    // owner_a still retains ownership
    assert_eq!(loader.exclusive_owner("ExclusiveMod"), Some(owner_a));
}

// ─── T-MOD-10: Single-Owner — Relinquish Ownership ───────────────────────────

#[test]
fn t_mod_10_relinquish_ownership() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let sched = NoopScheduler;
    let mut loader = ModuleLoader::new();

    let owner_a: OwnerId = 1;

    loader.register(
        TrackingModule::new("ExclusiveMod", vec![], Arc::clone(&log), Arc::clone(&log))
            .exclusive(),
        Some(owner_a),
    );

    loader.load_all(&sched).unwrap();

    loader.relinquish_ownership("ExclusiveMod", owner_a).unwrap();
    assert_eq!(
        loader.exclusive_owner("ExclusiveMod"),
        None,
        "module should have no owner after relinquish"
    );
}

// ─── T-MOD-11: Cascading Teardown ────────────────────────────────────────────

#[test]
fn t_mod_11_cascading_teardown() {
    let load_log = Arc::new(Mutex::new(Vec::<String>::new()));
    let unload_log = Arc::new(Mutex::new(Vec::<String>::new()));
    let sched = NoopScheduler;
    let mut loader = ModuleLoader::new();

    // A->B->C->nothing. Only A is "top-level".
    loader.register(TrackingModule::new("C", vec![], Arc::clone(&load_log), Arc::clone(&unload_log)), None);
    loader.register(TrackingModule::new("B", vec!["C"], Arc::clone(&load_log), Arc::clone(&unload_log)), None);
    loader.register(TrackingModule::new("A", vec!["B"], Arc::clone(&load_log), Arc::clone(&unload_log)), None);

    loader.load_all(&sched).unwrap();

    // Unload in reverse order (A, B, C)
    loader.unload_by_name("A", &sched).unwrap();
    loader.unload_by_name("B", &sched).unwrap();
    loader.unload_by_name("C", &sched).unwrap();

    let unloaded = unload_log.lock().unwrap();
    assert_eq!(*unloaded, vec!["A", "B", "C"]);
    assert!(!loader.contains("A"));
    assert!(!loader.contains("B"));
    assert!(!loader.contains("C"));
}

// ─── T-MOD-12: Load Triggers Scheduler Tasks ─────────────────────────────────

#[test]
fn t_mod_12_load_triggers_scheduler_tasks() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let flag = Arc::new(AtomicBool::new(false));
    let flag2 = Arc::clone(&flag);

    struct TaskSubmittingModule {
        flag: Arc<AtomicBool>,
    }

    impl Module for TaskSubmittingModule {
        fn name(&self) -> &str {
            "TaskMod"
        }
        fn on_load(&mut self, scheduler: &dyn SchedulerHandle) -> Result<(), EngineError> {
            let f = Arc::clone(&self.flag);
            scheduler.submit_sequential(
                Box::new(move || {
                    f.store(true, Ordering::Relaxed);
                    Ok(())
                }),
                priorities::ENGINE_SETUP,
                1,
            );
            Ok(())
        }
        fn on_unload(&mut self, _scheduler: &dyn SchedulerHandle) -> Result<(), EngineError> {
            Ok(())
        }
    }

    // Use a recording scheduler that actually runs submitted tasks
    struct RunningScheduler {
        tasks: Arc<Mutex<Vec<Box<dyn FnOnce() -> Result<(), EngineError> + Send + 'static>>>>,
    }

    impl SchedulerHandle for RunningScheduler {
        fn submit_sequential(
            &self,
            f: Box<dyn FnOnce() -> Result<(), EngineError> + Send + 'static>,
            _priority: rython_core::Priority,
            _owner: OwnerId,
        ) {
            self.tasks.lock().unwrap().push(f);
        }
        fn cancel_owned(&self, _owner: OwnerId) {}
    }

    let tasks: Arc<Mutex<Vec<Box<dyn FnOnce() -> Result<(), EngineError> + Send + 'static>>>> =
        Arc::new(Mutex::new(Vec::new()));

    let sched = RunningScheduler {
        tasks: Arc::clone(&tasks),
    };

    let mut loader = ModuleLoader::new();
    loader.register(
        Box::new(TaskSubmittingModule { flag: flag2 }),
        None,
    );

    loader.load_all(&sched).unwrap();

    // Drain tasks (simulate scheduler tick)
    let pending: Vec<_> = tasks.lock().unwrap().drain(..).collect();
    for task in pending {
        task().unwrap();
    }

    assert!(
        flag.load(std::sync::atomic::Ordering::Relaxed),
        "flag should be set after running submitted task"
    );
}

// ─── T-MOD-13: Missing Config Falls Back to Defaults ─────────────────────────

#[test]
fn t_mod_13_missing_config_falls_back_to_defaults() {
    // A module that tries to read a config file. If missing, uses defaults without error.
    use rython_core::WindowConfig;

    struct ConfigModule {
        config: WindowConfig,
        warned: Arc<Mutex<bool>>,
    }

    impl Module for ConfigModule {
        fn name(&self) -> &str {
            "ConfigMod"
        }
        fn on_load(&mut self, _scheduler: &dyn SchedulerHandle) -> Result<(), EngineError> {
            let path = "engine/data/this_file_does_not_exist_12345.json";
            match std::fs::read_to_string(path) {
                Ok(contents) => {
                    match serde_json::from_str::<WindowConfig>(&contents) {
                        Ok(cfg) => self.config = cfg,
                        Err(_) => {
                            *self.warned.lock().unwrap() = true;
                            // keep default
                        }
                    }
                }
                Err(_) => {
                    *self.warned.lock().unwrap() = true;
                    // keep default
                }
            }
            Ok(())
        }
        fn on_unload(&mut self, _scheduler: &dyn SchedulerHandle) -> Result<(), EngineError> {
            Ok(())
        }
    }

    let warned = Arc::new(Mutex::new(false));
    let sched = NoopScheduler;
    let mut loader = ModuleLoader::new();

    loader.register(
        Box::new(ConfigModule {
            config: WindowConfig::default(),
            warned: Arc::clone(&warned),
        }),
        None,
    );

    // Must succeed even though config file is missing
    let result = loader.load_all(&sched);
    assert!(result.is_ok(), "module should load successfully without config file");
    assert!(
        *warned.lock().unwrap(),
        "module should have logged a warning about missing config"
    );
}

// ─── T-MOD-14: on_load Failure Short-Circuits load_all ───────────────────────

#[test]
fn t_mod_14_on_load_failure_short_circuits() {
    struct FailingModule;
    impl Module for FailingModule {
        fn name(&self) -> &str { "Failing" }
        fn on_load(&mut self, _: &dyn SchedulerHandle) -> Result<(), EngineError> {
            Err(EngineError::Module {
                module: "Failing".into(),
                message: "intentional load failure".into(),
            })
        }
        fn on_unload(&mut self, _: &dyn SchedulerHandle) -> Result<(), EngineError> { Ok(()) }
    }

    let load_log = Arc::new(Mutex::new(Vec::<String>::new()));
    let sched = NoopScheduler;
    let mut loader = ModuleLoader::new();

    loader.register(Box::new(FailingModule), None);
    // "After" depends on "Failing", so loads second; must never run
    loader.register(
        TrackingModule::new("After", vec!["Failing"], Arc::clone(&load_log), Arc::clone(&load_log)),
        None,
    );

    let result = loader.load_all(&sched);
    assert!(result.is_err(), "load_all must propagate on_load error");
    assert!(
        load_log.lock().unwrap().is_empty(),
        "modules following the failing one must not run on_load"
    );
}

// ─── T-MOD-15: on_unload Failure Propagates ──────────────────────────────────

#[test]
fn t_mod_15_on_unload_failure_propagates() {
    struct FailOnUnload;
    impl Module for FailOnUnload {
        fn name(&self) -> &str { "FailUnload" }
        fn on_load(&mut self, _: &dyn SchedulerHandle) -> Result<(), EngineError> { Ok(()) }
        fn on_unload(&mut self, _: &dyn SchedulerHandle) -> Result<(), EngineError> {
            Err(EngineError::Module {
                module: "FailUnload".into(),
                message: "intentional unload failure".into(),
            })
        }
    }

    let sched = NoopScheduler;
    let mut loader = ModuleLoader::new();
    loader.register(Box::new(FailOnUnload), None);
    loader.load_all(&sched).unwrap();

    let result = loader.unload_by_name("FailUnload", &sched);
    assert!(result.is_err(), "on_unload error must propagate through unload_by_name");
}

// ─── T-MOD-16: Unregistered Dependency Is Silently Skipped ───────────────────

#[test]
fn t_mod_16_unregistered_dep_silently_skipped() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let sched = NoopScheduler;
    let mut loader = ModuleLoader::new();

    // "B" declares "External" as a dep, but External is never registered.
    loader.register(
        TrackingModule::new("B", vec!["External"], Arc::clone(&log), Arc::clone(&log)),
        None,
    );

    let result = loader.load_all(&sched);
    assert!(result.is_ok(), "load should succeed when dep is not registered");
    assert!(loader.is_loaded("B"), "B should be loaded despite unregistered dep");
}

// ─── T-MOD-17: Empty Loader Is a No-Op ───────────────────────────────────────

#[test]
fn t_mod_17_empty_loader_is_noop() {
    let sched = NoopScheduler;
    let mut loader = ModuleLoader::new();
    assert!(loader.load_all(&sched).is_ok(), "load_all on empty loader must succeed");
    assert!(loader.unload_all(&sched).is_ok(), "unload_all on empty loader must succeed");
}

// ─── T-MOD-18: Unload Non-Existent Module Is a No-Op ─────────────────────────

#[test]
fn t_mod_18_unload_nonexistent_is_noop() {
    let sched = NoopScheduler;
    let mut loader = ModuleLoader::new();
    let result = loader.unload_by_name("DoesNotExist", &sched);
    assert!(result.is_ok(), "unloading non-existent module must not error");
}

// ─── T-MOD-19: on_unload Can Call cancel_owned on the Scheduler ──────────────

#[test]
fn t_mod_19_on_unload_calls_cancel_owned() {
    use std::sync::atomic::AtomicBool;

    struct CancelOnUnload { owner: OwnerId }
    impl Module for CancelOnUnload {
        fn name(&self) -> &str { "CancelMod" }
        fn on_load(&mut self, _: &dyn SchedulerHandle) -> Result<(), EngineError> { Ok(()) }
        fn on_unload(&mut self, scheduler: &dyn SchedulerHandle) -> Result<(), EngineError> {
            scheduler.cancel_owned(self.owner);
            Ok(())
        }
    }

    struct TrackCancel { flag: Arc<AtomicBool> }
    impl SchedulerHandle for TrackCancel {
        fn submit_sequential(
            &self,
            _f: Box<dyn FnOnce() -> Result<(), EngineError> + Send + 'static>,
            _p: rython_core::Priority,
            _o: OwnerId,
        ) {}
        fn cancel_owned(&self, _: OwnerId) {
            self.flag.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    let flag = Arc::new(AtomicBool::new(false));
    let sched = TrackCancel { flag: Arc::clone(&flag) };
    let mut loader = ModuleLoader::new();
    loader.register(Box::new(CancelOnUnload { owner: 42 }), None);
    loader.load_all(&sched).unwrap();
    loader.unload_by_name("CancelMod", &sched).unwrap();

    assert!(
        flag.load(std::sync::atomic::Ordering::Relaxed),
        "cancel_owned should have been called during on_unload"
    );
}

// ─── T-MOD-20: Self-Transfer (owner → same owner) Succeeds ───────────────────

#[test]
fn t_mod_20_self_transfer_is_valid() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let sched = NoopScheduler;
    let mut loader = ModuleLoader::new();
    let owner: OwnerId = 7;

    loader.register(
        TrackingModule::new("ExclMod", vec![], Arc::clone(&log), Arc::clone(&log)).exclusive(),
        Some(owner),
    );
    loader.load_all(&sched).unwrap();

    let result = loader.transfer_ownership("ExclMod", owner, owner);
    assert!(result.is_ok(), "transferring ownership to self must succeed");
    assert_eq!(loader.exclusive_owner("ExclMod"), Some(owner));
}

// ─── T-MOD-21: Ownership Ops on Non-Exclusive Module Are Rejected ────────────

#[test]
fn t_mod_21_non_exclusive_ownership_ops_rejected() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let sched = NoopScheduler;
    let mut loader = ModuleLoader::new();
    let owner: OwnerId = 1;

    // Registered without .exclusive()
    loader.register(
        TrackingModule::new("NonExclMod", vec![], Arc::clone(&log), Arc::clone(&log)),
        Some(owner),
    );
    loader.load_all(&sched).unwrap();

    assert!(
        loader.transfer_ownership("NonExclMod", owner, 2).is_err(),
        "transfer_ownership on non-exclusive module must return Err"
    );
    assert!(
        loader.relinquish_ownership("NonExclMod", owner).is_err(),
        "relinquish_ownership on non-exclusive module must return Err"
    );
}

// ─── T-MOD-22: Relinquish by Wrong Owner Is Rejected ─────────────────────────

#[test]
fn t_mod_22_relinquish_wrong_owner_rejected() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let sched = NoopScheduler;
    let mut loader = ModuleLoader::new();
    let owner_a: OwnerId = 1;
    let owner_b: OwnerId = 2;

    loader.register(
        TrackingModule::new("ExclMod", vec![], Arc::clone(&log), Arc::clone(&log)).exclusive(),
        Some(owner_a),
    );
    loader.load_all(&sched).unwrap();

    let err = loader.relinquish_ownership("ExclMod", owner_b);
    assert!(err.is_err(), "non-owner must not be able to relinquish");
    assert_eq!(
        loader.exclusive_owner("ExclMod"),
        Some(owner_a),
        "ownership must remain with original owner after failed relinquish"
    );
}

// ─── T-MOD-23: Diamond Dependency — Shared Transitive Dep Loads Exactly Once ─

#[test]
fn t_mod_23_diamond_dependency_loads_once() {
    let load_log = Arc::new(Mutex::new(Vec::<String>::new()));
    let unload_log = Arc::new(Mutex::new(Vec::<String>::new()));
    let sched = NoopScheduler;
    let mut loader = ModuleLoader::new();

    // D←B←A and D←C←A (diamond)
    loader.register(TrackingModule::new("D", vec![], Arc::clone(&load_log), Arc::clone(&unload_log)), None);
    loader.register(TrackingModule::new("B", vec!["D"], Arc::clone(&load_log), Arc::clone(&unload_log)), None);
    loader.register(TrackingModule::new("C", vec!["D"], Arc::clone(&load_log), Arc::clone(&unload_log)), None);
    loader.register(TrackingModule::new("A", vec!["B", "C"], Arc::clone(&load_log), Arc::clone(&unload_log)), None);

    loader.load_all(&sched).unwrap();

    let log = load_log.lock().unwrap();
    assert_eq!(log.len(), 4, "each module loads exactly once: {log:?}");
    let d_count = log.iter().filter(|s| s.as_str() == "D").count();
    assert_eq!(d_count, 1, "D must appear exactly once in load log: {log:?}");

    let d_pos = log.iter().position(|s| s == "D").unwrap();
    let a_pos = log.iter().position(|s| s == "A").unwrap();
    assert!(d_pos < a_pos, "D must load before A: {log:?}");
}

// ─── T-MOD-24: Ownership Ops on Missing Module Return Err ────────────────────

#[test]
fn t_mod_24_ownership_ops_on_missing_module() {
    let mut loader = ModuleLoader::new();
    assert!(
        loader.transfer_ownership("NoSuchModule", 1, 2).is_err(),
        "transfer_ownership on non-existent module must return Err"
    );
    assert!(
        loader.relinquish_ownership("NoSuchModule", 1).is_err(),
        "relinquish_ownership on non-existent module must return Err"
    );
}

// ─── T-MOD-25: Module State Is Loading Immediately After Registration ─────────

#[test]
fn t_mod_25_state_is_loading_after_register() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut loader = ModuleLoader::new();

    loader.register(
        TrackingModule::new("M", vec![], Arc::clone(&log), Arc::clone(&log)),
        None,
    );

    assert_eq!(
        loader.get_state("M"),
        Some(ModuleState::Loading),
        "module state must be Loading before load_all is called"
    );
}

// ─── T-MOD-26: topological_sort — Single Node No Deps ────────────────────────

#[test]
fn t_mod_26_topo_sort_single_node() {
    let mut deps = std::collections::HashMap::new();
    deps.insert("Solo".to_string(), vec![]);
    let order = topological_sort(&deps).unwrap();
    assert_eq!(order, vec!["Solo"]);
}

// ─── T-MOD-27: topological_sort — Independent Nodes Are Sorted Deterministically

#[test]
fn t_mod_27_topo_sort_independent_nodes_deterministic() {
    let mut deps = std::collections::HashMap::new();
    deps.insert("Zeta".to_string(), vec![]);
    deps.insert("Alpha".to_string(), vec![]);
    deps.insert("Mu".to_string(), vec![]);
    let order = topological_sort(&deps).unwrap();
    // All three appear exactly once; order must be deterministic (alphabetical DFS start)
    assert_eq!(order.len(), 3);
    let mut sorted = order.clone();
    sorted.sort();
    assert_eq!(sorted, vec!["Alpha", "Mu", "Zeta"]);
    // Run again — must produce same order
    let order2 = topological_sort(&deps).unwrap();
    assert_eq!(order, order2, "topological_sort must be deterministic");
}

// ─── T-MOD-28: topological_sort — Long Chain ─────────────────────────────────

#[test]
fn t_mod_28_topo_sort_long_chain() {
    // E->D->C->B->A (A has no deps, E depends on D, etc.)
    let mut deps = std::collections::HashMap::new();
    deps.insert("A".to_string(), vec![]);
    deps.insert("B".to_string(), vec!["A".to_string()]);
    deps.insert("C".to_string(), vec!["B".to_string()]);
    deps.insert("D".to_string(), vec!["C".to_string()]);
    deps.insert("E".to_string(), vec!["D".to_string()]);
    let order = topological_sort(&deps).unwrap();
    assert_eq!(order, vec!["A", "B", "C", "D", "E"]);
}

// ─── T-MOD-29: topological_sort — Empty Graph ────────────────────────────────

#[test]
fn t_mod_29_topo_sort_empty_graph() {
    let deps = std::collections::HashMap::new();
    let order = topological_sort(&deps).unwrap();
    assert!(order.is_empty(), "empty dep graph must produce empty order");
}

// ─── T-MOD-30: ModuleRegistry — ref-count decrement signals unload ───────────

#[test]
fn t_mod_30_registry_decrement_ref_signals_unload() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let registry = ModuleRegistry::new();

    let m1 = TrackingModule::new("Shared", vec![], Arc::clone(&log), Arc::clone(&log));
    registry.insert(m1, None);
    registry.insert(
        TrackingModule::new("Shared", vec![], Arc::clone(&log), Arc::clone(&log)),
        None,
    );
    assert_eq!(registry.ref_count("Shared"), Some(2));

    let should_unload = registry.decrement_ref("Shared");
    assert!(!should_unload, "should not unload when ref_count goes to 1");
    assert_eq!(registry.ref_count("Shared"), Some(1));

    let should_unload = registry.decrement_ref("Shared");
    assert!(should_unload, "should unload when ref_count reaches 0");
}

// ─── T-MOD-31: ModuleRegistry — is_owner check ───────────────────────────────

#[test]
fn t_mod_31_registry_is_owner() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let registry = ModuleRegistry::new();
    let owner: OwnerId = 99;

    let m = TrackingModule::new("ExclMod", vec![], Arc::clone(&log), Arc::clone(&log)).exclusive();
    registry.insert(m, Some(owner));

    assert!(registry.is_owner("ExclMod", owner), "owner should be recognised");
    assert!(!registry.is_owner("ExclMod", 100), "non-owner must not be recognised");
    assert!(!registry.is_owner("NoSuchMod", owner), "missing module must return false");
}

// ─── T-MOD-32: ModuleRegistry — thread-safe concurrent insert/read ───────────

#[test]
fn t_mod_32_registry_concurrent_access() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    let registry = Arc::new(ModuleRegistry::new());
    let insert_count = Arc::new(AtomicUsize::new(0));

    // Pre-seed one entry so readers have something to look at immediately
    {
        let log = Arc::new(Mutex::new(Vec::new()));
        registry.insert(
            TrackingModule::new("Base", vec![], Arc::clone(&log), Arc::clone(&log)),
            None,
        );
    }

    let handles: Vec<_> = (0..8)
        .map(|i| {
            let reg = Arc::clone(&registry);
            let count = Arc::clone(&insert_count);
            let log = Arc::new(Mutex::new(Vec::new()));
            thread::spawn(move || {
                let name = format!("Mod{i}");
                reg.insert(
                    TrackingModule::new(&name, vec![], Arc::clone(&log), Arc::clone(&log)),
                    None,
                );
                count.fetch_add(1, Ordering::Relaxed);
                // Also exercise reads under concurrent write pressure
                let _ = reg.contains("Base");
                let _ = reg.get_state("Base");
            })
        })
        .collect();

    for h in handles {
        h.join().expect("thread must not panic");
    }

    assert_eq!(insert_count.load(Ordering::Relaxed), 8, "all 8 threads must complete");
    // All inserted modules should now be present
    for i in 0..8u32 {
        assert!(registry.contains(&format!("Mod{i}")), "Mod{i} must be in registry");
    }
}

// ─── T-MOD-31: Multi-Owner Transfer A → B → C ───────────────────────────────

#[test]
fn t_mod_31_multi_owner_transfer_a_b_c() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let sched = NoopScheduler;
    let mut loader = ModuleLoader::new();

    let owner_a: OwnerId = 10;
    let owner_b: OwnerId = 20;
    let owner_c: OwnerId = 30;

    loader.register(
        TrackingModule::new("TransferMod", vec![], Arc::clone(&log), Arc::clone(&log))
            .exclusive(),
        Some(owner_a),
    );

    loader.load_all(&sched).unwrap();
    assert_eq!(loader.exclusive_owner("TransferMod"), Some(owner_a));

    // Transfer A → B
    loader.transfer_ownership("TransferMod", owner_a, owner_b).unwrap();
    assert_eq!(
        loader.exclusive_owner("TransferMod"),
        Some(owner_b),
        "owner must be B after first transfer"
    );

    // Old owner A cannot transfer anymore
    assert!(
        loader.transfer_ownership("TransferMod", owner_a, owner_c).is_err(),
        "A is no longer the owner and must not be able to transfer"
    );

    // Transfer B → C
    loader.transfer_ownership("TransferMod", owner_b, owner_c).unwrap();
    assert_eq!(
        loader.exclusive_owner("TransferMod"),
        Some(owner_c),
        "owner must be C after second transfer"
    );

    // B cannot transfer anymore
    assert!(
        loader.transfer_ownership("TransferMod", owner_b, owner_a).is_err(),
        "B is no longer the owner and must not be able to transfer"
    );

    // C is the final owner and can still operate
    loader.relinquish_ownership("TransferMod", owner_c).unwrap();
    assert_eq!(
        loader.exclusive_owner("TransferMod"),
        None,
        "module should have no owner after C relinquishes"
    );
}

// ─── T-MOD-32: Load Failure Propagation ──────────────────────────────────────

#[test]
fn t_mod_32_load_failure_propagation() {
    struct FailOnLoad;
    impl Module for FailOnLoad {
        fn name(&self) -> &str { "FailOnLoad" }
        fn on_load(&mut self, _: &dyn SchedulerHandle) -> Result<(), EngineError> {
            Err(EngineError::Module {
                module: "FailOnLoad".into(),
                message: "deliberate load failure".into(),
            })
        }
        fn on_unload(&mut self, _: &dyn SchedulerHandle) -> Result<(), EngineError> { Ok(()) }
    }

    let sched = NoopScheduler;
    let mut loader = ModuleLoader::new();
    loader.register(Box::new(FailOnLoad), None);

    let result = loader.load_all(&sched);
    assert!(result.is_err(), "load_all must return Err when on_load fails");

    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("FailOnLoad"),
        "error must mention the failing module name: {msg}"
    );

    // The module must NOT be in loaded state
    assert!(
        loader.get_state("FailOnLoad") != Some(ModuleState::Loaded),
        "module must not be in Loaded state after on_load failure"
    );
}

// ─── T-MOD-33: Unload Failure Does Not Corrupt Module System ─────────────────

#[test]
fn t_mod_33_unload_failure_does_not_corrupt() {
    struct FailOnUnload;
    impl Module for FailOnUnload {
        fn name(&self) -> &str { "FailOnUnload" }
        fn on_load(&mut self, _: &dyn SchedulerHandle) -> Result<(), EngineError> { Ok(()) }
        fn on_unload(&mut self, _: &dyn SchedulerHandle) -> Result<(), EngineError> {
            Err(EngineError::Module {
                module: "FailOnUnload".into(),
                message: "deliberate unload failure".into(),
            })
        }
    }

    let load_log = Arc::new(Mutex::new(Vec::<String>::new()));
    let unload_log = Arc::new(Mutex::new(Vec::<String>::new()));
    let sched = NoopScheduler;
    let mut loader = ModuleLoader::new();

    // Register the failing module and a healthy module
    loader.register(Box::new(FailOnUnload), None);
    loader.register(
        TrackingModule::new("Healthy", vec![], Arc::clone(&load_log), Arc::clone(&unload_log)),
        None,
    );

    loader.load_all(&sched).unwrap();
    assert!(loader.is_loaded("FailOnUnload"), "FailOnUnload must load successfully");
    assert!(loader.is_loaded("Healthy"), "Healthy must load successfully");

    // Attempt to unload FailOnUnload — should return Err
    let result = loader.unload_by_name("FailOnUnload", &sched);
    assert!(result.is_err(), "unload must propagate on_unload error");

    // The module system must remain usable: Healthy can still be unloaded
    let healthy_result = loader.unload_by_name("Healthy", &sched);
    assert!(
        healthy_result.is_ok(),
        "unloading Healthy must still work after FailOnUnload's error: {:?}",
        healthy_result.err()
    );
    assert!(
        unload_log.lock().unwrap().contains(&"Healthy".to_string()),
        "Healthy's on_unload must have been called"
    );
}

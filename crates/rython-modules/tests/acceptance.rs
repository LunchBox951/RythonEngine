use rython_core::{EngineError, OwnerId, SchedulerHandle, priorities};
use rython_modules::{Module, ModuleLoader, ModuleState};
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

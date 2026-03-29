use crate::{
    frame_pacer::FramePacer,
    group::GroupState,
    task::{BgComplete, ParallelTask, RecurringTask, RemoteTask, SequentialTask},
};
use rython_core::{
    EngineError, GroupId, OwnerId, Priority, SchedulerConfig, SchedulerHandle, TaskError, TaskId,
};
use std::any::Any;
use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// A handle that allows cross-thread task submission to the scheduler.
#[derive(Clone)]
pub struct RemoteSender {
    tx: crossbeam_channel::Sender<RemoteTask>,
}

impl RemoteSender {
    pub fn submit(&self, f: Box<dyn FnOnce() -> Result<(), EngineError> + Send + 'static>, priority: Priority, owner: OwnerId) {
        let _ = self.tx.send(RemoteTask { owner, priority, f });
    }
}

impl SchedulerHandle for RemoteSender {
    fn submit_sequential(
        &self,
        f: Box<dyn FnOnce() -> Result<(), EngineError> + Send + 'static>,
        priority: Priority,
        owner: OwnerId,
    ) {
        self.submit(f, priority, owner);
    }

    fn cancel_owned(&self, _owner: OwnerId) {
        // Remote cancellation not supported — use scheduler.cancel_owned() directly
    }
}

/// The central task scheduler. Drives all engine work via tick().
pub struct TaskScheduler {
    // One-shot queues (filled before a tick, drained during it)
    seq_queue: Vec<SequentialTask>,
    par_queue: Vec<ParallelTask>,

    // Recurring tasks maintained across ticks
    recurring_seq: Vec<RecurringTask>,
    recurring_par: Vec<RecurringTask>,

    // Cross-thread submission channel
    remote_tx: crossbeam_channel::Sender<RemoteTask>,
    remote_rx: crossbeam_channel::Receiver<RemoteTask>,

    // Background task completion channel
    bg_tx: crossbeam_channel::Sender<BgComplete>,
    bg_rx: crossbeam_channel::Receiver<BgComplete>,

    // Task groups
    groups: HashMap<GroupId, Arc<GroupState>>,

    // Frame pacer
    pacer: FramePacer,

    // Pre-allocated scratch buffer for group callbacks ready to fire
    ready_group_callbacks: Vec<(
        GroupId,
        Box<
            dyn FnOnce(
                    Vec<Result<Box<dyn Any + Send + 'static>, EngineError>>,
                ) -> Result<(), EngineError>
                + Send
                + 'static,
        >,
        Vec<Result<Box<dyn Any + Send + 'static>, EngineError>>,
    )>,

    // Rayon thread pool
    pool: rayon::ThreadPool,
}

impl TaskScheduler {
    pub fn new(config: &SchedulerConfig) -> Self {
        let (remote_tx, remote_rx) = crossbeam_channel::unbounded();
        let (bg_tx, bg_rx) = crossbeam_channel::unbounded();

        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(config.parallel_threads.unwrap_or(0))
            .build()
            .expect("failed to build rayon thread pool");

        Self {
            seq_queue: Vec::new(),
            par_queue: Vec::new(),
            recurring_seq: Vec::new(),
            recurring_par: Vec::new(),
            remote_tx,
            remote_rx,
            bg_tx,
            bg_rx,
            groups: HashMap::new(),
            ready_group_callbacks: Vec::new(),
            pacer: FramePacer::new(config.target_fps, config.spin_threshold_us),
            pool,
        }
    }

    /// Returns a cloneable handle for cross-thread task submission.
    pub fn remote_sender(&self) -> RemoteSender {
        RemoteSender {
            tx: self.remote_tx.clone(),
        }
    }

    // ─── Submission ───────────────────────────────────────────────────────────

    /// Submit a one-shot sequential task.
    pub fn submit_sequential(
        &mut self,
        f: Box<dyn FnOnce() -> Result<(), EngineError> + Send + 'static>,
        priority: Priority,
        owner: OwnerId,
    ) -> TaskId {
        let id = next_id();
        self.seq_queue.push(SequentialTask { id, owner, priority, f });
        id
    }

    /// Submit a one-shot parallel task.
    pub fn submit_parallel(
        &mut self,
        f: Box<dyn Fn() -> Result<(), EngineError> + Send + Sync + 'static>,
        priority: Priority,
        owner: OwnerId,
    ) -> TaskId {
        let id = next_id();
        self.par_queue.push(ParallelTask { id, owner, priority, f });
        id
    }

    /// Submit a background (fire-and-forget) task.
    pub fn submit_background_raw(
        &mut self,
        f: Box<dyn FnOnce() -> Result<Box<dyn Any + Send + 'static>, EngineError> + Send + 'static>,
        callback: Option<
            Box<
                dyn FnOnce(
                        Result<Box<dyn Any + Send + 'static>, EngineError>,
                    ) -> Result<(), EngineError>
                    + Send
                    + 'static,
            >,
        >,
        _priority: Priority,
        owner: OwnerId,
    ) -> TaskId {
        let id = next_id();
        let bg_tx = self.bg_tx.clone();

        self.pool.spawn(move || {
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| f()));
            let result = match result {
                Ok(r) => r,
                Err(panic_val) => {
                    let msg = extract_panic_message(&panic_val);
                    Err(EngineError::Task(TaskError::Panicked { message: msg }))
                }
            };

            let _ = bg_tx.send(BgComplete {
                task_id: id,
                owner,
                result,
                callback,
                group_id: None,
            });
        });

        id
    }

    /// Typed wrapper for submit_background_raw.
    pub fn submit_background<F, R, C>(
        &mut self,
        f: F,
        callback: Option<C>,
        priority: Priority,
        owner: OwnerId,
    ) -> TaskId
    where
        F: FnOnce() -> R + Send + 'static,
        R: Any + Send + 'static,
        C: FnOnce(Result<R, EngineError>) -> Result<(), EngineError> + Send + 'static,
    {
        let erased_f = Box::new(move || -> Result<Box<dyn Any + Send + 'static>, EngineError> {
            Ok(Box::new(f()) as Box<dyn Any + Send + 'static>)
        });

        let erased_cb: Option<
            Box<
                dyn FnOnce(
                        Result<Box<dyn Any + Send + 'static>, EngineError>,
                    ) -> Result<(), EngineError>
                    + Send
                    + 'static,
            >,
        > = callback.map(|cb| {
            Box::new(
                move |res: Result<Box<dyn Any + Send + 'static>, EngineError>| {
                    let typed_res = res.map(|boxed| *boxed.downcast::<R>().expect("type mismatch in background callback"));
                    cb(typed_res)
                },
            ) as Box<dyn FnOnce(Result<Box<dyn Any + Send + 'static>, EngineError>) -> Result<(), EngineError> + Send + 'static>
        });

        self.submit_background_raw(erased_f, erased_cb, priority, owner)
    }

    /// Register a recurring sequential task (runs every tick until it returns false).
    pub fn register_recurring_sequential(
        &mut self,
        f: Box<dyn FnMut() -> bool + Send + 'static>,
        priority: Priority,
        owner: OwnerId,
    ) -> TaskId {
        let id = next_id();
        self.recurring_seq.push(RecurringTask { id, owner, priority, f });
        id
    }

    /// Register a recurring parallel task (runs every tick until it returns false).
    pub fn register_recurring_parallel(
        &mut self,
        f: Box<dyn FnMut() -> bool + Send + 'static>,
        priority: Priority,
        owner: OwnerId,
    ) -> TaskId {
        let id = next_id();
        self.recurring_par.push(RecurringTask { id, owner, priority, f });
        id
    }

    // ─── Task Groups ──────────────────────────────────────────────────────────

    /// Create a new task group. The callback fires when all members complete.
    pub fn create_group(
        &mut self,
        callback: Box<
            dyn FnOnce(Vec<Result<Box<dyn Any + Send + 'static>, EngineError>>) -> Result<(), EngineError>
                + Send
                + 'static,
        >,
        owner: OwnerId,
    ) -> GroupId {
        let id = next_id();
        let state = GroupState::new(id, owner, callback);
        self.groups.insert(id, state);
        id
    }

    /// Add a background member to an existing (unsealed) group.
    pub fn group_add_background<F, R>(
        &mut self,
        group_id: GroupId,
        f: F,
    ) where
        F: FnOnce() -> R + Send + 'static,
        R: Any + Send + 'static,
    {
        let group_state = match self.groups.get(&group_id) {
            Some(s) => Arc::clone(s),
            None => return,
        };

        group_state.add_member();

        let bg_tx = self.bg_tx.clone();
        let task_id = next_id();
        let owner = group_state.owner;

        self.pool.spawn(move || {
            let result: Result<Box<dyn Any + Send + 'static>, EngineError> =
                Ok(Box::new(f()) as Box<dyn Any + Send + 'static>);

            let _ = bg_tx.send(BgComplete {
                task_id,
                owner,
                result,
                callback: None,
                group_id: Some(group_id),
            });
        });
    }

    /// Seal a group. No more members may be added. Fires callback if already done.
    pub fn group_seal(&mut self, group_id: GroupId) {
        if let Some(state) = self.groups.get(&group_id) {
            let already_done = state.seal();
            if already_done {
                if let Some((cb, results)) = state.take_callback_and_results() {
                    let _ = cb(results);
                }
                self.groups.remove(&group_id);
            }
        }
    }

    // ─── Cancellation ─────────────────────────────────────────────────────────

    /// Cancel all pending tasks owned by the given owner.
    pub fn cancel_owned(&mut self, owner: OwnerId) {
        self.seq_queue.retain(|t| t.owner != owner);
        self.par_queue.retain(|t| t.owner != owner);
        self.recurring_seq.retain(|t| t.owner != owner);
        self.recurring_par.retain(|t| t.owner != owner);
    }

    // ─── Tick ─────────────────────────────────────────────────────────────────

    /// Execute one frame tick: drain remote queue, run sequential/parallel/background phases,
    /// process bg completions, then pace to target FPS.
    pub fn tick(&mut self) -> Result<(), EngineError> {
        let tick_start = self.pacer.tick_start();

        // 1. Drain remote queue
        while let Ok(remote) = self.remote_rx.try_recv() {
            self.seq_queue.push(SequentialTask {
                id: next_id(),
                owner: remote.owner,
                priority: remote.priority,
                f: remote.f,
            });
        }

        // 2. Sequential phase
        // Sort one-shots in-place by priority and drain, avoiding an intermediate Vec
        self.seq_queue.sort_by_key(|t| t.priority);

        for t in std::mem::take(&mut self.seq_queue) {
            run_sequential_task(t.f);
        }

        // Run recurring sequential tasks (sorted by priority)
        self.recurring_seq.sort_by_key(|t| t.priority);
        self.recurring_seq.retain_mut(|t| {
            std::panic::catch_unwind(AssertUnwindSafe(|| (t.f)())).unwrap_or(false)
        });

        // 3. Parallel phase
        let par_tasks = std::mem::take(&mut self.par_queue);
        if !par_tasks.is_empty() {
            use rayon::prelude::*;
            self.pool.install(|| {
                par_tasks.par_iter().for_each(|t| {
                    let result = std::panic::catch_unwind(AssertUnwindSafe(|| (t.f)()));
                    if let Err(e) = result {
                        // log panic — in real engine would use a logger
                        let _ = e;
                    }
                });
            });
        }

        // Run recurring parallel tasks
        self.pool.install(|| {
            self.recurring_par.retain_mut(|t| {
                std::panic::catch_unwind(AssertUnwindSafe(|| (t.f)())).unwrap_or(false)
            });
        });

        // 4. Background phase: check completions, fire callbacks
        while let Ok(complete) = self.bg_rx.try_recv() {
            if let Some(group_id) = complete.group_id {
                if let Some(state) = self.groups.get(&group_id) {
                    if state.member_complete(complete.result) {
                        if let Some((cb, results)) = state.take_callback_and_results() {
                            self.ready_group_callbacks.push((group_id, cb, results));
                        }
                    }
                }
            } else if let Some(cb) = complete.callback {
                // Run callback as a sequential task
                let _ = cb(complete.result);
            }
        }

        // Fire group callbacks and remove finished groups
        for (group_id, cb, results) in self.ready_group_callbacks.drain(..) {
            let _ = cb(results);
            self.groups.remove(&group_id);
        }

        // 5. Frame pacing
        let _ = tick_start; // pacer uses its own stored start time
        self.pacer.tick_end();

        Ok(())
    }
}

fn run_sequential_task(f: Box<dyn FnOnce() -> Result<(), EngineError> + Send>) {
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| f()));
    match result {
        Ok(Ok(())) => {}
        Ok(Err(_e)) => {
            // In real engine: log the error
        }
        Err(_panic_val) => {
            // In real engine: log TaskError::Panicked
        }
    }
}

fn extract_panic_message(panic_val: &dyn Any) -> String {
    if let Some(s) = panic_val.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = panic_val.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_string()
    }
}

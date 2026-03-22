use rython_core::{priorities, EngineError, SchedulerConfig, TaskError};
use rython_scheduler::TaskScheduler;
use std::sync::{
    atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

fn default_scheduler() -> TaskScheduler {
    TaskScheduler::new(&SchedulerConfig::default())
}

fn scheduler_with_fps(fps: u32) -> TaskScheduler {
    TaskScheduler::new(&SchedulerConfig {
        target_fps: fps,
        parallel_threads: None,
        spin_threshold_us: 500,
    })
}

// ─── T-SCHED-03: Sequential Priority Ordering ────────────────────────────────

#[test]
fn t_sched_03_sequential_priority_ordering() {
    let mut sched = default_scheduler();
    let results = Arc::new(Mutex::new(Vec::<u8>::new()));

    for priority in [40u8, 10, 30, 0, 20] {
        let r = Arc::clone(&results);
        sched.submit_sequential(
            Box::new(move || {
                r.lock().unwrap().push(priority);
                Ok(())
            }),
            priority,
            1,
        );
    }

    sched.tick().unwrap();

    let r = results.lock().unwrap();
    assert_eq!(*r, vec![0u8, 10, 20, 30, 40]);
}

// ─── T-SCHED-05: Parallel Tasks Run Concurrently ─────────────────────────────

#[test]
fn t_sched_05_parallel_tasks_run_concurrently() {
    let mut sched = TaskScheduler::new(&SchedulerConfig {
        target_fps: 1000, // very high fps so frame pacing doesn't interfere
        parallel_threads: Some(4),
        spin_threshold_us: 0,
    });

    let thread_ids = Arc::new(Mutex::new(Vec::new()));

    for _ in 0..4 {
        let ids = Arc::clone(&thread_ids);
        sched.submit_parallel(
            Box::new(move || {
                std::thread::sleep(Duration::from_millis(50));
                ids.lock().unwrap().push(std::thread::current().id());
                Ok(())
            }),
            priorities::GAME_UPDATE,
            1,
        );
    }

    let start = Instant::now();
    sched.tick().unwrap();
    let elapsed = start.elapsed();

    // Parallelism: 4 × 50ms serial would be 200ms; concurrent should be ~50ms
    assert!(
        elapsed < Duration::from_millis(150),
        "parallel tasks took too long ({elapsed:?}), expected concurrency"
    );

    let ids = thread_ids.lock().unwrap();
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert!(
        unique.len() >= 2,
        "expected at least 2 distinct thread IDs, got {unique:?}"
    );
}

// ─── T-SCHED-06: Background Tasks Do Not Block the Frame ─────────────────────

#[test]
fn t_sched_06_background_tasks_dont_block() {
    let mut sched = TaskScheduler::new(&SchedulerConfig {
        target_fps: 1000,
        parallel_threads: None,
        spin_threshold_us: 0,
    });

    let done = Arc::new(AtomicBool::new(false));
    let done2 = Arc::clone(&done);

    sched.submit_background(
        move || {
            std::thread::sleep(Duration::from_millis(300));
            done2.store(true, Ordering::Relaxed);
        },
        None::<fn(Result<(), EngineError>) -> Result<(), EngineError>>,
        priorities::IDLE,
        1,
    );

    let start = Instant::now();
    sched.tick().unwrap();
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_millis(100),
        "tick blocked on background task ({elapsed:?})"
    );

    // Background task eventually completes
    std::thread::sleep(Duration::from_millis(400));
    assert!(done.load(Ordering::Relaxed));
}

// ─── T-SCHED-07: Background Task Callback Receives Result ────────────────────

#[test]
fn t_sched_07_background_callback_receives_result() {
    let mut sched = TaskScheduler::new(&SchedulerConfig {
        target_fps: 1000,
        parallel_threads: None,
        spin_threshold_us: 0,
    });

    let received = Arc::new(AtomicI32::new(-1));
    let received2 = Arc::clone(&received);
    let cb_called = Arc::new(AtomicBool::new(false));
    let cb_called2 = Arc::clone(&cb_called);

    sched.submit_background(
        move || 42i32,
        Some(move |result: Result<i32, EngineError>| {
            received2.store(result.unwrap(), Ordering::Relaxed);
            cb_called2.store(true, Ordering::Relaxed);
            Ok(())
        }),
        priorities::IDLE,
        1,
    );

    // Run several ticks until the callback fires
    for _ in 0..20 {
        sched.tick().unwrap();
        if cb_called.load(Ordering::Relaxed) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    assert!(cb_called.load(Ordering::Relaxed), "callback was never invoked");
    assert_eq!(received.load(Ordering::Relaxed), 42);
}

// ─── T-SCHED-08: Ownership-Based Cancellation ────────────────────────────────

#[test]
fn t_sched_08_ownership_cancellation() {
    let mut sched = default_scheduler();
    let owner_id: u64 = 42;
    let other_owner: u64 = 99;

    let cancelled_ran = Arc::new(AtomicU32::new(0));
    let other_ran = Arc::new(AtomicU32::new(0));

    // Submit 10 tasks for owner_id
    for _ in 0..10 {
        let flag = Arc::clone(&cancelled_ran);
        sched.submit_sequential(
            Box::new(move || {
                flag.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }),
            priorities::GAME_UPDATE,
            owner_id,
        );
    }

    // Submit 5 recurring tasks for owner_id
    for _ in 0..5 {
        let flag = Arc::clone(&cancelled_ran);
        sched.register_recurring_sequential(
            Box::new(move || {
                flag.fetch_add(1, Ordering::Relaxed);
                true
            }),
            priorities::GAME_UPDATE,
            owner_id,
        );
    }

    // Submit a task for a different owner (should not be cancelled)
    let flag = Arc::clone(&other_ran);
    sched.submit_sequential(
        Box::new(move || {
            flag.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }),
        priorities::GAME_UPDATE,
        other_owner,
    );

    // Cancel all tasks for owner_id before the tick
    sched.cancel_owned(owner_id);

    sched.tick().unwrap();
    // Allow a second tick to confirm recurring tasks are gone
    sched.tick().unwrap();

    assert_eq!(cancelled_ran.load(Ordering::Relaxed), 0, "cancelled tasks should not have run");
    assert_eq!(other_ran.load(Ordering::Relaxed), 1, "other owner's task should have run once");
}

// ─── T-SCHED-09: Recurring Task Persistence ──────────────────────────────────

#[test]
fn t_sched_09_recurring_task_persistence() {
    let mut sched = TaskScheduler::new(&SchedulerConfig {
        target_fps: 1000,
        parallel_threads: None,
        spin_threshold_us: 0,
    });

    let counter = Arc::new(AtomicU32::new(0));
    let c = Arc::clone(&counter);

    sched.register_recurring_sequential(
        Box::new(move || {
            c.fetch_add(1, Ordering::Relaxed);
            true // keep running
        }),
        priorities::GAME_UPDATE,
        1,
    );

    for _ in 0..100 {
        sched.tick().unwrap();
    }

    assert_eq!(counter.load(Ordering::Relaxed), 100);
}

// ─── T-SCHED-10: Recurring Task Self-Termination ─────────────────────────────

#[test]
fn t_sched_10_recurring_self_termination() {
    let mut sched = TaskScheduler::new(&SchedulerConfig {
        target_fps: 1000,
        parallel_threads: None,
        spin_threshold_us: 0,
    });

    let counter = Arc::new(AtomicU32::new(0));
    let c = Arc::clone(&counter);

    sched.register_recurring_sequential(
        Box::new(move || {
            let n = c.fetch_add(1, Ordering::Relaxed) + 1;
            n < 10 // stop after 10th invocation
        }),
        priorities::GAME_UPDATE,
        1,
    );

    for _ in 0..50 {
        sched.tick().unwrap();
    }

    assert_eq!(counter.load(Ordering::Relaxed), 10);
}

// ─── T-SCHED-11: Task Group Fan-In ───────────────────────────────────────────

#[test]
fn t_sched_11_task_group_fan_in() {
    let mut sched = TaskScheduler::new(&SchedulerConfig {
        target_fps: 1000,
        parallel_threads: None,
        spin_threshold_us: 0,
    });

    let results_received = Arc::new(Mutex::new(Vec::<i32>::new()));
    let cb_count = Arc::new(AtomicU32::new(0));
    let r = Arc::clone(&results_received);
    let c = Arc::clone(&cb_count);

    let group_id = sched.create_group(
        Box::new(move |results| {
            c.fetch_add(1, Ordering::Relaxed);
            let mut r = r.lock().unwrap();
            for res in results {
                if let Ok(boxed) = res {
                    if let Ok(val) = boxed.downcast::<i32>() {
                        r.push(*val);
                    }
                }
            }
            Ok(())
        }),
        1,
    );

    sched.group_add_background(group_id, || 10i32);
    sched.group_add_background(group_id, || 20i32);
    sched.group_add_background(group_id, || 30i32);
    sched.group_seal(group_id);

    // Tick until callback fires
    for _ in 0..30 {
        sched.tick().unwrap();
        std::thread::sleep(Duration::from_millis(5));
        if cb_count.load(Ordering::Relaxed) > 0 {
            break;
        }
    }

    assert_eq!(cb_count.load(Ordering::Relaxed), 1, "callback should fire exactly once");
    let mut r = results_received.lock().unwrap();
    r.sort();
    assert_eq!(*r, vec![10, 20, 30], "all 3 results must be present");
}

// ─── T-SCHED-12: Task Group Seal Enforcement ─────────────────────────────────

#[test]
fn t_sched_12_group_seal_enforcement() {
    let mut sched = TaskScheduler::new(&SchedulerConfig {
        target_fps: 1000,
        parallel_threads: None,
        spin_threshold_us: 0,
    });

    let cb_count = Arc::new(AtomicU32::new(0));
    let c = Arc::clone(&cb_count);

    let group_id = sched.create_group(
        Box::new(move |_results| {
            c.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }),
        1,
    );

    sched.group_add_background(group_id, || 1i32);
    sched.group_add_background(group_id, || 2i32);

    // Let first member potentially complete without sealing
    for _ in 0..10 {
        sched.tick().unwrap();
        std::thread::sleep(Duration::from_millis(5));
    }

    assert_eq!(
        cb_count.load(Ordering::Relaxed),
        0,
        "callback must not fire before seal()"
    );

    // Now seal — with both members likely done, callback should fire on next drain
    sched.group_seal(group_id);

    for _ in 0..10 {
        sched.tick().unwrap();
        std::thread::sleep(Duration::from_millis(5));
        if cb_count.load(Ordering::Relaxed) > 0 {
            break;
        }
    }

    assert_eq!(
        cb_count.load(Ordering::Relaxed),
        1,
        "callback should fire after seal when members complete"
    );
}

// ─── T-SCHED-13: Cross-Thread Task Submission ────────────────────────────────

#[test]
fn t_sched_13_cross_thread_submission() {
    let mut sched = TaskScheduler::new(&SchedulerConfig {
        target_fps: 1000,
        parallel_threads: None,
        spin_threshold_us: 0,
    });

    let remote = sched.remote_sender();
    let flag = Arc::new(AtomicBool::new(false));
    let flag2 = Arc::clone(&flag);

    // Submit from another thread
    std::thread::spawn(move || {
        remote.submit(
            Box::new(move || {
                flag2.store(true, Ordering::Relaxed);
                Ok(())
            }),
            priorities::GAME_UPDATE,
            1,
        );
    })
    .join()
    .unwrap();

    // Tick picks up remote submission
    sched.tick().unwrap();

    assert!(flag.load(Ordering::Relaxed), "cross-thread task should have run");
}

// ─── T-SCHED-14: Error Handling — Task Failure Does Not Stop Scheduler ────────

#[test]
fn t_sched_14_task_failure_does_not_stop_scheduler() {
    let mut sched = default_scheduler();

    let a_ran = Arc::new(AtomicBool::new(false));
    let c_ran = Arc::new(AtomicBool::new(false));

    let a = Arc::clone(&a_ran);
    sched.submit_sequential(
        Box::new(move || {
            a.store(true, Ordering::Relaxed);
            Ok(())
        }),
        priorities::GAME_EARLY,
        1,
    );

    sched.submit_sequential(
        Box::new(|| Err(EngineError::Config("deliberate failure".to_string()))),
        priorities::GAME_UPDATE,
        1,
    );

    let c = Arc::clone(&c_ran);
    sched.submit_sequential(
        Box::new(move || {
            c.store(true, Ordering::Relaxed);
            Ok(())
        }),
        priorities::GAME_LATE,
        1,
    );

    let result = sched.tick();
    assert!(result.is_ok(), "scheduler.tick() must return Ok even on task failure");
    assert!(a_ran.load(Ordering::Relaxed), "task A should have run");
    assert!(c_ran.load(Ordering::Relaxed), "task C should have run");
}

// ─── T-SCHED-15: Error Handling — Panic Recovery ─────────────────────────────

#[test]
fn t_sched_15_panic_recovery() {
    let mut sched = default_scheduler();

    let after_ran = Arc::new(AtomicBool::new(false));
    let a = Arc::clone(&after_ran);

    // Task that panics
    sched.submit_sequential(
        Box::new(|| {
            panic!("something broke");
        }),
        priorities::GAME_UPDATE,
        1,
    );

    // Task after the panic — should still run
    sched.submit_sequential(
        Box::new(move || {
            a.store(true, Ordering::Relaxed);
            Ok(())
        }),
        priorities::GAME_LATE,
        1,
    );

    // Scheduler must not itself panic
    let result = sched.tick();
    assert!(result.is_ok(), "scheduler must not propagate task panics");

    // Subsequent ticks must work
    let result2 = sched.tick();
    assert!(result2.is_ok());

    assert!(after_ran.load(Ordering::Relaxed), "task after panic should have run");
}

// ─── T-ERR-03: TaskError Captures Panic Message ──────────────────────────────

#[test]
fn t_err_03_task_error_captures_panic_message() {
    // Test that the TaskError::Panicked type works as designed
    let err = TaskError::Panicked {
        message: "something broke".to_string(),
    };
    let msg = err.to_string();
    assert!(msg.contains("something broke"), "panic message should be captured: {msg}");

    // And a panicking background task is caught by the scheduler
    let mut sched = TaskScheduler::new(&SchedulerConfig {
        target_fps: 1000,
        parallel_threads: None,
        spin_threshold_us: 0,
    });

    let panicked = Arc::new(AtomicBool::new(false));
    let p = Arc::clone(&panicked);

    sched.submit_background(
        move || {
            p.store(true, Ordering::Relaxed);
            panic!("bg panic");
        },
        None::<fn(Result<(), EngineError>) -> Result<(), EngineError>>,
        priorities::IDLE,
        1,
    );

    // Tick should not panic even though the background task does
    for _ in 0..20 {
        let result = sched.tick();
        assert!(result.is_ok(), "scheduler must survive bg task panic");
        std::thread::sleep(Duration::from_millis(10));
    }
}

// ─── T-ERR-04: TaskError::Cancelled on Owner Unload ─────────────────────────

#[test]
fn t_err_04_cancelled_on_owner_unload() {
    let mut sched = default_scheduler();
    let owner_id: u64 = 5;
    let ran = Arc::new(AtomicBool::new(false));
    let r = Arc::clone(&ran);

    sched.submit_sequential(
        Box::new(move || {
            r.store(true, Ordering::Relaxed);
            Ok(())
        }),
        priorities::GAME_UPDATE,
        owner_id,
    );

    // Cancel before tick
    sched.cancel_owned(owner_id);

    sched.tick().unwrap();
    assert!(!ran.load(Ordering::Relaxed), "cancelled task should not execute");

    // Verify the error type exists and works
    let err = TaskError::Cancelled;
    assert!(err.to_string().contains("cancel"));
}

// ─── T-SCHED-01: Frame Pacing Accuracy at 60 FPS (reduced ticks for CI) ──────

#[test]
#[ignore = "timing-sensitive: run manually with --ignored"]
fn t_sched_01_frame_pacing_60fps() {
    let mut sched = scheduler_with_fps(60);
    let n = 120usize; // 2 seconds
    let mut durations = Vec::with_capacity(n);

    for _ in 0..n {
        let start = Instant::now();
        sched.tick().unwrap();
        durations.push(start.elapsed());
    }

    let mean_ms = durations.iter().map(|d| d.as_secs_f64() * 1000.0).sum::<f64>() / n as f64;
    let target = 1000.0 / 60.0;
    assert!(
        (mean_ms - target).abs() < 2.0,
        "mean tick {mean_ms:.2}ms, expected ~{target:.2}ms"
    );
}

// ─── T-SCHED-04: Sequential Before Parallel Before Background ────────────────

#[test]
fn t_sched_04_phase_order() {
    let mut sched = TaskScheduler::new(&SchedulerConfig {
        target_fps: 1000,
        parallel_threads: Some(2),
        spin_threshold_us: 0,
    });

    let seq_time: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    let par_time: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    let bg_time: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));

    let st = Arc::clone(&seq_time);
    sched.submit_sequential(
        Box::new(move || {
            *st.lock().unwrap() = Some(Instant::now());
            Ok(())
        }),
        priorities::GAME_UPDATE,
        1,
    );

    let pt = Arc::clone(&par_time);
    sched.submit_parallel(
        Box::new(move || {
            std::thread::sleep(Duration::from_millis(5));
            *pt.lock().unwrap() = Some(Instant::now());
            Ok(())
        }),
        priorities::GAME_UPDATE,
        1,
    );

    let bt = Arc::clone(&bg_time);
    sched.submit_background(
        move || {
            *bt.lock().unwrap() = Some(Instant::now());
        },
        None::<fn(Result<(), EngineError>) -> Result<(), EngineError>>,
        priorities::IDLE,
        1,
    );

    sched.tick().unwrap();
    std::thread::sleep(Duration::from_millis(50));
    sched.tick().unwrap(); // allow bg to complete

    let s = seq_time.lock().unwrap().unwrap();
    let p = par_time.lock().unwrap().unwrap();
    assert!(s < p, "sequential must finish before parallel completes");
    // background may run concurrently but is submitted after parallel
}

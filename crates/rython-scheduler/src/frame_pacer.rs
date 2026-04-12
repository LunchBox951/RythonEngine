use std::time::{Duration, Instant};

/// Maintains target tick rate using a hybrid sleep+spin strategy.
pub struct FramePacer {
    target_duration: Duration,
    spin_threshold: Duration,
    last_tick_start: Option<Instant>,
}

impl FramePacer {
    /// Construct a pacer targeting `target_fps` frames per second.
    /// A `target_fps` of zero is clamped to 1, avoiding the integer
    /// divide-by-zero that would otherwise panic here.
    pub fn new(target_fps: u32, spin_threshold_us: u64) -> Self {
        let fps = target_fps.max(1);
        let nanos = 1_000_000_000u64 / fps as u64;
        Self {
            target_duration: Duration::from_nanos(nanos),
            spin_threshold: Duration::from_micros(spin_threshold_us),
            last_tick_start: None,
        }
    }

    /// Call at the start of each tick to record the start time.
    pub fn tick_start(&mut self) -> Instant {
        let now = Instant::now();
        self.last_tick_start = Some(now);
        now
    }

    /// Call at the end of each tick to wait until the target duration has elapsed.
    pub fn tick_end(&self) {
        let start = match self.last_tick_start {
            Some(s) => s,
            None => return,
        };

        let elapsed = start.elapsed();
        if elapsed >= self.target_duration {
            return;
        }

        let remaining = self.target_duration - elapsed;

        if remaining > self.spin_threshold {
            let sleep_duration = remaining - self.spin_threshold;
            std::thread::sleep(sleep_duration);
        }

        // Spin for the final sub-threshold duration
        let deadline = start + self.target_duration;
        while Instant::now() < deadline {
            std::hint::spin_loop();
        }
    }

    pub fn target_duration(&self) -> Duration {
        self.target_duration
    }
}

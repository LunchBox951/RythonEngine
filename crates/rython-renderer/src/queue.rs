use crate::command::DrawCommand;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Mutex,
};

/// Double-buffered draw command queue.
///
/// During `RENDER_ENQUEUE`, game systems call [`push`] which writes to the back buffer.
/// At the phase boundary, [`swap`] atomically exchanges front and back.
/// During `RENDER_EXECUTE`, the renderer calls [`take_sorted_front`] to read and
/// drain the front buffer sorted by z-value.
///
/// The renderer never reads a partially-written back buffer because it only reads
/// from the front buffer, which was the sealed back buffer of the previous frame.
pub struct CommandQueue {
    max_commands: usize,
    /// Front buffer: renderer reads this during RENDER_EXECUTE.
    front: Mutex<Vec<DrawCommand>>,
    /// Back buffer: game systems write this during RENDER_ENQUEUE.
    back: Mutex<Vec<DrawCommand>>,
    /// Count of commands dropped this frame due to capacity overflow.
    dropped: AtomicU32,
}

impl CommandQueue {
    pub fn new(max_commands: usize) -> Self {
        Self {
            max_commands,
            front: Mutex::new(Vec::new()),
            back: Mutex::new(Vec::with_capacity(max_commands)),
            dropped: AtomicU32::new(0),
        }
    }

    /// Push a draw command into the back buffer.
    ///
    /// If the queue is at capacity the command is silently dropped and the drop
    /// counter is incremented. A warning is logged.
    pub fn push(&self, cmd: DrawCommand) {
        let mut back = self.back.lock().unwrap();
        if back.len() < self.max_commands {
            back.push(cmd);
        } else {
            let dropped = self.dropped.fetch_add(1, Ordering::Relaxed) + 1;
            log::warn!(
                "draw command dropped: queue at max_commands={} (total dropped this frame: {})",
                self.max_commands,
                dropped
            );
        }
    }

    /// Swap front and back buffers at the RENDER_ENQUEUE / RENDER_EXECUTE boundary.
    ///
    /// After the swap, `front` holds this frame's submitted commands and `back`
    /// is cleared and ready for the next frame.
    pub fn swap(&self) {
        let mut front = self.front.lock().unwrap();
        let mut back = self.back.lock().unwrap();
        // front ← back (this frame's commands); back ← front (cleared for next frame)
        std::mem::swap(&mut *front, &mut *back);
        back.clear();
        self.dropped.store(0, Ordering::Relaxed);
    }

    /// Number of commands available in the front buffer (ready to render).
    pub fn front_len(&self) -> usize {
        self.front.lock().unwrap().len()
    }

    /// Number of commands currently in the back buffer (being written this frame).
    pub fn back_len(&self) -> usize {
        self.back.lock().unwrap().len()
    }

    /// Commands dropped this frame due to capacity overflow.
    pub fn dropped_count(&self) -> u32 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Drain the front buffer and return commands sorted by z-value ascending.
    ///
    /// Lower z values are drawn first (painter's algorithm / back-to-front).
    /// Caller should invoke this once per frame after [`swap`].
    ///
    /// The front buffer's heap allocation is preserved (via `drain` instead of
    /// `std::mem::take`) so that the next frame reuses the same capacity without
    /// a fresh allocation.
    pub fn take_sorted_front(&self) -> Vec<DrawCommand> {
        let mut front = self.front.lock().unwrap();
        front.sort_by(|a, b| {
            a.z()
                .partial_cmp(&b.z())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        front.drain(..).collect()
    }
}

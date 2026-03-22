/// Opaque identifier for task/module owners.
pub type OwnerId = u64;

/// Task execution priority. Lower numbers run first.
pub type Priority = u8;

/// Task priority constants matching the frame timeline.
pub mod priorities {
    pub const MODULE_LIFECYCLE: u8 = 0;
    pub const ENGINE_SETUP: u8 = 5;
    pub const PRE_UPDATE: u8 = 10;
    pub const GAME_EARLY: u8 = 15;
    pub const GAME_UPDATE: u8 = 20;
    pub const GAME_LATE: u8 = 25;
    pub const RENDER_ENQUEUE: u8 = 30;
    pub const RENDER_EXECUTE: u8 = 35;
    pub const IDLE: u8 = 40;
}

/// Unique identifier for a task.
pub type TaskId = u64;

/// Unique identifier for a task group.
pub type GroupId = u64;

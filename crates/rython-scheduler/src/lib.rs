#![deny(warnings)]

pub mod frame_pacer;
pub mod group;
pub mod scheduler;
pub mod task;

pub use frame_pacer::FramePacer;
pub use group::GroupState;
pub use scheduler::{RemoteSender, TaskScheduler};
pub use task::{
    BackgroundTask, BgCallback, BgComplete, BgResult, BgTaskFn, GroupCallback, ParallelTask,
    RecurringTask, RemoteTask, SequentialTask,
};

#![deny(warnings)]

pub mod action;
pub mod binding;
pub mod bitset;
pub mod context;
pub mod controller;
pub mod events;
pub mod input_map;
pub mod modifier;
pub mod snapshot;
pub mod trigger;
pub mod value;

pub use action::InputAction as InputActionDecl;
pub use binding::{HardwareKey, InputBinding, InputSource};
pub use context::{ActionEvaluation, InputMappingContext};
pub use controller::PlayerController;
pub use events::InputActionEvent;
pub use input_map::{AxisBinding, ButtonBinding, InputMap};
pub use modifier::{Modifier, SwizzleOrder, apply_pipeline};
pub use snapshot::InputSnapshot;
pub use trigger::{Trigger, TriggerCtx, TriggerState};
pub use value::{ActionValue, BUTTON_THRESHOLD, ValueKind};

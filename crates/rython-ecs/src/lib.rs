#![deny(warnings)]

pub mod command;
pub mod component;
pub mod entity;
pub mod event_bus;
pub mod hierarchy;
pub mod scene;
pub mod systems;

#[cfg(test)]
mod tests;

pub use command::{Command, CommandQueue};
pub use component::{
    BillboardComponent, ColliderComponent, Component, ComponentStorage, LightComponent, LightKind,
    MeshComponent, RigidBodyComponent, TagComponent, TransformComponent,
};
pub use entity::EntityId;
pub use event_bus::{EventBus, HandlerId};
pub use hierarchy::{Hierarchy, MAX_HIERARCHY_DEPTH};
pub use scene::{Scene, SpawnHandle};
pub use systems::light::CollectedLight;
pub use systems::render::DrawCommand;
pub use systems::transform::WorldTransform;
pub use systems::{LightSystem, RenderSystem, TransformSystem};

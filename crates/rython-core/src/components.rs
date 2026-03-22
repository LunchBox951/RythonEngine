use downcast_rs::{impl_downcast, Downcast};

/// Trait for ECS components. Components must be downcasted by type.
pub trait Component: Downcast + Send + Sync + 'static {
    fn component_type_name(&self) -> &'static str;
}

impl_downcast!(Component);

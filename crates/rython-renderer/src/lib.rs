#![deny(warnings)]

pub mod camera;
pub mod command;
pub mod config;
pub mod gpu;
pub mod queue;
pub mod shaders;

pub use camera::Camera;
pub use command::{
    norm_to_clip, rect_to_clip_verts, Color, DrawBillboard, DrawCircle, DrawCommand, DrawImage,
    DrawLine, DrawMesh, DrawRect, DrawText,
};
pub use config::{RendererConfig, SceneSettings};
pub use gpu::{BindGroupLayouts, GpuContext, GpuUploadRequest, MeshBuffers, Pipelines, RendererError, RendererState};
pub use queue::CommandQueue;
pub use shaders::{
    validate_wgsl, ShaderError, IMAGE_WGSL, MESH_WGSL, PRIMITIVE_WGSL, TEXT_WGSL,
};

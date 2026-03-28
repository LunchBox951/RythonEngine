use std::collections::HashMap;

use crate::camera::Camera;
use crate::command::{DrawMesh, DrawRect, DrawText};
use crate::config::{RendererConfig, SceneSettings};
use crate::light::{GpuLight, LightBuffer};
use crate::shaders::{IMAGE_WGSL, MESH_WGSL, PRIMITIVE_WGSL, TEXT_WGSL};
use thiserror::Error;

/// Errors produced by the GPU renderer context.
#[derive(Debug, Error)]
pub enum RendererError {
    #[error("no wgpu adapter available (Vulkan/Metal/DX12 required)")]
    NoAdapter,

    #[error("wgpu device creation failed: {0}")]
    DeviceCreation(String),

    #[error("surface error: {0}")]
    Surface(String),

    #[error("shader compilation failed at {location}: {message}")]
    Shader { location: String, message: String },

    #[error("texture creation failed: {0}")]
    Texture(String),

    #[error("buffer overflow: submitted {submitted} commands, max {max}")]
    BufferOverflow { submitted: usize, max: usize },
}

/// GPU upload request queued from a background decode thread.
pub struct GpuUploadRequest {
    pub width: u32,
    pub height: u32,
    /// Raw RGBA8 pixel data decoded on the background thread.
    pub pixels: Vec<u8>,
    /// Callback invoked on the main thread once the texture is ready.
    pub on_ready: Box<dyn FnOnce(wgpu::Texture) + Send + 'static>,
}

/// Compiled render pipelines for all built-in shader programs.
pub struct Pipelines {
    pub primitive: wgpu::RenderPipeline,
    pub image: wgpu::RenderPipeline,
    pub text: wgpu::RenderPipeline,
    pub mesh: wgpu::RenderPipeline,
}

/// Bind group layouts matching the bindings declared in each built-in shader.
///
/// Stored on [`GpuContext`] so callers can create compatible bind groups when
/// uploading per-draw uniforms, textures, and samplers.
pub struct BindGroupLayouts {
    /// `primitive` shader: group(0) = uniform buffer
    pub primitive: wgpu::BindGroupLayout,
    /// `image` shader: group(0) = uniform buffer + texture_2d + sampler
    pub image: wgpu::BindGroupLayout,
    /// `text` shader: group(0) = uniform buffer + texture_2d + sampler
    pub text: wgpu::BindGroupLayout,
    /// `mesh` shader: group(0) = camera uniform buffer
    pub mesh_camera: wgpu::BindGroupLayout,
    /// `mesh` shader: group(1) = model uniform buffer
    pub mesh_model: wgpu::BindGroupLayout,
    /// `mesh` shader: group(2) = diffuse texture_2d + sampler
    pub mesh_texture: wgpu::BindGroupLayout,
    /// `mesh` shader: group(3) = normal map texture_2d + sampler
    pub mesh_normal_map: wgpu::BindGroupLayout,
    /// `mesh` shader: group(4) = specular map texture_2d + sampler
    pub mesh_specular_map: wgpu::BindGroupLayout,
    /// `mesh` shader: group(5) = LightBuffer uniform (multi-light)
    pub mesh_light_buffer: wgpu::BindGroupLayout,
    /// `mesh` shader: group(6) = emissive map texture_2d + sampler
    pub mesh_emissive_map: wgpu::BindGroupLayout,
}

/// GPU context: wgpu instance, adapter, device, queue, surface, and pipelines.
///
/// All GPU API calls must happen on the thread that owns this context (the main
/// render thread).  Background threads produce [`GpuUploadRequest`] values which
/// are processed here during the render tick.
pub struct GpuContext {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub pipelines: Pipelines,
    pub bind_group_layouts: BindGroupLayouts,
    pub surface_format: wgpu::TextureFormat,
    /// MSAA sample count used when creating pipelines (1 = disabled).
    pub sample_count: u32,
}

impl GpuContext {
    /// Initialise a headless GPU context (no surface) with sample_count=1.
    /// Useful for testing pipeline compilation without a window.
    pub async fn new_headless() -> Result<Self, RendererError> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .ok_or(RendererError::NoAdapter)?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    required_limits: wgpu::Limits {
                        max_bind_groups: 8,
                        ..wgpu::Limits::default()
                    },
                    ..wgpu::DeviceDescriptor::default()
                },
                None,
            )
            .await
            .map_err(|e| RendererError::DeviceCreation(e.to_string()))?;

        // Verify adapter is not a software fallback (spec T-REND-01)
        let info = adapter.get_info();
        log::info!(
            "wgpu adapter: {} ({:?})",
            info.name,
            info.backend
        );
        if info.device_type == wgpu::DeviceType::Cpu {
            log::warn!("software (CPU) renderer detected — spec requires hardware adapter");
        }

        let surface_format = wgpu::TextureFormat::Bgra8UnormSrgb;
        let sample_count = 1u32;
        let (pipelines, bind_group_layouts) =
            Self::create_pipelines(&device, surface_format, sample_count)?;

        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            pipelines,
            bind_group_layouts,
            surface_format,
            sample_count,
        })
    }

    /// Initialise a GPU context tied to an existing surface.
    /// The adapter is selected to be compatible with the given surface so that
    /// the swapchain format can be queried from surface capabilities.
    pub async fn new_for_surface(
        instance: wgpu::Instance,
        surface: &wgpu::Surface<'_>,
        sample_count: u32,
    ) -> Result<Self, RendererError> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or(RendererError::NoAdapter)?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    required_limits: wgpu::Limits {
                        max_bind_groups: 8,
                        ..wgpu::Limits::default()
                    },
                    ..wgpu::DeviceDescriptor::default()
                },
                None,
            )
            .await
            .map_err(|e| RendererError::DeviceCreation(e.to_string()))?;

        let info = adapter.get_info();
        log::info!("wgpu adapter: {} ({:?})", info.name, info.backend);

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let (pipelines, bind_group_layouts) =
            Self::create_pipelines(&device, surface_format, sample_count)?;

        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            pipelines,
            bind_group_layouts,
            surface_format,
            sample_count,
        })
    }

    /// Initialise a GPU context from externally-owned wgpu handles.
    ///
    /// Used by the editor to share the device/queue already created by eframe/egui-wgpu.
    /// Pipelines are compiled against the given `surface_format`.
    pub fn from_existing(
        instance: wgpu::Instance,
        adapter: wgpu::Adapter,
        device: wgpu::Device,
        queue: wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> Result<Self, RendererError> {
        let info = adapter.get_info();
        log::info!("wgpu adapter (shared): {} ({:?})", info.name, info.backend);

        let (pipelines, bind_group_layouts) =
            Self::create_pipelines(&device, surface_format, sample_count)?;

        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            pipelines,
            bind_group_layouts,
            surface_format,
            sample_count,
        })
    }

    /// Process pending GPU upload requests (called on main thread each render tick).
    ///
    /// Each request carries raw pixel bytes decoded on a background thread; this
    /// function creates the `wgpu::Texture` and fires the on-ready callback.
    pub fn process_uploads(&self, uploads: Vec<GpuUploadRequest>) {
        for req in uploads {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("uploaded texture"),
                size: wgpu::Extent3d {
                    width: req.width,
                    height: req.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });

            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &req.pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * req.width),
                    rows_per_image: Some(req.height),
                },
                wgpu::Extent3d {
                    width: req.width,
                    height: req.height,
                    depth_or_array_layers: 1,
                },
            );

            log::debug!(
                "GPU texture uploaded: {}x{} RGBA8Unorm ({} bytes)",
                req.width,
                req.height,
                req.pixels.len()
            );
            (req.on_ready)(texture);
        }
    }

    /// Render an empty frame (clear only) to the given surface texture.
    pub fn render_clear(
        &self,
        surface_texture: &wgpu::SurfaceTexture,
        clear_color: [u8; 4],
    ) {
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let [r, g, b, a] = clear_color;
        let wgpu_color = wgpu::Color {
            r: r as f64 / 255.0,
            g: g as f64 / 255.0,
            b: b as f64 / 255.0,
            a: a as f64 / 255.0,
        };

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("clear encoder"),
        });

        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("clear pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn create_pipelines(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> Result<(Pipelines, BindGroupLayouts), RendererError> {
        let (primitive, primitive_bgl) = Self::build_pipeline(
            device,
            "primitive",
            PRIMITIVE_WGSL,
            surface_format,
            false,
            sample_count,
        )?;
        let (image, image_bgl) = Self::build_pipeline(
            device,
            "image",
            IMAGE_WGSL,
            surface_format,
            true,
            sample_count,
        )?;
        let (text, text_bgl) = Self::build_pipeline(
            device,
            "text",
            TEXT_WGSL,
            surface_format,
            true,
            sample_count,
        )?;
        let (mesh, mesh_camera_bgl, mesh_model_bgl, mesh_texture_bgl, mesh_normal_map_bgl, mesh_specular_map_bgl, mesh_light_buffer_bgl, mesh_emissive_map_bgl) =
            Self::build_mesh_pipeline(device, surface_format, sample_count)?;

        let pipelines = Pipelines { primitive, image, text, mesh };
        let bind_group_layouts = BindGroupLayouts {
            primitive: primitive_bgl,
            image: image_bgl,
            text: text_bgl,
            mesh_camera: mesh_camera_bgl,
            mesh_model: mesh_model_bgl,
            mesh_texture: mesh_texture_bgl,
            mesh_normal_map: mesh_normal_map_bgl,
            mesh_specular_map: mesh_specular_map_bgl,
            mesh_light_buffer: mesh_light_buffer_bgl,
            mesh_emissive_map: mesh_emissive_map_bgl,
        };
        Ok((pipelines, bind_group_layouts))
    }

    fn build_pipeline(
        device: &wgpu::Device,
        label: &str,
        wgsl: &str,
        format: wgpu::TextureFormat,
        alpha_blend: bool,
        sample_count: u32,
    ) -> Result<(wgpu::RenderPipeline, wgpu::BindGroupLayout), RendererError> {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(label),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(wgsl)),
        });

        let blend = if alpha_blend {
            Some(wgpu::BlendState::ALPHA_BLENDING)
        } else {
            Some(wgpu::BlendState::REPLACE)
        };

        // Build a bind group layout that matches the shader's @group(0) bindings.
        // Non-textured (primitive): binding 0 = uniform buffer.
        // Textured (image/text):    binding 0 = uniform, 1 = texture_2d, 2 = sampler.
        let uniform_entry = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };

        let bgl = if alpha_blend {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries: &[
                    uniform_entry,
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            })
        } else {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries: &[uniform_entry],
            })
        };

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(label),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        Ok((pipeline, bgl))
    }

    #[allow(clippy::type_complexity)]
    fn build_mesh_pipeline(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> Result<(wgpu::RenderPipeline, wgpu::BindGroupLayout, wgpu::BindGroupLayout, wgpu::BindGroupLayout, wgpu::BindGroupLayout, wgpu::BindGroupLayout, wgpu::BindGroupLayout, wgpu::BindGroupLayout), RendererError> {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mesh"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(MESH_WGSL)),
        });

        // group(0): camera uniform (view_proj + eye_position) — vertex AND fragment
        // Fragment needs eye_position to compute the specular view direction.
        let camera_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mesh_camera"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        // group(1): model uniform (model matrix + color + flags) — vertex and fragment
        let model_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mesh_model"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        // group(2): diffuse texture + sampler — fragment-only
        let texture_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mesh_texture"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // group(3): normal map texture + sampler — fragment-only
        let normal_map_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mesh_normal_map"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // group(4): specular map texture + sampler — fragment-only
        let specular_map_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mesh_specular_map"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // group(5): LightBuffer uniform (multi-light) — fragment-only
        let light_buffer_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mesh_light_buffer"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        // group(6): emissive map texture + sampler — fragment-only
        let emissive_map_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mesh_emissive_map"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh"),
            bind_group_layouts: &[
                &camera_bgl,
                &model_bgl,
                &texture_bgl,
                &normal_map_bgl,
                &specular_map_bgl,
                &light_buffer_bgl,
                &emissive_map_bgl,
            ],
            push_constant_ranges: &[],
        });

        let vertex_attrs = wgpu::vertex_attr_array![
            0 => Float32x3, // position   (offset  0, 12 B)
            1 => Float32x3, // normal     (offset 12, 12 B)
            2 => Float32x2, // uv         (offset 24,  8 B)
            3 => Float32x3, // tangent    (offset 32, 12 B)
            4 => Float32x3, // bitangent  (offset 44, 12 B)
            // _pad [f32;2] at offset 56 — not read by shader, stride accounts for it
        ];

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mesh"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &vertex_attrs,
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        Ok((pipeline, camera_bgl, model_bgl, texture_bgl, normal_map_bgl, specular_map_bgl, light_buffer_bgl, emissive_map_bgl))
    }
}

/// GPU vertex and index buffers for one cached mesh.
pub struct MeshBuffers {
    pub vertex_buf: wgpu::Buffer,
    pub index_buf: wgpu::Buffer,
    pub index_count: u32,
}

/// Uniform layouts used internally by the mesh render dispatch.
// CameraUniform: 80 bytes — must match MESH_WGSL CameraUniforms layout exactly.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    view_proj:    [[f32; 4]; 4],  // 64 B [0-63]
    eye_position: [f32; 3],       // 12 B [64-75]  world-space camera position
    _pad:         f32,            //  4 B [76-79]
}

// ModelUniform: 144 bytes — must match MESH_WGSL ModelUniforms layout exactly.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ModelUniform {
    model:            [[f32; 4]; 4],  // 64 B [0-63]
    color:            [f32; 4],       // 16 B [64-79]
    specular_color:   [f32; 4],       // 16 B [80-95]   xyz=tint, w=unused
    emissive_color:   [f32; 4],       // 16 B [96-111]  xyz=emissive RGB, w=intensity
    has_texture:      u32,            //  4 B [112-115]
    has_normal_map:   u32,            //  4 B [116-119]
    has_specular_map: u32,            //  4 B [120-123]
    has_emissive_map: u32,            //  4 B [124-127]
    metallic:         f32,            //  4 B [128-131]
    roughness:        f32,            //  4 B [132-135]
    shininess:        f32,            //  4 B [136-139]
    _pad0:            u32,            //  4 B [140-143]
}

/// Primitive (rect/circle/line) uniform — matches PRIMITIVE_WGSL layout (48 bytes).
///
///   0–15:  rect: vec4<f32>   — clip-space (x, y, w, h)
///  16–31:  color: vec4<f32>  — RGBA 0.0–1.0
///  32–35:  mode: u32         — 0=rect_fill
///  36–47:  _pad: [u32; 3]
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PrimitiveUniform {
    rect: [f32; 4],
    color: [f32; 4],
    mode: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

/// Text glyph uniform — matches TEXT_WGSL layout (48 bytes).
///
///   0–15:  rect: vec4<f32>    — clip-space (x, y, w, h)
///  16–31:  uv_rect: vec4<f32> — atlas UV region (u0, v0, u1, v1)
///  32–47:  color: vec4<f32>   — RGBA 0.0–1.0
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct TextUniform {
    rect: [f32; 4],
    uv_rect: [f32; 4],
    color: [f32; 4],
}

// ── Glyph atlas ───────────────────────────────────────────────────────────────

const ATLAS_SIZE: u32 = 512;

/// Cached rasterized glyph entry in the atlas.
#[derive(Clone, Copy)]
struct GlyphEntry {
    /// Normalized UV rect [u0, v0, u1, v1] within the 512×512 atlas.
    uv_rect: [f32; 4],
    /// Horizontal advance in pixels.
    advance_width: f32,
    /// Rendered glyph width in pixels (0 for invisible glyphs like space).
    width: u32,
    /// Rendered glyph height in pixels.
    height: u32,
}

/// 512×512 R8Unorm glyph atlas with on-demand fontdue rasterization.
struct GlyphAtlas {
    font: fontdue::Font,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    cache: HashMap<(char, u32), GlyphEntry>,
    next_x: u32,
    next_y: u32,
    row_height: u32,
}

impl GlyphAtlas {
    /// Try to create an atlas by loading a system font. Returns None if no font is found.
    fn try_new(device: &wgpu::Device) -> Option<Self> {
        let font_paths = [
            "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
            "/usr/share/fonts/dejavu/DejaVuSansMono.ttf",
            "/usr/share/fonts/noto/NotoSansMono-Regular.ttf",
            "/usr/share/fonts/noto/NotoSans-Regular.ttf",
        ];

        let font_bytes = font_paths.iter().find_map(|p| {
            std::fs::read(p).ok().inspect(|_b| { log::info!("GlyphAtlas: loaded font {}", p); })
        });

        let font_bytes = match font_bytes {
            Some(b) => b,
            None => {
                log::warn!("GlyphAtlas: no system font found; text rendering disabled");
                return None;
            }
        };

        let font = match fontdue::Font::from_bytes(
            font_bytes.as_slice(),
            fontdue::FontSettings::default(),
        ) {
            Ok(f) => f,
            Err(e) => {
                log::warn!("GlyphAtlas: font parse error: {}; text rendering disabled", e);
                return None;
            }
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph_atlas"),
            size: wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("glyph_atlas_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        Some(Self {
            font,
            texture,
            view,
            sampler,
            cache: HashMap::new(),
            next_x: 0,
            next_y: 0,
            row_height: 0,
        })
    }

    /// Return a cached `GlyphEntry`, rasterizing into the atlas on first use.
    fn get_or_rasterize(&mut self, ch: char, size_px: u32, queue: &wgpu::Queue) -> GlyphEntry {
        if let Some(entry) = self.cache.get(&(ch, size_px)) {
            return *entry;
        }

        let (metrics, bitmap) = self.font.rasterize(ch, size_px as f32);

        // Invisible glyph (space, control character, etc.)
        if bitmap.is_empty() || metrics.width == 0 || metrics.height == 0 {
            let entry = GlyphEntry {
                uv_rect: [0.0; 4],
                advance_width: metrics.advance_width,
                width: 0,
                height: 0,
            };
            self.cache.insert((ch, size_px), entry);
            return entry;
        }

        let gw = metrics.width as u32;
        let gh = metrics.height as u32;

        // Row-pack: advance to next row if glyph doesn't fit on current row
        if self.next_x + gw > ATLAS_SIZE {
            self.next_y += self.row_height + 1;
            self.next_x = 0;
            self.row_height = 0;
        }

        // If atlas is full, return an invisible entry
        if self.next_y + gh > ATLAS_SIZE {
            log::warn!("GlyphAtlas: atlas full, skipping glyph '{}'", ch);
            let entry = GlyphEntry {
                uv_rect: [0.0; 4],
                advance_width: metrics.advance_width,
                width: 0,
                height: 0,
            };
            self.cache.insert((ch, size_px), entry);
            return entry;
        }

        let ax = self.next_x;
        let ay = self.next_y;

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: ax, y: ay, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &bitmap,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(gw),
                rows_per_image: Some(gh),
            },
            wgpu::Extent3d { width: gw, height: gh, depth_or_array_layers: 1 },
        );

        if gh > self.row_height {
            self.row_height = gh;
        }

        let u0 = ax as f32 / ATLAS_SIZE as f32;
        let v0 = ay as f32 / ATLAS_SIZE as f32;
        let u1 = (ax + gw) as f32 / ATLAS_SIZE as f32;
        let v1 = (ay + gh) as f32 / ATLAS_SIZE as f32;

        self.next_x += gw + 1;

        let entry = GlyphEntry {
            uv_rect: [u0, v0, u1, v1],
            advance_width: metrics.advance_width,
            width: gw,
            height: gh,
        };
        self.cache.insert((ch, size_px), entry);
        entry
    }
}

// ── RendererState ─────────────────────────────────────────────────────────────

/// Renderer configuration extended with runtime GPU state.
pub struct RendererState {
    pub gpu: GpuContext,
    pub config: RendererConfig,
    /// Runtime scene settings (clear color, directional light) — updated from Python each frame.
    pub scene_settings: SceneSettings,
    /// Cached GPU mesh buffers keyed by mesh_id.
    pub mesh_cache: HashMap<String, MeshBuffers>,
    /// Cached depth texture (Depth32Float) and its current dimensions.
    depth_texture: Option<(wgpu::Texture, wgpu::TextureView, u32, u32)>,
    /// MSAA resolve texture and its current dimensions + format.
    msaa_texture: Option<(wgpu::Texture, wgpu::TextureView, u32, u32, wgpu::TextureFormat)>,
    /// Lazily-initialized glyph atlas for text rendering.
    glyph_atlas: Option<GlyphAtlas>,
    /// Cached texture bind groups keyed by file path.
    texture_cache: HashMap<String, wgpu::BindGroup>,
    /// 1×1 white fallback bind group used when no diffuse texture is specified.
    fallback_texture_bg: wgpu::BindGroup,
    /// Cached normal map bind groups keyed by file path.
    normal_map_cache: HashMap<String, wgpu::BindGroup>,
    /// 1×1 flat-normal fallback bind group (RGB=127,127,255) used when normal_map_id=None or missing.
    fallback_normal_map_bg: wgpu::BindGroup,
    /// Cached specular map bind groups keyed by file path.
    specular_map_cache: HashMap<String, wgpu::BindGroup>,
    /// 1×1 fallback specular bind group (R=255 full intensity, G=128 mid-gloss) used when specular_map_id=None or missing.
    fallback_specular_map_bg: wgpu::BindGroup,
    /// Cached emissive map bind groups keyed by file path.
    emissive_map_cache: HashMap<String, wgpu::BindGroup>,
    /// 1×1 black fallback emissive bind group used when emissive_map_id=None or missing.
    fallback_emissive_map_bg: wgpu::BindGroup,
}

impl RendererState {
    /// Construct a new RendererState with an empty mesh cache and no depth texture.
    pub fn new(gpu: GpuContext, config: RendererConfig) -> Self {
        // Create 1×1 white fallback texture used for untextured meshes.
        let fallback_pixel: [u8; 4] = [255, 255, 255, 255];
        let fallback_tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("fallback_white"),
            size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &fallback_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &fallback_pixel,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        );
        let fallback_view = fallback_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let fallback_sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("fallback_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let fallback_texture_bg = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fallback_tex_bg"),
            layout: &gpu.bind_group_layouts.mesh_texture,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&fallback_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&fallback_sampler),
                },
            ],
        });

        // Create 1×1 flat-normal fallback texture (127, 127, 255) used when no normal map is set.
        // This encodes the tangent-space up vector (0, 0, 1) which leaves normals unchanged.
        let flat_normal_pixel: [u8; 4] = [127, 127, 255, 255];
        let flat_normal_tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("fallback_flat_normal"),
            size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &flat_normal_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &flat_normal_pixel,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        );
        let flat_normal_view = flat_normal_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let flat_normal_sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("fallback_normal_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let fallback_normal_map_bg = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fallback_normal_map_bg"),
            layout: &gpu.bind_group_layouts.mesh_normal_map,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&flat_normal_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&flat_normal_sampler),
                },
            ],
        });

        // Create 1×1 fallback specular texture: R=255 (full intensity), G=128 (mid-gloss).
        let fallback_specular_pixel: [u8; 4] = [255, 128, 0, 255];
        let fallback_specular_tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("fallback_specular"),
            size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &fallback_specular_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &fallback_specular_pixel,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        );
        let fallback_specular_view = fallback_specular_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let fallback_specular_sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("fallback_specular_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let fallback_specular_map_bg = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fallback_specular_map_bg"),
            layout: &gpu.bind_group_layouts.mesh_specular_map,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&fallback_specular_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&fallback_specular_sampler),
                },
            ],
        });

        // Create 1×1 black fallback emissive texture (0, 0, 0, 255) used when emissive_map_id=None.
        let fallback_emissive_pixel: [u8; 4] = [0, 0, 0, 255];
        let fallback_emissive_tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("fallback_emissive"),
            size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &fallback_emissive_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &fallback_emissive_pixel,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        );
        let fallback_emissive_view = fallback_emissive_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let fallback_emissive_sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("fallback_emissive_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let fallback_emissive_map_bg = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fallback_emissive_map_bg"),
            layout: &gpu.bind_group_layouts.mesh_emissive_map,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&fallback_emissive_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&fallback_emissive_sampler),
                },
            ],
        });

        Self {
            gpu,
            config,
            scene_settings: SceneSettings::default(),
            mesh_cache: HashMap::new(),
            depth_texture: None,
            msaa_texture: None,
            glyph_atlas: None,
            texture_cache: HashMap::new(),
            fallback_texture_bg,
            normal_map_cache: HashMap::new(),
            fallback_normal_map_bg,
            specular_map_cache: HashMap::new(),
            fallback_specular_map_bg,
            emissive_map_cache: HashMap::new(),
            fallback_emissive_map_bg,
        }
    }

    /// Clear color from `scene_settings` as a wgpu::Color (linear f32 → f64).
    pub fn clear_color_wgpu(&self) -> wgpu::Color {
        let [r, g, b, a] = self.scene_settings.clear_color;
        wgpu::Color {
            r: r as f64,
            g: g as f64,
            b: b as f64,
            a: a as f64,
        }
    }

    /// Upload a mesh to the GPU buffer cache.
    ///
    /// `vertices_bytes` must be the raw bytes of a `&[Vertex]` cast via bytemuck.
    /// Replaces any previously cached mesh with the same `mesh_id`.
    pub fn upload_mesh(&mut self, mesh_id: &str, vertices_bytes: &[u8], indices: &[u32]) {
        let vertex_buf = self.gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(mesh_id),
            size: vertices_bytes.len() as u64,
            usage: wgpu::BufferUsages::VERTEX,
            mapped_at_creation: true,
        });
        vertex_buf.slice(..).get_mapped_range_mut().copy_from_slice(vertices_bytes);
        vertex_buf.unmap();

        let index_bytes: &[u8] = bytemuck::cast_slice(indices);
        let index_buf = self.gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(mesh_id),
            size: index_bytes.len() as u64,
            usage: wgpu::BufferUsages::INDEX,
            mapped_at_creation: true,
        });
        index_buf.slice(..).get_mapped_range_mut().copy_from_slice(index_bytes);
        index_buf.unmap();

        self.mesh_cache.insert(
            mesh_id.to_string(),
            MeshBuffers { vertex_buf, index_buf, index_count: indices.len() as u32 },
        );
        log::debug!("mesh uploaded: '{}' ({} verts, {} indices)", mesh_id,
            vertices_bytes.len() / 64, indices.len());
    }

    /// Load a PNG texture from disk into the texture cache if not already loaded.
    /// Silently skips empty paths or paths that fail to load (logs a warning).
    fn ensure_texture_loaded(&mut self, texture_id: &str) {
        if texture_id.is_empty() || self.texture_cache.contains_key(texture_id) {
            return;
        }
        match image::open(texture_id) {
            Ok(img) => {
                let rgba = img.to_rgba8();
                let (w, h) = (rgba.width(), rgba.height());
                let tex = self.gpu.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some(texture_id),
                    size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                self.gpu.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &rgba,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(4 * w),
                        rows_per_image: Some(h),
                    },
                    wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                );
                let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
                let sampler = self.gpu.device.create_sampler(&wgpu::SamplerDescriptor {
                    label: Some("tex_sampler"),
                    address_mode_u: wgpu::AddressMode::Repeat,
                    address_mode_v: wgpu::AddressMode::Repeat,
                    mag_filter: wgpu::FilterMode::Linear,
                    min_filter: wgpu::FilterMode::Linear,
                    ..Default::default()
                });
                let bg = self.gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("mesh_tex_bg"),
                    layout: &self.gpu.bind_group_layouts.mesh_texture,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                    ],
                });
                log::debug!("texture loaded: '{}' ({}x{})", texture_id, w, h);
                self.texture_cache.insert(texture_id.to_string(), bg);
            }
            Err(e) => {
                log::warn!("failed to load texture '{}': {}", texture_id, e);
            }
        }
    }

    /// Load a PNG normal map from disk into the normal_map_cache if not already loaded.
    /// Falls back to the flat-normal texture on missing files (logs a warning).
    fn ensure_normal_map_loaded(&mut self, normal_map_id: &str) {
        if normal_map_id.is_empty() || self.normal_map_cache.contains_key(normal_map_id) {
            return;
        }
        match image::open(normal_map_id) {
            Ok(img) => {
                let rgba = img.to_rgba8();
                let (w, h) = (rgba.width(), rgba.height());
                let tex = self.gpu.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some(normal_map_id),
                    size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                self.gpu.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &rgba,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(4 * w),
                        rows_per_image: Some(h),
                    },
                    wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                );
                let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
                let sampler = self.gpu.device.create_sampler(&wgpu::SamplerDescriptor {
                    label: Some("normal_map_sampler"),
                    address_mode_u: wgpu::AddressMode::Repeat,
                    address_mode_v: wgpu::AddressMode::Repeat,
                    mag_filter: wgpu::FilterMode::Linear,
                    min_filter: wgpu::FilterMode::Linear,
                    ..Default::default()
                });
                let bg = self.gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("normal_map_bg"),
                    layout: &self.gpu.bind_group_layouts.mesh_normal_map,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                    ],
                });
                log::debug!("normal map loaded: '{}' ({}x{})", normal_map_id, w, h);
                self.normal_map_cache.insert(normal_map_id.to_string(), bg);
            }
            Err(e) => {
                log::warn!("failed to load normal map '{}': {} — using flat-normal fallback", normal_map_id, e);
            }
        }
    }

    /// Load a PNG specular map from disk into the specular_map_cache if not already loaded.
    /// Falls back to the fallback specular texture on missing files (logs a warning).
    fn ensure_specular_map_loaded(&mut self, specular_map_id: &str) {
        if specular_map_id.is_empty() || self.specular_map_cache.contains_key(specular_map_id) {
            return;
        }
        match image::open(specular_map_id) {
            Ok(img) => {
                let rgba = img.to_rgba8();
                let (w, h) = (rgba.width(), rgba.height());
                let tex = self.gpu.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some(specular_map_id),
                    size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                self.gpu.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &rgba,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(4 * w),
                        rows_per_image: Some(h),
                    },
                    wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                );
                let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
                let sampler = self.gpu.device.create_sampler(&wgpu::SamplerDescriptor {
                    label: Some("specular_map_sampler"),
                    address_mode_u: wgpu::AddressMode::Repeat,
                    address_mode_v: wgpu::AddressMode::Repeat,
                    mag_filter: wgpu::FilterMode::Linear,
                    min_filter: wgpu::FilterMode::Linear,
                    ..Default::default()
                });
                let bg = self.gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("specular_map_bg"),
                    layout: &self.gpu.bind_group_layouts.mesh_specular_map,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                    ],
                });
                log::debug!("specular map loaded: '{}' ({}x{})", specular_map_id, w, h);
                self.specular_map_cache.insert(specular_map_id.to_string(), bg);
            }
            Err(e) => {
                log::warn!("failed to load specular map '{}': {} — using fallback specular", specular_map_id, e);
            }
        }
    }

    /// Load a PNG emissive map from disk into the emissive_map_cache if not already loaded.
    /// Falls back to the black fallback texture on missing files (logs a warning).
    fn ensure_emissive_map_loaded(&mut self, emissive_map_id: &str) {
        if emissive_map_id.is_empty() || self.emissive_map_cache.contains_key(emissive_map_id) {
            return;
        }
        match image::open(emissive_map_id) {
            Ok(img) => {
                let rgba = img.to_rgba8();
                let (w, h) = (rgba.width(), rgba.height());
                let tex = self.gpu.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some(emissive_map_id),
                    size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                self.gpu.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &rgba,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(4 * w),
                        rows_per_image: Some(h),
                    },
                    wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                );
                let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
                let sampler = self.gpu.device.create_sampler(&wgpu::SamplerDescriptor {
                    label: Some("emissive_map_sampler"),
                    address_mode_u: wgpu::AddressMode::Repeat,
                    address_mode_v: wgpu::AddressMode::Repeat,
                    mag_filter: wgpu::FilterMode::Linear,
                    min_filter: wgpu::FilterMode::Linear,
                    ..Default::default()
                });
                let bg = self.gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("emissive_map_bg"),
                    layout: &self.gpu.bind_group_layouts.mesh_emissive_map,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                    ],
                });
                log::debug!("emissive map loaded: '{}' ({}x{})", emissive_map_id, w, h);
                self.emissive_map_cache.insert(emissive_map_id.to_string(), bg);
            }
            Err(e) => {
                log::warn!("failed to load emissive map '{}': {} — using black fallback", emissive_map_id, e);
            }
        }
    }

    /// Ensure a Depth32Float texture of the given dimensions exists, recreating
    /// it when the surface has been resized.
    pub fn ensure_depth_texture(&mut self, width: u32, height: u32) {
        let needs_new = self.depth_texture
            .as_ref()
            .is_none_or(|&(_, _, w, h)| w != width || h != height);

        if needs_new {
            let tex = self.gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("depth"),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: self.gpu.sample_count,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
            self.depth_texture = Some((tex, view, width, height));
            log::debug!("depth texture created: {}×{} (sample_count={})", width, height, self.gpu.sample_count);
        }
    }

    /// Returns a reference to the depth texture view, if one has been created.
    pub fn depth_view(&self) -> Option<&wgpu::TextureView> {
        self.depth_texture.as_ref().map(|(_, view, _, _)| view)
    }

    /// Ensure an MSAA resolve texture of the given dimensions and format exists.
    /// Only creates a texture when `gpu.sample_count > 1`.
    pub fn ensure_msaa_texture(&mut self, width: u32, height: u32, format: wgpu::TextureFormat) {
        if self.gpu.sample_count <= 1 {
            return;
        }
        let needs_new = self.msaa_texture
            .as_ref()
            .is_none_or(|&(_, _, w, h, f)| w != width || h != height || f != format);

        if needs_new {
            let tex = self.gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("msaa"),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: self.gpu.sample_count,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
            self.msaa_texture = Some((tex, view, width, height, format));
            log::debug!("MSAA texture created: {}×{} ({}x)", width, height, self.gpu.sample_count);
        }
    }

    /// Returns a reference to the MSAA texture view, if one has been created.
    pub fn msaa_view(&self) -> Option<&wgpu::TextureView> {
        self.msaa_texture.as_ref().map(|(_, view, _, _, _)| view)
    }

    /// Render a batch of `DrawMesh` commands using the mesh pipeline.
    ///
    /// Each command is looked up in the mesh cache; commands whose `mesh_id` has
    /// not been uploaded are silently skipped.  Caller must call
    /// [`ensure_depth_texture`] before this so the depth texture exists.
    ///
    /// `light_buffer`: when `Some`, the provided `LightBuffer` is uploaded to bind group 5.
    /// When `None`, a fallback directional light is built from `scene_settings`.
    pub fn render_meshes(
        &mut self,
        commands: &[DrawMesh],
        camera: &Camera,
        color_view: &wgpu::TextureView,
        light_buffer: Option<&LightBuffer>,
    ) {
        if commands.is_empty() {
            return;
        }

        // Pre-load all textures referenced by this batch.
        for cmd in commands {
            self.ensure_texture_loaded(&cmd.texture_id);
            if let Some(ref nm_id) = cmd.normal_map_id {
                self.ensure_normal_map_loaded(nm_id);
            }
            if let Some(ref sm_id) = cmd.specular_map_id {
                self.ensure_specular_map_loaded(sm_id);
            }
            if let Some(ref em_id) = cmd.emissive_map_id {
                self.ensure_emissive_map_loaded(em_id);
            }
        }

        // Access depth texture view (must have been created by ensure_depth_texture).
        let Some((_, ref depth_view, _, _)) = self.depth_texture else {
            log::warn!("render_meshes: no depth texture — call ensure_depth_texture first");
            return;
        };

        // Determine MSAA attachment vs resolve target
        let (mesh_color_view, resolve_target): (&wgpu::TextureView, Option<&wgpu::TextureView>) =
            if self.gpu.sample_count > 1 {
                match self.msaa_texture.as_ref() {
                    Some((_, ref mv, ..)) => (mv, Some(color_view)),
                    None => (color_view, None),
                }
            } else {
                (color_view, None)
            };

        // Camera uniform — shared across all mesh draws in this batch.
        let cam_uniform = CameraUniform {
            view_proj: camera.view_projection().to_cols_array_2d(),
            eye_position: [camera.position.x, camera.position.y, camera.position.z],
            _pad: 0.0,
        };
        let cam_bytes: &[u8] = bytemuck::bytes_of(&cam_uniform);
        let cam_buf = self.gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cam_uniform"),
            size: cam_bytes.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: true,
        });
        cam_buf.slice(..).get_mapped_range_mut().copy_from_slice(cam_bytes);
        cam_buf.unmap();

        let cam_bg = self.gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cam_bg"),
            layout: &self.gpu.bind_group_layouts.mesh_camera,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: cam_buf.as_entire_binding(),
            }],
        });

        // Light buffer — shared across all mesh draws in this batch.
        // Use provided LightBuffer, or build a fallback from scene_settings.
        let light_bg = {
            let lb: LightBuffer = if let Some(provided) = light_buffer {
                *provided
            } else {
                let [lx, ly, lz] = self.scene_settings.light_direction;
                let len = (lx * lx + ly * ly + lz * lz).sqrt();
                let (nx, ny, nz) = if len > 1e-6 {
                    (lx / len, ly / len, lz / len)
                } else {
                    (0.0, 1.0, 0.0)
                };
                let [cr, cg, cb] = self.scene_settings.light_color;
                let [ar, ag, ab] = self.scene_settings.ambient_color;
                let ai = self.scene_settings.ambient_intensity;
                let mut fallback = LightBuffer::empty();
                fallback.ambient = [ar * ai, ag * ai, ab * ai];
                fallback.lights[0] = GpuLight {
                    position_or_dir: [nx, ny, nz, 0.0],
                    color_intensity:  [cr, cg, cb, self.scene_settings.light_intensity],
                    spot_params:      [0.0, 0.0, 0.0, 1.0],
                    spot_dir_pad:     [0.0; 4],
                };
                fallback.light_count = 1;
                fallback
            };
            let lb_bytes: &[u8] = bytemuck::bytes_of(&lb);
            let lb_buf = self.gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("light_buffer"),
                size: lb_bytes.len() as u64,
                usage: wgpu::BufferUsages::UNIFORM,
                mapped_at_creation: true,
            });
            lb_buf.slice(..).get_mapped_range_mut().copy_from_slice(lb_bytes);
            lb_buf.unmap();
            self.gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("light_buf_bg"),
                layout: &self.gpu.bind_group_layouts.mesh_light_buffer,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: lb_buf.as_entire_binding(),
                }],
            })
        };

        let mut encoder = self.gpu.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("mesh encoder") },
        );

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mesh pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: mesh_color_view,
                    resolve_target,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(&self.gpu.pipelines.mesh);
            pass.set_bind_group(0, &cam_bg, &[]);
            pass.set_bind_group(5, &light_bg, &[]);

            for cmd in commands {
                let Some(mesh) = self.mesh_cache.get(&cmd.mesh_id) else {
                    log::warn!("render_meshes: mesh '{}' not in cache — skipped", cmd.mesh_id);
                    continue;
                };

                let has_tex = if cmd.texture_id.is_empty() { 0u32 } else { 1u32 };
                let has_normal_map = if cmd.normal_map_id.is_some() { 1u32 } else { 0u32 };
                let has_specular_map = if cmd.specular_map_id.is_some() { 1u32 } else { 0u32 };
                let has_emissive_map = if cmd.emissive_map_id.is_some() { 1u32 } else { 0u32 };
                let [sr, sg, sb] = cmd.specular_color;
                let [er, eg, eb, _] = cmd.emissive_color;
                let model_uniform = ModelUniform {
                    model: cmd.transform.to_cols_array_2d(),
                    color: [1.0, 1.0, 1.0, 1.0],
                    specular_color: [sr, sg, sb, 0.0],
                    emissive_color: [er, eg, eb, cmd.emissive_intensity],
                    has_texture: has_tex,
                    has_normal_map,
                    has_specular_map,
                    has_emissive_map,
                    metallic: cmd.metallic.clamp(0.0, 1.0),
                    roughness: cmd.roughness.clamp(0.0, 1.0),
                    shininess: cmd.shininess,
                    _pad0: 0,
                };
                let model_bytes: &[u8] = bytemuck::bytes_of(&model_uniform);
                let model_buf = self.gpu.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("model_uniform"),
                    size: model_bytes.len() as u64,
                    usage: wgpu::BufferUsages::UNIFORM,
                    mapped_at_creation: true,
                });
                model_buf.slice(..).get_mapped_range_mut().copy_from_slice(model_bytes);
                model_buf.unmap();

                let model_bg = self.gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("model_bg"),
                    layout: &self.gpu.bind_group_layouts.mesh_model,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: model_buf.as_entire_binding(),
                    }],
                });

                let tex_bg = if cmd.texture_id.is_empty() {
                    &self.fallback_texture_bg
                } else {
                    self.texture_cache
                        .get(&cmd.texture_id)
                        .unwrap_or(&self.fallback_texture_bg)
                };

                let normal_map_bg = match &cmd.normal_map_id {
                    Some(nm_id) => {
                        self.normal_map_cache
                            .get(nm_id.as_str())
                            .unwrap_or(&self.fallback_normal_map_bg)
                    }
                    None => &self.fallback_normal_map_bg,
                };

                let specular_map_bg = match &cmd.specular_map_id {
                    Some(sm_id) => {
                        self.specular_map_cache
                            .get(sm_id.as_str())
                            .unwrap_or(&self.fallback_specular_map_bg)
                    }
                    None => &self.fallback_specular_map_bg,
                };

                let emissive_map_bg = match &cmd.emissive_map_id {
                    Some(em_id) => {
                        self.emissive_map_cache
                            .get(em_id.as_str())
                            .unwrap_or(&self.fallback_emissive_map_bg)
                    }
                    None => &self.fallback_emissive_map_bg,
                };

                pass.set_bind_group(1, &model_bg, &[]);
                pass.set_bind_group(2, tex_bg, &[]);
                pass.set_bind_group(3, normal_map_bg, &[]);
                pass.set_bind_group(4, specular_map_bg, &[]);
                pass.set_bind_group(6, emissive_map_bg, &[]);
                pass.set_vertex_buffer(0, mesh.vertex_buf.slice(..));
                pass.set_index_buffer(mesh.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            }
        }

        self.gpu.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render a batch of `DrawText` commands using the text pipeline and glyph atlas.
    ///
    /// Glyphs are rasterized on demand via fontdue and cached in a 512×512 R8Unorm
    /// atlas texture.  Each glyph is rendered as a full-screen-space quad.
    pub fn render_text(
        &mut self,
        commands: &[DrawText],
        color_view: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) {
        if commands.is_empty() {
            return;
        }

        // Lazily initialize the glyph atlas
        if self.glyph_atlas.is_none() {
            self.glyph_atlas = GlyphAtlas::try_new(&self.gpu.device);
        }
        if self.glyph_atlas.is_none() {
            return; // no font available
        }

        // Phase 1: rasterize all needed glyphs and collect TextUniforms.
        // This uses &mut self.glyph_atlas and &self.gpu.queue (different fields).
        let mut draws: Vec<TextUniform> = Vec::new();
        {
            let queue = &self.gpu.queue;
            let atlas = self.glyph_atlas.as_mut().unwrap();
            for cmd in commands {
                let size_px = cmd.size.max(1);
                let mut cursor_x = cmd.x;
                let color = cmd.color.to_linear();
                for ch in cmd.text.chars() {
                    let entry = atlas.get_or_rasterize(ch, size_px, queue);
                    if entry.width == 0 {
                        // Invisible glyph — advance cursor only
                        cursor_x += entry.advance_width / width as f32;
                        continue;
                    }
                    let clip_w = entry.advance_width / width as f32 * 2.0;
                    let clip_h = -(entry.height as f32 / height as f32 * 2.0);
                    // norm_to_clip: clip_x = nx*2-1, clip_y = 1-ny*2
                    let clip_x = cursor_x * 2.0 - 1.0;
                    let clip_y = 1.0 - cmd.y * 2.0;
                    draws.push(TextUniform {
                        rect: [clip_x, clip_y, clip_w, clip_h],
                        uv_rect: entry.uv_rect,
                        color,
                    });
                    cursor_x += entry.advance_width / width as f32;
                }
            }
        }

        if draws.is_empty() {
            return;
        }

        // Phase 2: build render pass.
        // Determine MSAA attachment vs resolve target (same logic as render_meshes).
        let (text_color_view, resolve_target): (&wgpu::TextureView, Option<&wgpu::TextureView>) =
            if self.gpu.sample_count > 1 {
                match self.msaa_texture.as_ref() {
                    Some((_, ref mv, ..)) => (mv, Some(color_view)),
                    None => (color_view, None),
                }
            } else {
                (color_view, None)
            };

        let atlas_view = &self.glyph_atlas.as_ref().unwrap().view;
        let atlas_sampler = &self.glyph_atlas.as_ref().unwrap().sampler;

        let mut encoder = self.gpu.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("text encoder") },
        );

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("text pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: text_color_view,
                    resolve_target,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(&self.gpu.pipelines.text);

            for uniform in &draws {
                let bytes: &[u8] = bytemuck::bytes_of(uniform);
                let ubuf = self.gpu.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("text_uniform"),
                    size: bytes.len() as u64,
                    usage: wgpu::BufferUsages::UNIFORM,
                    mapped_at_creation: true,
                });
                ubuf.slice(..).get_mapped_range_mut().copy_from_slice(bytes);
                ubuf.unmap();

                let bg = self.gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("text_bg"),
                    layout: &self.gpu.bind_group_layouts.text,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: ubuf.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(atlas_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(atlas_sampler),
                        },
                    ],
                });

                pass.set_bind_group(0, &bg, &[]);
                pass.draw(0..6, 0..1);
            }
        }

        self.gpu.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render a batch of `DrawRect` commands using the primitive pipeline.
    ///
    /// Each filled rect is drawn as a clip-space quad (6 vertices, no index buffer).
    /// If `rect.border` is `Some`, four thin border quads are drawn over the fill.
    ///
    /// MSAA handling mirrors `render_text`: when `gpu.sample_count > 1` and an MSAA
    /// texture is available, it is used as the render attachment and `color_view`
    /// becomes the resolve target.
    pub fn render_rects(
        &self,
        rects: &[DrawRect],
        color_view: &wgpu::TextureView,
        _width: u32,
        _height: u32,
    ) {
        if rects.is_empty() {
            return;
        }

        let (att_view, resolve_target): (&wgpu::TextureView, Option<&wgpu::TextureView>) =
            if self.gpu.sample_count > 1 {
                match self.msaa_texture.as_ref() {
                    Some((_, ref mv, ..)) => (mv, Some(color_view)),
                    None => (color_view, None),
                }
            } else {
                (color_view, None)
            };

        // Build all primitive uniforms: one fill quad per rect, plus up to 4 border quads.
        let mut draws: Vec<PrimitiveUniform> = Vec::new();
        for rect in rects {
            let clip_x = rect.x * 2.0 - 1.0;
            let clip_y = 1.0 - rect.y * 2.0;
            let clip_w = rect.w * 2.0;
            let clip_h = -(rect.h * 2.0);
            draws.push(PrimitiveUniform {
                rect: [clip_x, clip_y, clip_w, clip_h],
                color: rect.color.to_linear(),
                mode: 0,
                _pad0: 0,
                _pad1: 0,
                _pad2: 0,
            });

            if let Some(border_color) = rect.border {
                let bw = rect.border_width;
                let bc = border_color.to_linear();
                let border_rects = [
                    (rect.x, rect.y, rect.w, bw),                         // top
                    (rect.x, rect.y + rect.h - bw, rect.w, bw),           // bottom
                    (rect.x, rect.y, bw, rect.h),                          // left
                    (rect.x + rect.w - bw, rect.y, bw, rect.h),           // right
                ];
                for (bx, by, bw2, bh) in border_rects {
                    draws.push(PrimitiveUniform {
                        rect: [bx * 2.0 - 1.0, 1.0 - by * 2.0, bw2 * 2.0, -(bh * 2.0)],
                        color: bc,
                        mode: 0,
                        _pad0: 0,
                        _pad1: 0,
                        _pad2: 0,
                    });
                }
            }
        }

        let mut encoder = self.gpu.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("rect encoder") },
        );

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rect pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: att_view,
                    resolve_target,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(&self.gpu.pipelines.primitive);

            for uniform in &draws {
                let bytes: &[u8] = bytemuck::bytes_of(uniform);
                let ubuf = self.gpu.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("rect_uniform"),
                    size: bytes.len() as u64,
                    usage: wgpu::BufferUsages::UNIFORM,
                    mapped_at_creation: true,
                });
                ubuf.slice(..).get_mapped_range_mut().copy_from_slice(bytes);
                ubuf.unmap();

                let bg = self.gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("rect_bg"),
                    layout: &self.gpu.bind_group_layouts.primitive,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: ubuf.as_entire_binding(),
                    }],
                });

                pass.set_bind_group(0, &bg, &[]);
                pass.draw(0..6, 0..1);
            }
        }

        self.gpu.queue.submit(std::iter::once(encoder.finish()));
    }
}

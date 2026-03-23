use crate::config::RendererConfig;
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
}

impl GpuContext {
    /// Initialise a headless GPU context (no surface).  Useful for testing
    /// pipeline compilation without a window.
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
            .request_device(&wgpu::DeviceDescriptor::default(), None)
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
        let (pipelines, bind_group_layouts) = Self::create_pipelines(&device, surface_format)?;

        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            pipelines,
            bind_group_layouts,
            surface_format,
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
    ) -> Result<(Pipelines, BindGroupLayouts), RendererError> {
        let (primitive, primitive_bgl) = Self::build_pipeline(
            device,
            "primitive",
            PRIMITIVE_WGSL,
            surface_format,
            false,
        )?;
        let (image, image_bgl) = Self::build_pipeline(
            device,
            "image",
            IMAGE_WGSL,
            surface_format,
            true,
        )?;
        let (text, text_bgl) = Self::build_pipeline(
            device,
            "text",
            TEXT_WGSL,
            surface_format,
            true,
        )?;
        let (mesh, mesh_camera_bgl, mesh_model_bgl) =
            Self::build_mesh_pipeline(device, surface_format)?;

        let pipelines = Pipelines { primitive, image, text, mesh };
        let bind_group_layouts = BindGroupLayouts {
            primitive: primitive_bgl,
            image: image_bgl,
            text: text_bgl,
            mesh_camera: mesh_camera_bgl,
            mesh_model: mesh_model_bgl,
        };
        Ok((pipelines, bind_group_layouts))
    }

    fn build_pipeline(
        device: &wgpu::Device,
        label: &str,
        wgsl: &str,
        format: wgpu::TextureFormat,
        alpha_blend: bool,
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
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Ok((pipeline, bgl))
    }

    fn build_mesh_pipeline(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) -> Result<(wgpu::RenderPipeline, wgpu::BindGroupLayout, wgpu::BindGroupLayout), RendererError> {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mesh"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(MESH_WGSL)),
        });

        // group(0): camera uniform (view_proj matrix) — vertex-only
        let camera_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mesh_camera"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        // group(1): model uniform (model matrix + color) — vertex and fragment
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

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh"),
            bind_group_layouts: &[&camera_bgl, &model_bgl],
            push_constant_ranges: &[],
        });

        let vertex_attrs = wgpu::vertex_attr_array![
            0 => Float32x3, // position
            1 => Float32x3, // normal
            2 => Float32x2, // uv
        ];

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mesh"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 32,
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
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Ok((pipeline, camera_bgl, model_bgl))
    }
}

/// Renderer configuration extended with a runtime surface reference.
pub struct RendererState {
    pub gpu: GpuContext,
    pub config: RendererConfig,
}

impl RendererState {
    pub fn clear_color_wgpu(&self) -> wgpu::Color {
        let [r, g, b, a] = self.config.clear_color;
        wgpu::Color {
            r: r as f64 / 255.0,
            g: g as f64 / 255.0,
            b: b as f64 / 255.0,
            a: a as f64 / 255.0,
        }
    }
}

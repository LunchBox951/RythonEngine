/// Primitive shader — renders rects, circles, and lines via a unit-quad instanced draw.
///
/// Uniform layout (bytes):
///   0–15:  rect: vec4<f32>  — clip-space (x, y, w, h)
///  16–31:  color: vec4<f32> — RGBA 0.0–1.0
///  32–35:  mode: u32        — 0=rect_fill, 1=rect_border, 2=circle, 3=line
///  36–47:  _pad: vec3<u32>  — alignment padding
pub const PRIMITIVE_WGSL: &str = r#"
struct Uniforms {
    rect:  vec4<f32>,
    color: vec4<f32>,
    mode:  u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};

@group(0) @binding(0)
var<uniform> u: Uniforms;

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );
    let uv = uvs[vi];
    let cx = u.rect.x + uv.x * u.rect.z;
    let cy = u.rect.y + uv.y * u.rect.w;
    var out: VertexOutput;
    out.clip_pos = vec4<f32>(cx, cy, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return u.color;
}
"#;

/// Image shader — renders textured quads with alpha blending.
///
/// Uniform layout (bytes):
///   0–15:  rect: vec4<f32>  — clip-space (x, y, w, h)
///  16–19:  alpha: f32
///  20–31:  _pad: vec3<f32>
pub const IMAGE_WGSL: &str = r#"
struct Uniforms {
    rect:  vec4<f32>,
    alpha: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var t_diffuse: texture_2d<f32>;
@group(0) @binding(2) var s_diffuse: sampler;

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );
    let uv = uvs[vi];
    let cx = u.rect.x + uv.x * u.rect.z;
    let cy = u.rect.y + uv.y * u.rect.w;
    var out: VertexOutput;
    out.clip_pos = vec4<f32>(cx, cy, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var color = textureSample(t_diffuse, s_diffuse, in.uv);
    color.a = color.a * u.alpha;
    return color;
}
"#;

/// Text shader — renders individual glyph quads sampled from a glyph atlas.
///
/// Uniform layout (bytes):
///   0–15:  rect: vec4<f32>   — clip-space (x, y, w, h) for this glyph quad
///  16–31:  uv_rect: vec4<f32>— atlas UV region (u0, v0, u1, v1)
///  32–47:  color: vec4<f32>  — RGBA 0.0–1.0
pub const TEXT_WGSL: &str = r#"
struct Uniforms {
    rect:    vec4<f32>,
    uv_rect: vec4<f32>,
    color:   vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var t_atlas: texture_2d<f32>;
@group(0) @binding(2) var s_atlas: sampler;

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    var local_uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );
    let luv = local_uvs[vi];
    let cx = u.rect.x + luv.x * u.rect.z;
    let cy = u.rect.y + luv.y * u.rect.w;
    // Map local UV to atlas UV region
    let au = u.uv_rect.x + luv.x * (u.uv_rect.z - u.uv_rect.x);
    let av = u.uv_rect.y + luv.y * (u.uv_rect.w - u.uv_rect.y);
    var out: VertexOutput;
    out.clip_pos = vec4<f32>(cx, cy, 0.0, 1.0);
    out.uv = vec2<f32>(au, av);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let alpha = textureSample(t_atlas, s_atlas, in.uv).r;
    return vec4<f32>(u.color.rgb, u.color.a * alpha);
}
"#;

/// Mesh shader — §1 normal mapping: TBN matrix, tangent-space normal sampling, optional normal map.
pub const MESH_WGSL: &str = r#"
struct CameraUniforms {
    view_proj: mat4x4<f32>,
};

// ModelUniforms: 96 bytes total
//   model:          mat4x4<f32>  [0-63]   64 B
//   color:          vec4<f32>    [64-79]  16 B
//   has_texture:    u32          [80-83]   4 B
//   metallic:       f32          [84-87]   4 B  (PBR hint: 0=dielectric, 1=metal)
//   roughness:      f32          [88-91]   4 B  (PBR hint: 0=smooth, 1=rough; default 0.5)
//   has_normal_map: u32          [92-95]   4 B  (1 = sample normal map, 0 = use vertex normal)
struct ModelUniforms {
    model:          mat4x4<f32>,
    color:          vec4<f32>,
    has_texture:    u32,
    metallic:       f32,
    roughness:      f32,
    has_normal_map: u32,
};

// DirectionalLightUniform: 32 bytes
//   direction: vec4<f32>  xyz = normalized direction, w = intensity
//   color:     vec4<f32>  xyz = RGB color, w = unused
struct DirectionalLightUniform {
    direction: vec4<f32>,
    color:     vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera:     CameraUniforms;
@group(1) @binding(0) var<uniform> model_data: ModelUniforms;
@group(2) @binding(0) var t_diffuse:    texture_2d<f32>;
@group(2) @binding(1) var s_diffuse:    sampler;
@group(3) @binding(0) var t_normal_map: texture_2d<f32>;
@group(3) @binding(1) var s_normal_map: sampler;
@group(4) @binding(0) var<uniform> dir_light: DirectionalLightUniform;

struct VertexInput {
    @location(0) position:  vec3<f32>,
    @location(1) normal:    vec3<f32>,
    @location(2) uv:        vec2<f32>,
    @location(3) tangent:   vec3<f32>,
    @location(4) bitangent: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_pos:       vec4<f32>,
    @location(0)       world_normal:   vec3<f32>,
    @location(1)       world_tangent:  vec3<f32>,
    @location(2)       world_bitangent: vec3<f32>,
    @location(3)       uv:             vec2<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    let world_pos = model_data.model * vec4<f32>(in.position, 1.0);
    var out: VertexOutput;
    out.clip_pos        = camera.view_proj * world_pos;
    out.world_normal    = (model_data.model * vec4<f32>(in.normal,    0.0)).xyz;
    out.world_tangent   = (model_data.model * vec4<f32>(in.tangent,   0.0)).xyz;
    out.world_bitangent = (model_data.model * vec4<f32>(in.bitangent, 0.0)).xyz;
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let light_dir       = normalize(dir_light.direction.xyz);
    let light_col       = dir_light.color.xyz;
    let intensity_scale = dir_light.direction.w;

    var N: vec3<f32>;
    if (model_data.has_normal_map != 0u) {
        let tbn_sample    = textureSample(t_normal_map, s_normal_map, in.uv).rgb;
        let tangent_normal = tbn_sample * 2.0 - 1.0;
        let TBN = mat3x3<f32>(
            normalize(in.world_tangent),
            normalize(in.world_bitangent),
            normalize(in.world_normal),
        );
        N = normalize(TBN * tangent_normal);
    } else {
        N = normalize(in.world_normal);
    }

    let diffuse = max(dot(N, light_dir), 0.0);

    // Roughness modulates specular contribution: smoother (low roughness) → brighter highlights.
    // At default roughness=0.5 the effective factor equals the original 0.8, preserving prior look.
    let shininess_factor = clamp(0.8 * (1.5 - model_data.roughness), 0.0, 1.0);
    let intensity = (0.2 + diffuse * shininess_factor) * intensity_scale;

    var base_color: vec4<f32>;
    if (model_data.has_texture != 0u) {
        base_color = textureSample(t_diffuse, s_diffuse, in.uv);
    } else {
        base_color = model_data.color;
    }
    return vec4<f32>(base_color.rgb * light_col * intensity, base_color.a);
}
"#;

/// Error returned by [`validate_wgsl`].
#[derive(Debug)]
pub struct ShaderError {
    /// The source string that failed validation.
    pub source: String,
    /// Human-readable description of the failure.
    pub message: String,
    /// Approximate location hint if available.
    pub location: String,
}

impl std::fmt::Display for ShaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "shader error at {}: {}", self.location, self.message)
    }
}

impl std::error::Error for ShaderError {}

/// Lightweight offline WGSL validator.
///
/// Checks for structural requirements without a GPU device:
/// - Must contain a `@vertex` entry point annotation.
/// - Must contain a `@fragment` entry point annotation.
/// - Must not be empty.
///
/// For full validation (type checking, binding compatibility, etc.) a
/// `wgpu::Device` is required via [`wgpu::Device::create_shader_module`].
pub fn validate_wgsl(source: &str) -> Result<(), ShaderError> {
    let src = source.trim();
    if src.is_empty() {
        return Err(ShaderError {
            source: source.to_string(),
            message: "empty shader source".to_string(),
            location: "<empty>".to_string(),
        });
    }
    if !src.contains("@vertex") {
        return Err(ShaderError {
            source: source.to_string(),
            message: "missing @vertex entry point annotation".to_string(),
            location: "global scope".to_string(),
        });
    }
    if !src.contains("@fragment") {
        return Err(ShaderError {
            source: source.to_string(),
            message: "missing @fragment entry point annotation".to_string(),
            location: "global scope".to_string(),
        });
    }
    // Check for clearly broken syntax markers
    let unmatched_open = src.chars().filter(|&c| c == '{').count();
    let unmatched_close = src.chars().filter(|&c| c == '}').count();
    if unmatched_open != unmatched_close {
        return Err(ShaderError {
            source: source.to_string(),
            message: format!(
                "unmatched braces: {} opening vs {} closing",
                unmatched_open, unmatched_close
            ),
            location: "global scope".to_string(),
        });
    }
    Ok(())
}

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

/// Mesh shader — §2 specular mapping: Phong specular term, specular map, CameraUniform eye_position.
pub const MESH_WGSL: &str = r#"
// CameraUniforms: 80 bytes total
//   view_proj:    mat4x4<f32>  [0-63]   64 B
//   eye_position: vec3<f32>    [64-75]  12 B  (world-space camera position for specular)
//   _pad:         f32          [76-79]   4 B
struct CameraUniforms {
    view_proj:    mat4x4<f32>,
    eye_position: vec3<f32>,
    _pad:         f32,
};

// ModelUniforms: 128 bytes total
//   model:            mat4x4<f32>  [0-63]    64 B
//   color:            vec4<f32>    [64-79]   16 B
//   specular_color:   vec4<f32>    [80-95]   16 B  xyz = tint, w unused
//   has_texture:      u32          [96-99]    4 B
//   has_normal_map:   u32          [100-103]  4 B
//   has_specular_map: u32          [104-107]  4 B
//   metallic:         f32          [108-111]  4 B
//   roughness:        f32          [112-115]  4 B
//   shininess:        f32          [116-119]  4 B  scalar fallback when has_specular_map==0
//   _pad0:            u32          [120-123]  4 B
//   _pad1:            u32          [124-127]  4 B
struct ModelUniforms {
    model:            mat4x4<f32>,
    color:            vec4<f32>,
    specular_color:   vec4<f32>,
    has_texture:      u32,
    has_normal_map:   u32,
    has_specular_map: u32,
    metallic:         f32,
    roughness:        f32,
    shininess:        f32,
    _pad0:            u32,
    _pad1:            u32,
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
@group(4) @binding(0) var t_specular:   texture_2d<f32>;
@group(4) @binding(1) var s_specular:   sampler;
@group(5) @binding(0) var<uniform> dir_light: DirectionalLightUniform;

struct VertexInput {
    @location(0) position:  vec3<f32>,
    @location(1) normal:    vec3<f32>,
    @location(2) uv:        vec2<f32>,
    @location(3) tangent:   vec3<f32>,
    @location(4) bitangent: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_pos:        vec4<f32>,
    @location(0)       world_normal:    vec3<f32>,
    @location(1)       world_tangent:   vec3<f32>,
    @location(2)       world_bitangent: vec3<f32>,
    @location(3)       uv:              vec2<f32>,
    @location(4)       world_pos:       vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    let world_pos4 = model_data.model * vec4<f32>(in.position, 1.0);
    var out: VertexOutput;
    out.clip_pos        = camera.view_proj * world_pos4;
    out.world_normal    = (model_data.model * vec4<f32>(in.normal,    0.0)).xyz;
    out.world_tangent   = (model_data.model * vec4<f32>(in.tangent,   0.0)).xyz;
    out.world_bitangent = (model_data.model * vec4<f32>(in.bitangent, 0.0)).xyz;
    out.uv              = in.uv;
    out.world_pos       = world_pos4.xyz;
    return out;
}

fn compute_specular(
    view_dir:  vec3<f32>,
    N:         vec3<f32>,
    light_dir: vec3<f32>,
    uv:        vec2<f32>,
) -> vec3<f32> {
    var spec_intensity: f32 = 1.0;
    var spec_power:     f32 = model_data.shininess;

    if (model_data.has_specular_map != 0u) {
        let spec_sample = textureSample(t_specular, s_specular, uv).rg;
        spec_intensity  = spec_sample.r;
        spec_power      = exp2(spec_sample.g * 7.0);  // [1, 128]
    }

    let reflect_dir = reflect(-light_dir, N);
    let spec_factor = pow(max(dot(view_dir, reflect_dir), 0.0), spec_power);
    return model_data.specular_color.xyz * spec_intensity * spec_factor;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let light_dir       = normalize(dir_light.direction.xyz);
    let light_col       = dir_light.color.xyz;
    let intensity_scale = dir_light.direction.w;

    var N: vec3<f32>;
    if (model_data.has_normal_map != 0u) {
        let tbn_sample     = textureSample(t_normal_map, s_normal_map, in.uv).rgb;
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

    // Roughness modulates diffuse contribution; smoother → brighter diffuse highlight.
    let roughness_factor = clamp(0.8 * (1.5 - model_data.roughness), 0.0, 1.0);
    let diffuse_intensity = (0.2 + diffuse * roughness_factor) * intensity_scale;

    let view_dir = normalize(camera.eye_position - in.world_pos);
    let spec = compute_specular(view_dir, N, light_dir, in.uv) * intensity_scale;

    var base_color: vec4<f32>;
    if (model_data.has_texture != 0u) {
        base_color = textureSample(t_diffuse, s_diffuse, in.uv);
    } else {
        base_color = model_data.color;
    }
    return vec4<f32>(base_color.rgb * light_col * diffuse_intensity + spec * light_col, base_color.a);
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

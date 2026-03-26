# Physically Based Rendering (PBR)

**Status:** Pending
**Priority:** Advanced, Higher Effort
**SPEC.md entry:** §16
**Depends on:** multiple-lights.spec.md (§5), skybox-cubemap.spec.md (§8), normal-mapping.spec.md (§1)

---

## Overview

Implement the metallic/roughness PBR workflow (glTF 2.0 standard). Replaces the Phong shading model with a Cook-Torrance microfacet BRDF. PBR meshes use a dedicated `PbrMaterial` component and pipeline; existing `MeshComponent` with Phong shading continues to work alongside.

BRDF components:
- **D** — GGX normal distribution function.
- **G** — Smith-GGX geometric attenuation (height-correlated).
- **F** — Schlick Fresnel.
- Diffuse: Lambertian / Disney diffuse.
- Image-based lighting (IBL): diffuse irradiance + specular radiance from the scene cubemap (§8).

---

## Rust Implementation

### New Component

**`crates/rython-ecs/src/component.rs` — `PbrMeshComponent`**

```rust
pub struct PbrMeshComponent {
    pub mesh_id:            String,

    // PBR texture maps (all optional)
    pub albedo_map:         Option<String>,    // RGBA — base color
    pub normal_map:         Option<String>,    // tangent-space normal
    pub metallic_roughness_map: Option<String>, // R=unused, G=roughness, B=metallic (glTF convention)
    pub ao_map:             Option<String>,    // R channel = ambient occlusion
    pub emissive_map:       Option<String>,    // RGB emissive

    // Scalar fallbacks (used when no map; also multiplied with map values)
    pub albedo_factor:      [f32; 4],          // RGBA linear; default [1,1,1,1]
    pub metallic_factor:    f32,               // default 0.0 (dielectric)
    pub roughness_factor:   f32,               // default 0.5
    pub ao_strength:        f32,               // default 1.0
    pub emissive_factor:    [f32; 3],          // default [0,0,0]

    pub alpha_mode:         AlphaMode,         // from alpha-blending spec
    pub yaw_offset:         f32,
    pub visible:            bool,
}
```

### New PBR Pipeline

**`crates/rython-renderer/src/pbr_pipeline.rs`** (new file)

```rust
pub struct PbrPipeline {
    pub opaque:      wgpu::RenderPipeline,
    pub cutout:      wgpu::RenderPipeline,
    pub blend:       wgpu::RenderPipeline,
    pub bgl_camera:  wgpu::BindGroupLayout,   // @group(0): camera + lights
    pub bgl_material: wgpu::BindGroupLayout,  // @group(1): PBR uniforms + 5 texture slots
    pub bgl_ibl:     wgpu::BindGroupLayout,   // @group(2): irradiance cube + prefilter cube + brdf lut
}
```

### IBL Pre-computation

**`crates/rython-renderer/src/ibl.rs`** (new file)

Computed once when the skybox cubemap is set:

```rust
pub struct IblResources {
    /// Diffuse irradiance cubemap (32×32 each face)
    pub irradiance_cube:  wgpu::Texture,

    /// Pre-filtered specular cubemap (6 mip levels, 128×128 top mip)
    pub prefilter_cube:   wgpu::Texture,

    /// BRDF integration LUT (512×512, Rg16Float — stores scale/bias for F0)
    pub brdf_lut:         wgpu::Texture,
}

impl IblResources {
    /// Compute IBL textures from an environment cubemap using compute shaders.
    pub fn compute(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        env_cube: &wgpu::Texture,
    ) -> Self;
}
```

### PBR Material Uniform

```rust
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PbrMaterialUniform {
    pub albedo_factor:    [f32; 4],
    pub emissive_factor:  [f32; 4],    // xyz = color, w = unused
    pub metallic_factor:  f32,
    pub roughness_factor: f32,
    pub ao_strength:      f32,
    pub alpha_cutoff:     f32,
    // Texture presence flags
    pub has_albedo:       u32,
    pub has_normal:       u32,
    pub has_metallic_roughness: u32,
    pub has_ao:           u32,
    pub has_emissive:     u32,
    pub alpha_mode:       u32,         // 0=opaque, 1=cutout, 2=blend
    pub _pad:             [u32; 2],
}
```

### Shader

**`crates/rython-renderer/src/shaders.rs`** — New constant `PBR_MESH_WGSL`:

```wgsl
// --- GGX BRDF ---
fn distribution_ggx(N: vec3<f32>, H: vec3<f32>, roughness: f32) -> f32 {
    let a  = roughness * roughness;
    let a2 = a * a;
    let NdH  = max(dot(N, H), 0.0);
    let NdH2 = NdH * NdH;
    let denom = (NdH2 * (a2 - 1.0) + 1.0);
    return a2 / (PI * denom * denom);
}

fn geometry_schlick_ggx(NdV: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;
    return NdV / (NdV * (1.0 - k) + k);
}

fn geometry_smith(N: vec3<f32>, V: vec3<f32>, L: vec3<f32>, roughness: f32) -> f32 {
    let NdV = max(dot(N, V), 0.0);
    let NdL = max(dot(N, L), 0.0);
    return geometry_schlick_ggx(NdV, roughness) * geometry_schlick_ggx(NdL, roughness);
}

fn fresnel_schlick_pbr(cos_theta: f32, F0: vec3<f32>, roughness: f32) -> vec3<f32> {
    return F0 + (max(vec3(1.0 - roughness), F0) - F0) * pow(1.0 - cos_theta, 5.0);
}

// --- IBL ---
@group(2) @binding(0) var t_irradiance:   texture_cube<f32>;
@group(2) @binding(1) var t_prefilter:    texture_cube<f32>;
@group(2) @binding(2) var t_brdf_lut:     texture_2d<f32>;
@group(2) @binding(3) var s_cube_linear:  sampler;
@group(2) @binding(4) var s_brdf:         sampler;

fn sample_ibl(N: vec3<f32>, V: vec3<f32>, F0: vec3<f32>, roughness: f32, metallic: f32, albedo: vec3<f32>) -> vec3<f32> {
    let R = reflect(-V, N);
    let NdV = max(dot(N, V), 0.0);

    // Diffuse irradiance
    let irradiance = textureSample(t_irradiance, s_cube_linear, N).rgb;
    let F = fresnel_schlick_pbr(NdV, F0, roughness);
    let kD = (1.0 - F) * (1.0 - metallic);
    let diffuse_ibl = kD * irradiance * albedo;

    // Specular pre-filtered
    let max_mip = 5.0;
    let prefiltered = textureSampleLevel(t_prefilter, s_cube_linear, R, roughness * max_mip).rgb;
    let brdf = textureSample(t_brdf_lut, s_brdf, vec2(NdV, roughness)).rg;
    let specular_ibl = prefiltered * (F0 * brdf.x + brdf.y);

    return diffuse_ibl + specular_ibl;
}
```

### Frame Loop

PBR entities are collected into a separate `DrawPbrMesh` command type. `render_pbr_meshes()` runs in the same 3D render pass as `render_meshes()`, using the PBR pipeline.

### Key Files to Modify

| File | Change |
|------|--------|
| `crates/rython-ecs/src/component.rs` | New `PbrMeshComponent` |
| `crates/rython-renderer/src/pbr_pipeline.rs` | New — `PbrPipeline` |
| `crates/rython-renderer/src/ibl.rs` | New — IBL pre-computation |
| `crates/rython-renderer/src/shaders.rs` | `PBR_MESH_WGSL` |
| `crates/rython-renderer/src/command.rs` | New `DrawPbrMesh` variant |
| `crates/rython-renderer/src/lib.rs` | PBR render sub-pass |
| `crates/rython-scripting/src/bridge/scene.rs` | `pbr_mesh` kwarg in `spawn()` |

---

## Python API

```python
entity = rython.scene.spawn(
    transform=rython.Transform(0, 0, 0),
    pbr_mesh={                                          # NEW kwarg — uses PBR pipeline
        "mesh_id":                 "models/helmet.glb",
        "albedo_map":              "textures/helmet_albedo.png",
        "normal_map":              "textures/helmet_normal.png",
        "metallic_roughness_map":  "textures/helmet_mr.png",
        "ao_map":                  "textures/helmet_ao.png",
        "emissive_map":            "textures/helmet_emissive.png",
        # Scalar factors (multiply with maps)
        "metallic_factor":         1.0,
        "roughness_factor":        1.0,
        "albedo_factor":           (1.0, 1.0, 1.0, 1.0),
    },
)
```

---

## Test Cases

### Test 1: GGX distribution is 1 when roughness=0 and N==H

- **Expected:** `distribution_ggx(N, N, 0.001) ≈ 1/PI * 1e6` (very high, near delta function).

### Test 2: Fresnel F0 for dielectric is (0.04, 0.04, 0.04)

- **Setup:** `metallic=0`, `albedo=(0.5,0.5,0.5)`. F0 computation.
- **Expected:** `F0 == vec3(0.04)`.

### Test 3: Fresnel F0 for metallic is albedo color

- **Setup:** `metallic=1.0`, `albedo=(0.8, 0.3, 0.1)`.
- **Expected:** `F0 == vec3(0.8, 0.3, 0.1)`.

### Test 4: IBL textures computed after skybox upload

- **Setup:** `set_skybox(...)` call.
- **Expected:** `IblResources.irradiance_cube` and `prefilter_cube` created; BRDF LUT generated.

### Test 5: Irradiance cubemap is 32×32

- **Expected:** `irradiance_cube` dimensions are `32×32`, 6 layers.

### Test 6: BRDF LUT is 512×512 Rg16Float

- **Expected:** `brdf_lut` dimensions `512×512`, format `Rg16Float`.

### Test 7: PBR mesh falls back to albedo_factor when no albedo_map

- **Setup:** `PbrMeshComponent { albedo_map: None, albedo_factor: [1,0,0,1] }`.
- **Expected:** `has_albedo == 0`, `albedo_factor == [1,0,0,1]` in `PbrMaterialUniform`.

### Test 8: glTF metallic_roughness_map uses G=roughness, B=metallic

- **Setup:** Sample a pixel with `G=0.3, B=0.9`.
- **Expected:** Shader reads `roughness = 0.3`, `metallic = 0.9`.

### Test 9: Existing MeshComponent entities still render with Phong pipeline

- **Setup:** Scene with one `MeshComponent` and one `PbrMeshComponent`.
- **Expected:** Two separate render passes; `MeshComponent` uses `mesh_opaque_pipeline`, `PbrMeshComponent` uses `PbrPipeline.opaque`.

### Test 10: PBR entity with `alpha_mode="blend"` sorted with other transparent entities

- **Setup:** PBR blend entity and non-PBR blend entity.
- **Expected:** Both in the back-to-front sorted transparent pass.

---

## Gauntlet of Cubes Demo

**Where:** `game/scripts/levels/arena_3.py` — boss skeleton and lava pit; `game/scripts/level_builder.py` — optional upgrade path for pickup boxes.

**Effect:** The boss in Arena 3 currently looks like a big purple cube indistinguishable from regular enemies except in scale. Switching the boss to a PBR material with `metallic=1.0`, `roughness=0.15` transforms it into a shiny black metal monolith — obviously different from matte enemies without any model changes. The lava pit gets PBR with `roughness=0.05` (near-mirror) and a fiery albedo.

**Example — boss with PBR in `game/scripts/levels/arena_3.py`:**

```python
def _spawn_wave_2():
    # Regular enemies use existing MeshComponent (Phong pipeline)
    for pos in [(6,1,0), (-6,1,0), (0,1,6), (0,1,-6), (4,1,-4)]:
        level_builder.spawn_enemy(*pos, "skeleton")

    # Boss uses PBR pipeline for a visually distinct material
    boss = rython.scene.spawn(
        transform=rython.Transform(0, 1, -8, scale_x=1.5, scale_y=2.5, scale_z=1.5),
        pbr_mesh={
            "mesh_id":            "cube",
            "albedo_map":         "game/assets/textures/Purple/purple_box.png",
            "metallic_factor":    1.0,
            "roughness_factor":   0.15,
            "albedo_factor":      (0.3, 0.0, 0.5, 1.0),   # deep purple tint
            "emissive_factor":    (0.1, 0.0, 0.2),         # faint purple glow
        },
        rigid_body={"body_type": "dynamic", "mass": 5.0, "gravity_factor": 1.0},
        collider={"shape": "box", "size": [1.5, 2.5, 1.5]},
        tags={"tags": ["enemy", "boss"]},
    )
    _registered.append(boss)
    enemies.register(boss, "skeleton", is_boss=True)
```

**Example — lava pit with PBR in `game/scripts/levels/arena_3.py`:**

```python
lava = rython.scene.spawn(
    transform=rython.Transform(0, 0.05, 0, scale_x=6, scale_y=0.1, scale_z=6),
    pbr_mesh={
        "mesh_id":         "cube",
        "albedo_map":      "game/assets/textures/Red/red_box.png",
        "metallic_factor": 0.0,
        "roughness_factor": 0.05,         # near-mirror surface
        "emissive_factor": (1.0, 0.3, 0.0),
    },
    tags={"tags": ["lava"]},
)
```

**Why it works here:** Because the Phong and PBR pipelines coexist, only the boss and lava are upgraded. All 16 perimeter walls and platform cubes keep the cheap Phong shading — the boss visually stands out from the crowd without any scene-wide material cost.

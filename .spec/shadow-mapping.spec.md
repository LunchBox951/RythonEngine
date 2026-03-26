# Shadow Mapping

**Status:** Pending
**Priority:** High-Impact, Moderate Effort
**SPEC.md entry:** §3

---

## Overview

Add dynamic shadow casting from the primary directional light. The technique is a two-pass approach:

1. **Shadow pass** — Render the scene depth from the light's perspective into a `Texture2d` (shadow map).
2. **Main pass** — Sample the shadow map in the mesh fragment shader; fragments in shadow skip the diffuse/specular light contribution.

This extends the existing single hardcoded directional light (see also §5 for multi-light extension).

---

## Rust Implementation

### New Types

**`crates/rython-renderer/src/shadow.rs`** (new file)

```rust
pub struct ShadowMap {
    pub texture:     wgpu::Texture,
    pub view:        wgpu::TextureView,
    pub sampler:     wgpu::Sampler,         // comparison sampler (DepthCompare::LessEqual)
    pub depth_format: wgpu::TextureFormat,  // Depth32Float
    pub size:        u32,                   // resolution in px (square); e.g. 2048
}

impl ShadowMap {
    pub fn new(device: &wgpu::Device, size: u32) -> Self;
    pub fn resize(&mut self, device: &wgpu::Device, size: u32);
}
```

**`crates/rython-renderer/src/shadow.rs`** — Light-space matrices:

```rust
pub struct LightMatrices {
    pub view_proj: Mat4,     // orthographic light camera
    pub bias:      f32,      // depth bias to prevent shadow acne; default 0.005
}

impl LightMatrices {
    /// Build orthographic view-projection from directional light.
    /// `scene_center` and `scene_radius` define the ortho frustum.
    pub fn from_directional(
        direction: Vec3,
        scene_center: Vec3,
        scene_radius: f32,
    ) -> Self;
}
```

### Scene-Level Shadow Settings

**`crates/rython-core/src/types.rs`** or new `crates/rython-renderer/src/settings.rs`:

```rust
pub struct ShadowSettings {
    pub enabled:          bool,
    pub map_size:         u32,    // 512 | 1024 | 2048 | 4096; default 2048
    pub bias:             f32,    // default 0.005
    pub pcf_samples:      u32,   // PCF kernel size: 1 (no PCF), 4, or 9; default 4
    pub light_direction:  Vec3,   // mirrors LightSettings.direction; kept in sync
}
```

`ShadowSettings` is stored in `GpuContext` and exposed through `RendererBridge`.

### GPU Pipeline Changes

**`crates/rython-renderer/src/gpu.rs`**

New fields on `GpuContext`:

```rust
pub shadow_map:      ShadowMap,
pub shadow_pipeline: wgpu::RenderPipeline,  // depth-only, no color target
pub shadow_bgl:      wgpu::BindGroupLayout, // @group(0): shadow texture + comparison sampler
pub shadow_settings: ShadowSettings,
```

**Shadow pipeline** — vertex-only shader, no fragment output, depth write enabled, no color attachment.

**Main mesh pipeline** — add bind group 5 for shadow map:

```rust
pub mesh_shadow: wgpu::BindGroupLayout,  // @group(5): depth texture + sampler
```

### Shader Changes

**`crates/rython-renderer/src/shaders.rs`**

New shader constant `SHADOW_WGSL` (shadow pass):

```wgsl
// Shadow pass — vertex only, depth write
struct ShadowUniform { light_view_proj: mat4x4<f32> };
@group(0) @binding(0) var<uniform> shadow: ShadowUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    // other attribs unused in shadow pass
};

@vertex
fn vs_shadow(in: VertexInput) -> @builtin(position) vec4<f32> {
    return shadow.light_view_proj * vec4(in.position, 1.0);
}
```

**Modified `MESH_WGSL`** — shadow sampling in fragment shader:

```wgsl
struct LightShadowUniform {
    light_view_proj: mat4x4<f32>,
    bias:            f32,
    pcf_samples:     u32,
    _pad:            vec2<f32>,
};
@group(5) @binding(0) var<uniform> shadow_params: LightShadowUniform;
@group(5) @binding(1) var t_shadow_map: texture_depth_2d;
@group(5) @binding(2) var s_shadow_map: sampler_comparison;

fn sample_shadow_pcf(light_uv: vec2<f32>, depth: f32) -> f32 {
    // PCF 2x2 or 3x3 depending on shadow_params.pcf_samples
    var shadow = 0.0;
    let texel = 1.0 / f32(textureDimensions(t_shadow_map).x);
    for (var x = -1; x <= 1; x++) {
        for (var y = -1; y <= 1; y++) {
            let offset = vec2(f32(x), f32(y)) * texel;
            shadow += textureSampleCompare(
                t_shadow_map, s_shadow_map,
                light_uv + offset,
                depth - shadow_params.bias,
            );
        }
    }
    return shadow / 9.0;
}

// In fs_main:
let light_space_pos = shadow_params.light_view_proj * vec4(in.world_pos, 1.0);
let light_uv = light_space_pos.xy * 0.5 + 0.5;
let shadow_factor = sample_shadow_pcf(light_uv, light_space_pos.z);
// Attenuate diffuse by shadow_factor (0 = full shadow, 1 = lit)
let lit_color = ambient + shadow_factor * (diffuse + specular);
```

### Frame Loop Changes

**`crates/rython-renderer/src/lib.rs`** — `render_frame()`:

1. Compute `LightMatrices` from current `ShadowSettings.light_direction`.
2. **Shadow pass:** `render_meshes_shadow_pass()` — encode depth-only draw for all visible meshes.
3. **Main pass:** Bind shadow map to group 5 for all mesh draws.

### Key Files to Modify

| File | Change |
|------|--------|
| `crates/rython-renderer/src/shadow.rs` | New — `ShadowMap`, `LightMatrices`, `ShadowSettings` |
| `crates/rython-renderer/src/shaders.rs` | New `SHADOW_WGSL`; shadow sampling in `MESH_WGSL` |
| `crates/rython-renderer/src/gpu.rs` | New fields: `shadow_map`, `shadow_pipeline`, `shadow_bgl` |
| `crates/rython-renderer/src/lib.rs` | Two-pass frame loop; shadow bind group |
| `crates/rython-scripting/src/bridge/renderer.rs` | Expose `set_shadow_settings()` |

---

## Python API

### Renderer Bridge Methods (new)

```python
rython.renderer.set_shadow_enabled(enabled: bool) -> None
rython.renderer.set_shadow_map_size(size: int) -> None   # 512, 1024, 2048, or 4096
rython.renderer.set_shadow_bias(bias: float) -> None
rython.renderer.set_shadow_pcf(samples: int) -> None     # 1, 4, or 9
```

### Example Usage

```python
import rython

def on_start():
    rython.renderer.set_shadow_enabled(True)
    rython.renderer.set_shadow_map_size(2048)
    rython.renderer.set_shadow_bias(0.003)
    rython.renderer.set_shadow_pcf(9)

    rython.scene.spawn(
        transform=rython.Transform(0, -1, 0, scale_x=20, scale_y=0.1, scale_z=20),
        mesh={"mesh_id": "cube", "texture_id": "ground.png"},
    )
    rython.scene.spawn(
        transform=rython.Transform(0, 1, 0),
        mesh={"mesh_id": "cube", "texture_id": "stone.png"},
    )
```

---

## Test Cases

### Test 1: ShadowMap texture has correct format and size

- **Setup:** `ShadowMap::new(device, 2048)`.
- **Expected:** `texture.format() == Depth32Float`, dimensions `2048×2048`.

### Test 2: `LightMatrices::from_directional` produces valid view-proj

- **Setup:** `direction = Vec3::new(0.5, -1.0, 0.5).normalize()`, `center = Vec3::ZERO`, `radius = 10.0`.
- **Expected:** `view_proj` transforms a point at `Vec3::ZERO` to NDC within `[-1,1]³`.

### Test 3: Shadow pass executes before main pass

- **Setup:** Instrument render pipeline with ordered execution markers.
- **Expected:** Shadow depth texture is fully written before mesh main pass begins.

### Test 4: Shadow disabled by default

- **Setup:** Default `GpuContext` without calling `set_shadow_enabled`.
- **Expected:** `shadow_settings.enabled == false`; main pass does not bind shadow map.

### Test 5: `set_shadow_map_size` rejects invalid values

- **Setup:** Call `set_shadow_map_size(300)` (not a power of two in allowed set).
- **Expected:** Returns `Err(RendererError::InvalidShadowMapSize)` or logs warning and clamps to 512.

### Test 6: Shadow map is not allocated when shadows disabled

- **Setup:** `shadow_settings.enabled = false`.
- **Expected:** `ShadowMap` texture is not created (saves GPU memory).

### Test 7: PCF sample count 1 disables filtering

- **Setup:** `set_shadow_pcf(1)`.
- **Expected:** `shadow_params.pcf_samples == 1`; the fragment shader issues a single `textureSampleCompare` call.

### Test 8: Full shadow at origin (occluded)

- **Setup:** Headless GPU with a cube at `(0,0,0)` and a ground plane at `(0,-2,0)`. Light from directly above `(0,1,0)`.
- **Action:** Read back shadow factor texel at ground plane directly below cube.
- **Expected:** `shadow_factor < 0.1` (fully in shadow).

### Test 9: No shadow beyond scene radius

- **Setup:** Spawn entity at `(9999, 0, 0)`, well outside `scene_radius = 10`.
- **Expected:** No out-of-bounds shadow map sampling panic; entity treated as unshadowed.

### Test 10: Python `set_shadow_bias` round-trips

- **Setup:** Call `rython.renderer.set_shadow_bias(0.007)`.
- **Action:** Read back `shadow_settings.bias`.
- **Expected:** `bias == 0.007f32`.

---

## Gauntlet of Cubes Demo

**Where:** `game/scripts/main.py` — `init()` and `_on_load_level()`.

**Effect:** Arena 2 (Gauntlet Run) is a chain of floating platforms over a void. Without shadows the player has no depth cue for platform edges or jump distance. With shadow mapping, the player's cube casts a shadow down onto the platform beneath it — immediately communicating how far they are above the surface. In Arena 3, enemies cast shadows on the boss-arena floor, making the wave fight feel grounded.

**Example — enable shadows in `game/scripts/main.py` `init()`:**

```python
def init():
    rython.physics.set_gravity(0, -20, 0)

    # Enable shadow mapping for all arenas
    rython.renderer.set_shadow_enabled(True)
    rython.renderer.set_shadow_map_size(1024)
    rython.renderer.set_shadow_bias(0.003)
    rython.renderer.set_shadow_pcf(4)

    # ... existing subscriptions ...
```

**Example — per-arena shadow quality in `_on_load_level()`:**

```python
def _on_load_level(data):
    level = data.get("level", 1)
    if level == 3:
        # Boss arena: higher resolution shadows for dramatic effect
        rython.renderer.set_shadow_map_size(2048)
        rython.renderer.set_shadow_pcf(9)
    else:
        rython.renderer.set_shadow_map_size(1024)
        rython.renderer.set_shadow_pcf(4)
    # ... existing level load ...
```

**Why it's compelling here:** Arena 2 has no floor under most of the path. The moment the player sees their own shadow shrink as they jump, they get precise height feedback — turning shadow mapping from a visual effect into a gameplay aid.

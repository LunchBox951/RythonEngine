# Ambient Occlusion (SSAO)

**Status:** Pending
**Priority:** High-Impact, Moderate Effort
**SPEC.md entry:** §6

---

## Overview

Screen-Space Ambient Occlusion (SSAO) darkens surface crevices and contact shadows cheaply in screen space. The technique requires a geometry buffer (GBuffer) with view-space positions and normals, a hemisphere of random sample offsets, and a blur pass to reduce noise.

This spec assumes the **deferred rendering** spec (§18) is not yet implemented. We use a lightweight **forward+ SSAO** approach: the GBuffer is rendered in a pre-pass (depth + normals only), SSAO is computed in a post-process, and the result is applied as an ambient occlusion multiplier in the mesh main pass.

---

## Rust Implementation

### New Types

**`crates/rython-renderer/src/ssao.rs`** (new file)

```rust
pub struct SsaoResources {
    /// Texture storing view-space normals (RGBA16Float) — same resolution as swap chain
    pub normal_texture:   wgpu::Texture,
    pub normal_view:      wgpu::TextureView,

    /// SSAO result texture (R8Unorm) — full or half resolution
    pub occlusion_texture: wgpu::Texture,
    pub occlusion_view:    wgpu::TextureView,

    /// Blurred SSAO texture (same format)
    pub blur_texture: wgpu::Texture,
    pub blur_view:    wgpu::TextureView,

    /// Noise texture 4×4 (RGBA32Float, tiled) — random rotation vectors
    pub noise_texture: wgpu::Texture,
    pub noise_view:    wgpu::TextureView,

    /// Uniform buffer: SSAO kernel samples (64 × vec4)
    pub kernel_buffer: wgpu::Buffer,

    /// Bind groups for each pass
    pub normal_pass_bgl:     wgpu::BindGroupLayout,
    pub ssao_compute_bgl:    wgpu::BindGroupLayout,
    pub ssao_blur_bgl:       wgpu::BindGroupLayout,
    pub ssao_apply_bgl:      wgpu::BindGroupLayout,
}

pub struct SsaoSettings {
    pub enabled:       bool,
    pub radius:        f32,       // world-space sample radius; default 0.5
    pub bias:          f32,       // depth bias to prevent self-occlusion; default 0.025
    pub sample_count:  u32,       // kernel size: 16, 32, or 64; default 32
    pub intensity:     f32,       // occlusion strength [0,1]; default 1.0
    pub blur_passes:   u32,       // separable blur iterations; default 2
    pub half_res:      bool,      // compute at half resolution then upsample; default false
}
```

### SSAO Kernel Generation

**`crates/rython-renderer/src/ssao.rs`**

```rust
/// Generate hemisphere sample kernel (Lerped toward center for better distribution).
pub fn generate_ssao_kernel(count: u32) -> Vec<[f32; 4]>;   // w = 0 (padding)

/// Generate 4×4 noise texture (rotation vectors in XY plane).
pub fn generate_noise_texture(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::Texture;
```

Kernel samples are distributed in a hemisphere with their lerp scaled by `lerp(0.1, 1.0, (i/count)^2)` to concentrate samples near the origin.

### Multi-Pass Structure

Three new render passes inserted after depth pre-pass and before main shading:

1. **Normal pre-pass** — Renders view-space normals into `normal_texture` using a minimal vertex/fragment shader. Reuses existing depth buffer.
2. **SSAO compute pass** — Samples hemisphere around each fragment, tests depth, accumulates occlusion.
3. **Blur pass** — Separable 4×4 Gaussian blur on `occlusion_texture` → `blur_texture`.

### Shader Changes

**`crates/rython-renderer/src/shaders.rs`**

New constants: `SSAO_NORMAL_PREPASS_WGSL`, `SSAO_COMPUTE_WGSL`, `SSAO_BLUR_WGSL`.

**`SSAO_NORMAL_PREPASS_WGSL`:**

```wgsl
// Writes view-space normals to attachment; depth writes enabled (reuses main depth).
@group(0) @binding(0) var<uniform> camera: CameraUniform;  // includes view matrix
@group(1) @binding(0) var<uniform> model: ModelUniform;

@fragment
fn fs_prepass(in: VertexOutput) -> @location(0) vec4<f32> {
    let view_normal = normalize((camera.view * vec4(in.world_normal, 0.0)).xyz);
    return vec4(view_normal * 0.5 + 0.5, 1.0);  // encode to [0,1]
}
```

**`SSAO_COMPUTE_WGSL`:**

```wgsl
@group(0) @binding(0) var t_depth:   texture_depth_2d;
@group(0) @binding(1) var t_normal:  texture_2d<f32>;
@group(0) @binding(2) var t_noise:   texture_2d<f32>;
@group(0) @binding(3) var<uniform>   proj:   mat4x4<f32>;
@group(0) @binding(4) var<storage>   kernel: array<vec4<f32>>;
@group(0) @binding(5) var<uniform>   params: SsaoParams;  // radius, bias, sample_count, intensity

// Full-screen quad; outputs R8Unorm occlusion value
```

**Modified `MESH_WGSL`** — SSAO apply:

```wgsl
@group(8) @binding(0) var t_ssao:   texture_2d<f32>;
@group(8) @binding(1) var s_ssao:   sampler;

// In fs_main:
let screen_uv = in.clip_position.xy / vec2<f32>(screen_width, screen_height);
let ao = textureSample(t_ssao, s_ssao, screen_uv).r;
let ambient_contribution = light_buf.ambient * ao;
// Replace existing ambient usage with ambient_contribution
```

### CameraUniform Extension

SSAO compute needs the view matrix and projection matrix separately:

```rust
pub struct CameraUniform {
    pub view_proj:    [[f32; 4]; 4],
    pub view:         [[f32; 4]; 4],    // NEW
    pub proj:         [[f32; 4]; 4],    // NEW
    pub eye_position: [f32; 3],
    pub _pad:         f32,
}
```

### Key Files to Modify

| File | Change |
|------|--------|
| `crates/rython-renderer/src/ssao.rs` | New — `SsaoResources`, `SsaoSettings`, kernel gen |
| `crates/rython-renderer/src/shaders.rs` | 3 new SSAO shaders; modified `MESH_WGSL` for AO apply |
| `crates/rython-renderer/src/gpu.rs` | `SsaoResources` field, camera `view` + `proj` matrices |
| `crates/rython-renderer/src/camera.rs` | Add `view`, `proj` to `CameraUniform` |
| `crates/rython-renderer/src/lib.rs` | SSAO passes in frame loop before main mesh pass |
| `crates/rython-scripting/src/bridge/renderer.rs` | Expose `set_ssao_*` settings |

---

## Python API

```python
rython.renderer.set_ssao_enabled(True)
rython.renderer.set_ssao_radius(0.5)          # world-space units
rython.renderer.set_ssao_bias(0.025)
rython.renderer.set_ssao_samples(32)          # 16, 32, or 64
rython.renderer.set_ssao_intensity(1.0)       # 0.0 = no occlusion, 1.0 = full
rython.renderer.set_ssao_blur(passes=2)
rython.renderer.set_ssao_half_res(False)      # True = half-resolution AO (perf)
```

---

## Test Cases

### Test 1: SSAO disabled by default

- **Expected:** `SsaoSettings.enabled == false`, SSAO textures not allocated.

### Test 2: SSAO kernel has correct size

- **Setup:** `generate_ssao_kernel(32)`.
- **Expected:** Returns 32 `[f32; 4]` samples; each `xyz` is unit length within hemisphere `(z > 0)`.

### Test 3: All kernel samples lie within hemisphere

- **Setup:** `generate_ssao_kernel(64)`.
- **Expected:** For every sample `s`, `s[0]^2 + s[1]^2 + s[2]^2 ≤ 1.0` and `s[2] >= 0`.

### Test 4: Noise texture is 4×4

- **Setup:** `generate_noise_texture(device, queue)`.
- **Expected:** Texture dimensions `4×4`, format `Rgba32Float`.

### Test 5: Noise vectors lie in XY plane

- **Setup:** Read noise texture pixels.
- **Expected:** All pixels have `z == 0.0`, `w == 0.0`; `x` and `y` are unit-length vectors.

### Test 6: Invalid sample count rejected

- **Setup:** `set_ssao_samples(100)` (not in {16,32,64}).
- **Expected:** Warning logged; clamped to nearest valid value (64).

### Test 7: Half-resolution mode allocates smaller textures

- **Setup:** Enable SSAO with window 1920×1080 and `half_res=true`.
- **Expected:** `occlusion_texture` dimensions are `960×540`.

### Test 8: AO texture is white (no occlusion) in empty scene

- **Setup:** Headless render, single mesh floating in void with no surrounding geometry.
- **Action:** Read back occlusion texture pixels near the mesh center.
- **Expected:** Values close to `1.0` (no occlusion).

### Test 9: Disabled SSAO multiplier is 1.0

- **Setup:** SSAO disabled; inspect fragment shader AO term.
- **Expected:** AO value read is `1.0` (fallback white texture), so ambient is unaffected.

### Test 10: CameraUniform contains `view` and `proj` separately

- **Setup:** Set `camera.set_position(5, 5, 5)` and `camera.set_look_at(0, 0, 0)`.
- **Action:** Upload camera uniform.
- **Expected:** `CameraUniform.view_proj == CameraUniform.view × CameraUniform.proj` (within float tolerance).

---

## Gauntlet of Cubes Demo

**Where:** `game/scripts/main.py` — `init()`.

**Effect:** Arena 1's 20×20 flat floor butts up against four border walls. Without AO those corners are uniformly lit flat planes. With SSAO, the base of every wall gets a thin dark band where it meets the floor — the geometry suddenly feels like it has real contact. Arena 3's circular perimeter wall creates 18 column-like segments; SSAO darkens the concave gaps between them.

**Example — enable SSAO in `game/scripts/main.py`:**

```python
def init():
    rython.physics.set_gravity(0, -20, 0)

    rython.renderer.set_ssao_enabled(True)
    rython.renderer.set_ssao_radius(0.4)      # tight radius fits the cube-scale geometry
    rython.renderer.set_ssao_bias(0.02)
    rython.renderer.set_ssao_samples(32)
    rython.renderer.set_ssao_intensity(0.9)
    rython.renderer.set_ssao_blur(passes=2)
    rython.renderer.set_ssao_half_res(True)   # keep perf budget for 60 fps

    # ... existing subscriptions ...
```

**Visible in all three arenas:**

- **Arena 1:** Corners where platforms meet the floor; base of border walls.
- **Arena 2:** Underside of platforms (viewed from below during a fall) shows AO where platform edge meets support geometry.
- **Arena 3:** The lava pit is recessed below the arena floor — the rim around the pit is visibly darkened by AO, which visually marks the danger zone even before the player reads the color.

**Tip for the demo:** Toggle SSAO on/off with a debug key in `game/scripts/main.py` and compare Arena 3's lava pit rim — the contact shadow is the single most visible change in this scene.

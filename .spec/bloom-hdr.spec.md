# Bloom / HDR

**Status:** Pending
**Priority:** Medium-Impact, Lower Effort
**SPEC.md entry:** §13
**Depends on:** post-processing.spec.md (§7) — requires HDR render target

---

## Overview

Bloom creates a glow around bright surfaces. HDR rendering (already introduced by the post-processing spec) is a prerequisite. Bloom is implemented as a post-process effect:

1. **Threshold pass** — Extract pixels brighter than `threshold`; write to a half-resolution brightness buffer.
2. **Downsample + blur** — Iterative bilinear downsample (mip chain) + dual Kawase blur.
3. **Upsample + blend** — Additive blend of blurred brightness back into the HDR scene buffer before tone mapping.

---

## Rust Implementation

### New Types

**`crates/rython-renderer/src/bloom.rs`** (new file)

```rust
pub struct BloomSettings {
    pub enabled:    bool,
    pub threshold:  f32,       // luminance cutoff; default 1.0 (values >1.0 in HDR glow)
    pub knee:       f32,       // soft-knee width; default 0.1
    pub intensity:  f32,       // additive blend weight; default 0.04
    pub mip_levels: u32,       // number of downsample levels; default 5
    pub scatter:    f32,       // upsample scatter / radius; default 0.7
}

pub struct BloomResources {
    /// Half-resolution brightness extraction texture (Rgba16Float)
    pub threshold_texture: wgpu::Texture,
    pub threshold_view:    wgpu::TextureView,

    /// Mip-chain of downsample targets (mip_levels textures, each half the previous)
    pub mip_textures: Vec<wgpu::Texture>,
    pub mip_views:    Vec<wgpu::TextureView>,

    /// Pipelines
    pub threshold_pipeline:  wgpu::RenderPipeline,
    pub downsample_pipeline: wgpu::RenderPipeline,
    pub upsample_pipeline:   wgpu::RenderPipeline,

    /// Shared bind group layout: input texture + sampler
    pub bgl: wgpu::BindGroupLayout,

    /// Uniform buffer: BloomUniform
    pub bloom_buf: wgpu::Buffer,
}
```

### Bloom Uniform

```rust
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BloomUniform {
    pub threshold:  f32,
    pub knee:       f32,
    pub intensity:  f32,
    pub scatter:    f32,
    pub texel_size: [f32; 2],   // 1/width, 1/height of current pass target
    pub _pad:       [f32; 2],
}
```

### Shader Changes

**`crates/rython-renderer/src/shaders.rs`** — Three new constants:

**`BLOOM_THRESHOLD_WGSL`:**

```wgsl
// Extracts bright pixels with soft knee.
// Luminance > threshold → output color; otherwise 0.
fn quadratic_threshold(color: vec3<f32>, threshold: f32, knee: f32) -> vec3<f32> {
    let brightness = max(color.r, max(color.g, color.b));
    var rq = clamp(brightness - threshold + knee, 0.0, 2.0 * knee);
    rq = (rq * rq) / (4.0 * knee + 0.00001);
    let weight = max(rq, brightness - threshold) / max(brightness, 0.00001);
    return color * weight;
}
```

**`BLOOM_DOWNSAMPLE_WGSL`:**

```wgsl
// 13-tap Kawase downsample (Chermain et al.)
// Samples 4 bilinear taps at half-texel offset + 1 center → reduces aliasing.
```

**`BLOOM_UPSAMPLE_WGSL`:**

```wgsl
// 9-tap tent upsample filter.
// Additive blend: scene_hdr + bloom * intensity
```

### Frame Loop Integration

**`crates/rython-renderer/src/lib.rs`** — after main 3D render, before post-process:

```
1. bloom_threshold_pass(hdr_texture → threshold_texture)
2. for i in 0..mip_levels:
       bloom_downsample_pass(mip[i-1] → mip[i])
3. for i in (mip_levels-1)..0:
       bloom_upsample_pass(mip[i] → mip[i-1])
4. blend threshold_texture[0] additively into hdr_texture
5. post_process_pass(hdr_texture → swapchain)
```

### Key Files to Modify

| File | Change |
|------|--------|
| `crates/rython-renderer/src/bloom.rs` | New — all bloom types and pipeline |
| `crates/rython-renderer/src/shaders.rs` | 3 new bloom shaders |
| `crates/rython-renderer/src/gpu.rs` | `BloomResources` field; integrate into frame loop |
| `crates/rython-renderer/src/lib.rs` | Bloom passes in frame loop |
| `crates/rython-scripting/src/bridge/renderer.rs` | Expose bloom settings |

---

## Python API

```python
rython.renderer.set_bloom_enabled(True)
rython.renderer.set_bloom_threshold(1.0)     # luminance above this blooms
rython.renderer.set_bloom_knee(0.1)          # soft transition width
rython.renderer.set_bloom_intensity(0.04)    # additive blend weight
rython.renderer.set_bloom_mip_levels(5)      # 3–6 recommended
rython.renderer.set_bloom_scatter(0.7)
```

### Typical Setup

```python
# Stylized neon bloom
rython.renderer.set_bloom_enabled(True)
rython.renderer.set_bloom_threshold(0.8)
rython.renderer.set_bloom_intensity(0.1)
rython.renderer.set_bloom_scatter(0.85)

# Combined with emissive meshes for glowing objects
lamp = rython.scene.spawn(
    transform=rython.Transform(0, 2, 0),
    mesh={
        "mesh_id":            "models/bulb.glb",
        "texture_id":         "textures/bulb.png",
        "emissive_color":     (2.0, 1.8, 0.5),  # HDR values > 1.0 will bloom
        "emissive_intensity": 3.0,
    },
)
```

---

## Test Cases

### Test 1: Bloom disabled by default

- **Expected:** `BloomSettings.enabled == false`; bloom passes not executed.

### Test 2: Threshold pass — sub-threshold pixel outputs black

- **Setup:** Input color `(0.5, 0.5, 0.5)`, `threshold=1.0`, `knee=0.1`.
- **Expected:** Threshold output ≈ `(0, 0, 0)`.

### Test 3: Threshold pass — bright pixel passes through

- **Setup:** Input `(2.0, 2.0, 2.0)`, `threshold=1.0`.
- **Expected:** Threshold output is non-zero, proportional to `2.0 - 1.0 = 1.0`.

### Test 4: Knee smoothly transitions near threshold

- **Setup:** Input luminance exactly at threshold. `knee=0.2`.
- **Expected:** Output weight ∈ (0, 1) — smooth blend, not a hard cut.

### Test 5: Mip chain creates correct number of textures

- **Setup:** `mip_levels=4`, window `1920×1080`.
- **Expected:** `BloomResources.mip_textures.len() == 4`; sizes `960×540`, `480×270`, `240×135`, `120×67` (approx).

### Test 6: `set_bloom_intensity(0)` results in no visible bloom

- **Setup:** HDR scene with bright emissive. `bloom_intensity=0.0`.
- **Expected:** Upsample blend contributes nothing; output identical to no-bloom.

### Test 7: Bloom requires HDR render target

- **Setup:** Attempt to enable bloom without the post-processing HDR target.
- **Expected:** `Err(RendererError::BloomRequiresHdr)` or auto-enable HDR with warning.

### Test 8: `mip_levels` minimum is 2

- **Setup:** `set_bloom_mip_levels(1)`.
- **Expected:** Warning logged; clamped to 2.

### Test 9: Resize correctly re-creates mip chain

- **Setup:** Create at `800×600`. Resize to `1280×720`.
- **Expected:** `BloomResources.threshold_texture` is `640×360`; mip chain re-built.

### Test 10: BloomUniform `texel_size` matches current pass target

- **Setup:** Downsample pass targeting mip level 1 at `480×270`.
- **Expected:** `BloomUniform.texel_size == (1/480, 1/270)`.

---

## Gauntlet of Cubes Demo

**Where:** `game/scripts/main.py` — `init()`, and `game/scripts/levels/arena_3.py` for the lava emissive setup.

**Effect:** Bloom has no visible effect unless the scene has HDR values above `1.0`. The lava pit in Arena 3 — given `emissive_intensity: 2.5` from §4 — becomes the bloom anchor. The orange glow bleeds several pixels outward from the pit surface, illuminating the surrounding dark floor without adding a real light source. Pickup boxes with subtle emissive also get a soft corona, making them easier to spot.

**Example — enable bloom in `game/scripts/main.py`:**

```python
def init():
    rython.physics.set_gravity(0, -20, 0)

    # Post-processing + bloom
    rython.renderer.set_post_process_enabled(True)
    rython.renderer.set_tone_map("aces")
    rython.renderer.set_bloom_enabled(True)
    rython.renderer.set_bloom_threshold(0.9)    # lava at 2.5 intensity will bloom hard
    rython.renderer.set_bloom_intensity(0.06)
    rython.renderer.set_bloom_scatter(0.75)
    rython.renderer.set_bloom_mip_levels(5)

    # ... existing subscriptions ...
```

**Example — lava pit tuned for visible bloom in `game/scripts/levels/arena_3.py`:**

```python
lava = rython.scene.spawn(
    transform=rython.Transform(0, 0.05, 0, scale_x=6, scale_y=0.1, scale_z=6),
    mesh={
        "mesh_id":            "cube",
        "texture_id":         "game/assets/textures/Red/red_box.png",
        "emissive_color":     (1.0, 0.35, 0.0),
        "emissive_intensity": 2.5,   # >1.0 in HDR space — will bloom
    },
    tags={"tags": ["lava"]},
)
```

**Example — pickup corona in `game/scripts/level_builder.py`:**

```python
mesh={
    "mesh_id":            "cube",
    "texture_id":         "game/assets/textures/Green/green_box.png",
    "emissive_color":     (0.0, 1.0, 0.25),
    "emissive_intensity": 1.3,   # just above threshold for a subtle halo
},
```

**Demo moment:** Load Arena 3, stand on the opposite side of the arena from the lava pit, and look toward it. The orange glow should bleed into the black void above the pit. Toggle `set_bloom_enabled(False)` at runtime and the pit immediately looks flat and digital — a clear side-by-side demonstration.

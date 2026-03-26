# Post-Processing Pipeline

**Status:** Pending
**Priority:** High-Impact, Moderate Effort
**SPEC.md entry:** §7

---

## Overview

A composable post-processing pipeline that applies full-screen effects after the 3D scene is rendered into an intermediate HDR render target. The pipeline consists of an ordered chain of passes; each pass reads from the previous output and writes to a ping-pong buffer. The final pass outputs to the swap chain.

Included effects:
- **Tone mapping** — Map HDR → LDR (ACES, Reinhard, or Uncharted2).
- **Exposure control** — Pre-tone-mapping exposure multiplier.
- **Color grading** — Lift/gamma/gain, saturation, contrast.
- **Vignette** — Edge darkening.

Bloom (§13), SSAO apply (§6), and depth fog (§9) also feed into this pipeline as passes.

---

## Rust Implementation

### New Types

**`crates/rython-renderer/src/post_process.rs`** (new file)

```rust
pub enum ToneMapMode {
    None,       // Linear (passthrough, for debugging)
    Reinhard,
    Aces,
    Uncharted2,
}

pub struct PostProcessSettings {
    pub enabled:     bool,

    // Tone mapping
    pub tone_map:    ToneMapMode,     // default: Aces
    pub exposure:    f32,             // pre-tonemap multiplier; default 1.0

    // Color grading
    pub saturation:  f32,             // 0 = greyscale, 1 = identity, 2 = vivid; default 1.0
    pub contrast:    f32,             // 0 = flat, 1 = identity; default 1.0
    pub brightness:  f32,             // additive; default 0.0
    pub gamma:       f32,             // gamma correction; default 2.2

    // Lift / Gamma / Gain (shadows/mids/highlights color correction)
    pub lift:        [f32; 3],        // default [0,0,0]
    pub gg_gamma:    [f32; 3],        // default [1,1,1]
    pub gain:        [f32; 3],        // default [1,1,1]

    // Vignette
    pub vignette_strength: f32,       // 0 = off; default 0.0
    pub vignette_radius:   f32,       // 0.5 = screen edge; default 0.75
}

/// A ping-pong buffer pair for post-process passes.
pub struct PostProcessTargets {
    pub hdr_texture:    wgpu::Texture,   // Rgba16Float — scene render target
    pub hdr_view:       wgpu::TextureView,
    pub ping:           wgpu::Texture,   // Rgba16Float
    pub ping_view:      wgpu::TextureView,
    pub pong:           wgpu::Texture,   // Rgba16Float
    pub pong_view:      wgpu::TextureView,
}

impl PostProcessTargets {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self;
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32);
}

/// Owns pipelines for all post-process passes.
pub struct PostProcessPipeline {
    pub tone_map_pipeline:    wgpu::RenderPipeline,
    pub color_grade_pipeline: wgpu::RenderPipeline,
    pub vignette_pipeline:    wgpu::RenderPipeline,
    pub bgl:                  wgpu::BindGroupLayout,  // input texture + sampler
    pub settings_buf:         wgpu::Buffer,            // uniform: PostProcessUniform
}
```

### HDR Scene Render Target

The main 3D scene is no longer rendered directly to the swap chain. Instead:

1. Scene renders into `PostProcessTargets.hdr_texture` (`Rgba16Float`).
2. Post-process chain executes on the `hdr_texture`.
3. Final LDR result blits to the swap chain surface.

**`crates/rython-renderer/src/gpu.rs`** changes:
- Add `post_targets: PostProcessTargets`.
- Add `post_pipeline: PostProcessPipeline`.
- Main scene render pass uses `hdr_texture` as color attachment.

### Post-Process Uniform

```rust
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PostProcessUniform {
    pub exposure:         f32,
    pub saturation:       f32,
    pub contrast:         f32,
    pub brightness:       f32,
    pub gamma:            f32,
    pub tone_map_mode:    u32,    // 0=None, 1=Reinhard, 2=Aces, 3=Uncharted2
    pub vignette_strength: f32,
    pub vignette_radius:  f32,
    pub lift:             [f32; 4],   // xyz = lift, w = pad
    pub gg_gamma:         [f32; 4],
    pub gain:             [f32; 4],
}
```

### Shader Changes

**`crates/rython-renderer/src/shaders.rs`** — New constant `POST_PROCESS_WGSL`:

```wgsl
@group(0) @binding(0) var t_input:  texture_2d<f32>;
@group(0) @binding(1) var s_input:  sampler;
@group(0) @binding(2) var<uniform> pp: PostProcessUniform;

fn reinhard(x: vec3<f32>) -> vec3<f32> { return x / (x + vec3(1.0)); }

fn aces(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51; let b = 0.03; let c = 2.43; let d = 0.59; let e = 0.14;
    return clamp((x*(a*x+b))/(x*(c*x+d)+e), vec3(0.0), vec3(1.0));
}

fn apply_color_grade(color: vec3<f32>) -> vec3<f32> {
    // Lift/gamma/gain
    var c = color * pp.gain.xyz + pp.lift.xyz;
    c = pow(max(c, vec3(0.0)), 1.0 / pp.gg_gamma.xyz);
    // Saturation
    let luma = dot(c, vec3(0.2126, 0.7152, 0.0722));
    c = mix(vec3(luma), c, pp.saturation);
    // Contrast + brightness
    c = (c - 0.5) * pp.contrast + 0.5 + pp.brightness;
    return max(c, vec3(0.0));
}

@fragment
fn fs_post(in: FullscreenOutput) -> @location(0) vec4<f32> {
    var color = textureSample(t_input, s_input, in.uv).rgb;

    // Exposure
    color *= pp.exposure;

    // Tone map
    switch (pp.tone_map_mode) {
        case 1u: { color = reinhard(color); }
        case 2u: { color = aces(color); }
        default: {}
    }

    // Color grade
    color = apply_color_grade(color);

    // Gamma
    color = pow(max(color, vec3(0.0)), vec3(1.0 / pp.gamma));

    // Vignette
    let uv2 = in.uv - 0.5;
    let vignette = 1.0 - smoothstep(pp.vignette_radius, 1.0, length(uv2) * 2.0);
    color *= mix(1.0, vignette, pp.vignette_strength);

    return vec4(color, 1.0);
}
```

### Frame Loop Changes

**`crates/rython-renderer/src/lib.rs` — `render_frame()`**

```
1. Scene 3D render → hdr_texture
2. (Optional) SSAO, bloom passes on hdr_texture
3. Post-process pass: hdr_texture → swap chain surface
```

### Key Files to Modify

| File | Change |
|------|--------|
| `crates/rython-renderer/src/post_process.rs` | New — all post-process types and pipeline |
| `crates/rython-renderer/src/shaders.rs` | `POST_PROCESS_WGSL` |
| `crates/rython-renderer/src/gpu.rs` | HDR render target, `PostProcessPipeline` |
| `crates/rython-renderer/src/lib.rs` | Updated frame loop |
| `crates/rython-scripting/src/bridge/renderer.rs` | Expose all settings |

---

## Python API

```python
rython.renderer.set_post_process_enabled(True)

# Tone mapping
rython.renderer.set_tone_map("aces")        # "none" | "reinhard" | "aces" | "uncharted2"
rython.renderer.set_exposure(1.2)

# Color grading
rython.renderer.set_saturation(1.1)
rython.renderer.set_contrast(1.05)
rython.renderer.set_brightness(0.0)
rython.renderer.set_gamma(2.2)

# Lift / Gamma / Gain
rython.renderer.set_lift(0.0, 0.0, 0.05)          # cool shadows
rython.renderer.set_gg_gamma(1.0, 1.0, 1.0)
rython.renderer.set_gain(1.1, 1.05, 0.95)          # warm highlights

# Vignette
rython.renderer.set_vignette(strength=0.4, radius=0.7)
```

---

## Test Cases

### Test 1: `PostProcessTargets` creates Rgba16Float textures

- **Setup:** `PostProcessTargets::new(device, 1920, 1080)`.
- **Expected:** `hdr_texture.format() == Rgba16Float`, dimensions `1920×1080`.

### Test 2: Disabled post-processing blits HDR directly to swap chain

- **Setup:** `post_process_settings.enabled = false`.
- **Expected:** No tone map applied; HDR values clamp at 1.0 on LDR display path.

### Test 3: ACES tone mapping maps mid-grey correctly

- **Setup:** Input color `(0.18, 0.18, 0.18)` with exposure `1.0`.
- **Expected:** ACES output ≈ `(0.197, 0.197, 0.197)` (known reference value).

### Test 4: Exposure multiplier scales linearly before tone mapping

- **Setup:** Input `(0.5, 0.5, 0.5)`, exposure `2.0`, tone_map `"none"`.
- **Expected:** Output ≈ `(1.0, 1.0, 1.0)` (clamped).

### Test 5: Saturation 0.0 produces greyscale

- **Setup:** Input `(0.8, 0.2, 0.4)`, saturation `0.0`.
- **Expected:** Output R == G == B (luma-weighted average).

### Test 6: Contrast 1.0 is identity

- **Setup:** Any input, contrast `1.0`, brightness `0.0`.
- **Expected:** Output matches input color (within float tolerance).

### Test 7: Resize re-creates textures at new size

- **Setup:** Create at 800×600. Call `resize(device, 1280, 720)`.
- **Expected:** `hdr_texture` is now `1280×720`; old texture released.

### Test 8: Invalid tone map mode string

- **Setup:** Python call `set_tone_map("unknown")`.
- **Expected:** Warning logged; tone map defaults to `"aces"`.

### Test 9: Vignette strength 0.0 is no-op

- **Setup:** `vignette_strength=0.0`.
- **Expected:** Corner and center pixels have same multiplication factor (1.0).

### Test 10: PostProcessUniform size is a multiple of 16 bytes

- **Setup:** Compute `std::mem::size_of::<PostProcessUniform>()`.
- **Expected:** Size is a multiple of 16 (wgpu uniform alignment requirement).

---

## Gauntlet of Cubes Demo

**Where:** `game/scripts/levels/arena_1.py`, `arena_2.py`, `arena_3.py` — each arena configures its own look on load, and `game/scripts/main.py` `_on_enemy_attack()` drives a damage vignette.

**Effect:** Each arena becomes a distinct visual world rather than three recoloured versions of the same flat-shader scene.

- **Arena 1 (Tutorial):** Bright, high contrast, slightly warm tones — welcoming and readable.
- **Arena 2 (Gauntlet Run):** Cold, slightly desaturated, strong vignette — the void below feels vast and dangerous.
- **Arena 3 (Boss Arena):** Heavy vignette, red lift in shadows, exposure pulled down — oppressive and dramatic.
- **Damage flash:** When `_on_enemy_attack()` fires, briefly raise vignette strength to `0.9` and drop exposure to `0.7`, then lerp back over 0.3 seconds.

**Example — per-arena grading called from each arena's `load()` function:**

```python
# arena_1.py
def _apply_post_process():
    rython.renderer.set_post_process_enabled(True)
    rython.renderer.set_tone_map("aces")
    rython.renderer.set_exposure(1.1)
    rython.renderer.set_saturation(1.05)
    rython.renderer.set_contrast(1.05)
    rython.renderer.set_vignette(strength=0.15, radius=0.8)
    rython.renderer.set_gamma(2.2)

# arena_2.py
def _apply_post_process():
    rython.renderer.set_post_process_enabled(True)
    rython.renderer.set_tone_map("aces")
    rython.renderer.set_exposure(0.9)
    rython.renderer.set_saturation(0.85)   # cold, slightly washed out
    rython.renderer.set_contrast(1.1)
    rython.renderer.set_vignette(strength=0.45, radius=0.65)
    rython.renderer.set_lift(0.0, 0.0, 0.04)  # cool blue shadows

# arena_3.py
def _apply_post_process():
    rython.renderer.set_post_process_enabled(True)
    rython.renderer.set_tone_map("aces")
    rython.renderer.set_exposure(0.8)
    rython.renderer.set_saturation(0.9)
    rython.renderer.set_contrast(1.2)
    rython.renderer.set_vignette(strength=0.55, radius=0.6)
    rython.renderer.set_lift(0.06, 0.0, 0.0)   # red shadows
    rython.renderer.set_gain(1.0, 0.8, 0.8)    # desaturate highlights
```

**Example — damage flash in `game/scripts/main.py`:**

```python
_damage_flash = 0.0

def _on_enemy_attack(data):
    global _damage_flash
    damage = data.get("damage", 10)
    game_state.take_damage(damage)
    _damage_flash = 0.3
    rython.renderer.set_vignette(strength=0.9, radius=0.5)
    rython.renderer.set_exposure(0.65)

def _game_tick():
    global _damage_flash
    dt = rython.time.dt
    if _damage_flash > 0:
        _damage_flash -= dt
        t = max(0.0, _damage_flash / 0.3)
        rython.renderer.set_vignette(strength=0.15 + 0.75 * t, radius=0.8 - 0.3 * t)
        rython.renderer.set_exposure(1.1 - 0.45 * t)
    # ... existing tick ...
```

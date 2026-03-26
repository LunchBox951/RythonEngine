# Screen-Space Reflections (SSR)

**Status:** Pending
**Priority:** Advanced, Higher Effort
**SPEC.md entry:** §19
**Depends on:** deferred-rendering.spec.md (§18) — requires GBuffer with world positions and normals, post-processing.spec.md (§7) — requires HDR scene color texture

---

## Overview

Reflect scene geometry on glossy surfaces by ray-marching in screen space against the previous frame's depth buffer. SSR is a post-process effect that reads the HDR scene color texture and GBuffer, then writes reflection contributions to a reflection buffer that is additively blended into the scene before tone mapping.

Limitations inherent to screen-space techniques (documented for Python developers):
- Reflections only show what is currently on screen.
- Objects outside the view frustum are not reflected.
- Grazing-angle reflections can break.
- Quality degrades for very rough surfaces (fade-out by roughness).

---

## Rust Implementation

### New Types

**`crates/rython-renderer/src/ssr.rs`** (new file)

```rust
pub struct SsrSettings {
    pub enabled:          bool,
    pub max_distance:     f32,    // world-space max ray length; default 10.0
    pub resolution:       f32,    // fraction of screen res; default 0.5 (half)
    pub step_count:       u32,    // ray march steps; default 64
    pub binary_steps:     u32,    // binary refinement steps; default 8
    pub thickness:        f32,    // depth thickness for hit detection; default 0.1
    pub roughness_cutoff: f32,    // surfaces rougher than this are not reflected; default 0.4
    pub intensity:        f32,    // reflection blend weight; default 1.0
    pub fade_edges:       bool,   // fade reflections near screen edges; default true
}

pub struct SsrResources {
    /// Reflection color buffer (same format as HDR scene, half or full resolution)
    pub reflection_texture: wgpu::Texture,
    pub reflection_view:    wgpu::TextureView,

    /// Pipelines
    pub ssr_pipeline:   wgpu::RenderPipeline,
    pub blend_pipeline: wgpu::RenderPipeline,   // additive blend into HDR scene

    pub bgl: wgpu::BindGroupLayout,
    pub settings_buf: wgpu::Buffer,              // SsrUniform
}
```

### SSR Uniform

```rust
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SsrUniform {
    pub max_distance:     f32,
    pub step_count:       u32,
    pub binary_steps:     u32,
    pub thickness:        f32,
    pub roughness_cutoff: f32,
    pub intensity:        f32,
    pub screen_size:      [f32; 2],   // width, height
    pub resolution:       f32,
    pub fade_edges:       u32,        // 0 or 1
    pub _pad:             [f32; 2],
}
```

### Shader

**`crates/rython-renderer/src/shaders.rs`** — New `SSR_WGSL`:

```wgsl
@group(0) @binding(0) var t_scene_color: texture_2d<f32>;   // HDR scene from current frame
@group(0) @binding(1) var t_gbuf_normal: texture_2d<f32>;   // world-space normals
@group(0) @binding(2) var t_gbuf_depth:  texture_depth_2d;  // scene depth
@group(0) @binding(3) var t_gbuf_mr:     texture_2d<f32>;   // metallic/roughness
@group(0) @binding(4) var<uniform> camera: CameraUniform;   // includes inv_view_proj
@group(0) @binding(5) var<uniform> ssr: SsrUniform;
@group(0) @binding(6) var s_linear: sampler;

fn ray_march(
    ray_origin: vec3<f32>,
    ray_dir: vec3<f32>,
) -> vec4<f32> {  // xy=screen UV of hit, z=hit confidence, w=unused
    let step_size = ssr.max_distance / f32(ssr.step_count);
    var t = step_size;
    var hit_uv = vec2(0.0);
    var confidence = 0.0;

    for (var i = 0u; i < ssr.step_count; i++) {
        let sample_pos = ray_origin + ray_dir * t;

        // Project to screen space
        let clip = camera.view_proj * vec4(sample_pos, 1.0);
        let ndc  = clip.xyz / clip.w;
        let uv   = ndc.xy * 0.5 + 0.5;

        if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
            break;
        }

        let scene_depth = textureSampleLevel(t_gbuf_depth, s_linear, uv, 0.0);
        let diff = clip.z / clip.w - scene_depth;

        if (diff > 0.0 && diff < ssr.thickness) {
            // Binary refinement
            var lo = t - step_size;
            var hi = t;
            for (var j = 0u; j < ssr.binary_steps; j++) {
                let mid = (lo + hi) * 0.5;
                let mid_pos  = ray_origin + ray_dir * mid;
                let mid_clip = camera.view_proj * vec4(mid_pos, 1.0);
                let mid_uv   = mid_clip.xy / mid_clip.w * 0.5 + 0.5;
                let mid_scene_depth = textureSampleLevel(t_gbuf_depth, s_linear, mid_uv, 0.0);
                if (mid_clip.z / mid_clip.w > mid_scene_depth) {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            let final_uv = (camera.view_proj * vec4(ray_origin + ray_dir * hi, 1.0)).xy;
            hit_uv   = final_uv / (camera.view_proj * vec4(ray_origin + ray_dir * hi, 1.0)).w * 0.5 + 0.5;
            confidence = 1.0;

            // Edge fade
            if (ssr.fade_edges != 0u) {
                let edge = min(hit_uv.x, min(1.0 - hit_uv.x, min(hit_uv.y, 1.0 - hit_uv.y)));
                confidence *= smoothstep(0.0, 0.1, edge);
            }
            break;
        }
        t += step_size;
    }
    return vec4(hit_uv, confidence, 0.0);
}

@fragment
fn fs_ssr(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let N = textureSample(t_gbuf_normal, s_linear, in.uv).rgb * 2.0 - 1.0;
    let mr = textureSample(t_gbuf_mr, s_linear, in.uv);
    let roughness = mr.g;

    if (roughness > ssr.roughness_cutoff) { return vec4(0.0); }

    let depth = textureSampleLevel(t_gbuf_depth, s_linear, in.uv, 0.0);
    let ndc = vec4(in.uv * 2.0 - 1.0, depth, 1.0);
    let world = camera.inv_view_proj * ndc;
    let world_pos = world.xyz / world.w;

    let view_dir = normalize(camera.eye_position - world_pos);
    let reflect_dir = reflect(-view_dir, normalize(N));

    let hit = ray_march(world_pos, reflect_dir);
    if (hit.z < 0.001) { return vec4(0.0); }

    let reflection = textureSample(t_scene_color, s_linear, hit.xy).rgb;
    let roughness_fade = 1.0 - roughness / ssr.roughness_cutoff;
    return vec4(reflection * hit.z * roughness_fade * ssr.intensity, 1.0);
}
```

### Frame Loop Integration

**`crates/rython-renderer/src/lib.rs`** — after 3D scene render, before bloom:

```
1. 3D scene render → hdr_texture (and GBuffer if deferred)
2. SSR pass: hdr_texture + GBuffer → reflection_texture
3. Blend: reflection_texture additively into hdr_texture
4. Bloom passes
5. Post-process (tone map, etc.) → swap chain
```

### Key Files to Modify

| File | Change |
|------|--------|
| `crates/rython-renderer/src/ssr.rs` | New — `SsrSettings`, `SsrResources`, `SsrUniform` |
| `crates/rython-renderer/src/shaders.rs` | `SSR_WGSL` |
| `crates/rython-renderer/src/gpu.rs` | `SsrResources` field |
| `crates/rython-renderer/src/lib.rs` | SSR pass in frame loop |
| `crates/rython-scripting/src/bridge/renderer.rs` | Expose SSR settings |

---

## Python API

```python
rython.renderer.set_ssr_enabled(True)
rython.renderer.set_ssr_max_distance(10.0)
rython.renderer.set_ssr_steps(64)               # higher = better quality, slower
rython.renderer.set_ssr_binary_steps(8)
rython.renderer.set_ssr_thickness(0.1)
rython.renderer.set_ssr_roughness_cutoff(0.4)   # rougher surfaces are not reflected
rython.renderer.set_ssr_intensity(0.8)
rython.renderer.set_ssr_fade_edges(True)
rython.renderer.set_ssr_resolution(0.5)         # 0.5 = half-resolution
```

---

## Test Cases

### Test 1: SSR disabled by default

- **Expected:** `SsrSettings.enabled == false`; SSR pass not executed.

### Test 2: Surfaces rougher than `roughness_cutoff` output black

- **Setup:** GBuffer fragment with `roughness=0.6`, `roughness_cutoff=0.4`.
- **Expected:** SSR output for that fragment is `vec4(0.0)`.

### Test 3: Ray-march terminates at screen edge

- **Setup:** Ray marching toward screen edge; no hit before edge.
- **Expected:** Function returns `confidence=0.0`; no out-of-bounds texture access.

### Test 4: Binary refinement reduces thickness error

- **Setup:** Step ray over a depth step. Compare hit position with `binary_steps=0` vs `binary_steps=8`.
- **Expected:** 8-step version hits closer to the actual depth edge.

### Test 5: Edge fade reduces confidence near screen boundary

- **Setup:** `fade_edges=True`. Ray hits at UV `(0.02, 0.5)` (near left edge).
- **Expected:** `confidence < 0.5` (fade applied).

### Test 6: Edge fade disabled — confidence at screen edge is not reduced

- **Setup:** `fade_edges=False`. Hit at UV `(0.01, 0.5)`.
- **Expected:** `confidence == 1.0` (no fade).

### Test 7: Reflection blended additively into scene

- **Setup:** Inspect blend pipeline state.
- **Expected:** `src_factor = One`, `dst_factor = One` (additive blend).

### Test 8: Half-resolution SSR allocates smaller texture

- **Setup:** Window 1920×1080, `resolution=0.5`.
- **Expected:** `reflection_texture` is 960×540.

### Test 9: SSR requires deferred rendering

- **Setup:** Attempt to enable SSR in forward rendering mode.
- **Expected:** `Err(RendererError::SsrRequiresDeferred)` or warning + auto-switch.

### Test 10: `SsrUniform` size is multiple of 16 bytes

- **Expected:** `std::mem::size_of::<SsrUniform>() % 16 == 0`.

### Test 11: No-hit areas sample black, not garbage

- **Setup:** Large open sky scene; many rays miss geometry.
- **Expected:** Output pixels for miss rays are `(0,0,0,1)`, not NaN or random values.

---

## Gauntlet of Cubes Demo

**Where:** `game/scripts/levels/arena_3.py` — lava pit and boss arena floor.

**Effect:** Arena 3's lava pit is a flat red surface surrounded by dark floor tiles. With SSR, the lava surface reflects the enemies walking around it, the perimeter walls, and the player's shadow. As the boss emerges from the far side of the arena, its silhouette appears in the lava's reflection before the player sees it directly — a dramatic reveal moment that emerges entirely from the rendering system, not any scripted event.

The dark arena floor tiles (low roughness via PBR or `roughness=0.15` uniform) also pick up SSR, showing faint reflections of nearby enemies and walls.

**Example — enable SSR in `game/scripts/main.py` `init()`:**

```python
def init():
    rython.renderer.set_render_path("deferred")   # SSR requires deferred
    rython.renderer.set_post_process_enabled(True)
    rython.renderer.set_tone_map("aces")

    rython.renderer.set_ssr_enabled(True)
    rython.renderer.set_ssr_max_distance(12.0)    # lava pit is ~6 units wide; reflect walls at ~11 units
    rython.renderer.set_ssr_steps(48)
    rython.renderer.set_ssr_binary_steps(6)
    rython.renderer.set_ssr_thickness(0.15)
    rython.renderer.set_ssr_roughness_cutoff(0.25)  # only mirror-like surfaces reflect
    rython.renderer.set_ssr_intensity(0.7)
    rython.renderer.set_ssr_fade_edges(True)
    rython.renderer.set_ssr_resolution(0.5)        # half-res for performance

    rython.physics.set_gravity(0, -20, 0)
    # ...
```

**Example — lava and floor configured for reflectivity in `game/scripts/levels/arena_3.py`:**

```python
# Lava pit — smooth surface, SSR will reflect enemies above it
lava = rython.scene.spawn(
    transform=rython.Transform(0, 0.05, 0, scale_x=6, scale_y=0.1, scale_z=6),
    pbr_mesh={
        "mesh_id":         "cube",
        "albedo_map":      "game/assets/textures/Red/red_box.png",
        "metallic_factor": 0.0,
        "roughness_factor": 0.08,     # smooth enough for SSR
        "emissive_factor": (1.0, 0.3, 0.0),
    },
    tags={"tags": ["lava"]},
)

# Dark floor — slightly reflective
floor = rython.scene.spawn(
    transform=rython.Transform(0, -0.5, 0, scale_x=22, scale_y=0.5, scale_z=22),
    pbr_mesh={
        "mesh_id":         "cube",
        "albedo_map":      "game/assets/textures/Dark/dark_floor_grid.png",
        "metallic_factor": 0.3,
        "roughness_factor": 0.2,      # just below cutoff — faint reflections only
    },
    tags={"tags": ["static"]},
)
```

**Demo moment:** Position the player near the lava pit and walk an enemy skeleton past the far side. The enemy's reflection should track it in the lava surface. Toggle `set_ssr_enabled(False)` and the lava immediately becomes a flat emissive red tile — no reflection at all.

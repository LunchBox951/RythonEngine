# Deferred Rendering

**Status:** Pending
**Priority:** Advanced, Higher Effort
**SPEC.md entry:** §18
**Depends on:** multiple-lights.spec.md (§5), normal-mapping.spec.md (§1), pbr.spec.md (§16)
**Unlocks:** ssao.spec.md (§6) — simplifies GBuffer reuse, ssr.spec.md (§19)

---

## Overview

Replace the forward rendering pipeline with a geometry pass → light pass architecture. All scene geometry is drawn once into a GBuffer; then a screen-space light pass accumulates all lights without re-reading geometry. This makes many dynamic lights feasible without per-mesh × per-light draw calls.

**GBuffer layout:**

| Texture | Format | Contents |
|---------|--------|----------|
| G0 — Albedo+AO | Rgba8Unorm | RGB=albedo, A=ambient occlusion |
| G1 — Normal | Rgba16Float | RGB=world-space normal (signed), A=unused |
| G2 — Metallic+Roughness+Emissive | Rgba8Unorm | R=metallic, G=roughness, BA=emissive mask |
| G3 — Depth | Depth32Float | Linear depth (reused from shadow spec) |

The deferred pipeline is opt-in at `GpuContext` creation. Forward rendering remains available for simple projects that don't need many lights.

---

## Rust Implementation

### New Types

**`crates/rython-renderer/src/deferred.rs`** (new file)

```rust
pub struct GBuffer {
    pub albedo_ao:          wgpu::Texture,    // Rgba8Unorm
    pub normal:             wgpu::Texture,    // Rgba16Float
    pub metallic_roughness: wgpu::Texture,    // Rgba8Unorm
    pub depth:              wgpu::Texture,    // Depth32Float
    pub views: GBufferViews,
    pub bind_group:         wgpu::BindGroup,  // for light pass — all 4 textures + samplers
}

pub struct GBufferViews {
    pub albedo_ao:          wgpu::TextureView,
    pub normal:             wgpu::TextureView,
    pub metallic_roughness: wgpu::TextureView,
    pub depth:              wgpu::TextureView,
}

impl GBuffer {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self;
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32);
}

pub enum RenderPath {
    Forward,    // existing pipeline
    Deferred,   // GBuffer + light pass
}
```

**`crates/rython-renderer/src/gpu.rs`**

```rust
pub render_path:  RenderPath,
pub gbuffer:      Option<GBuffer>,    // Some when deferred
pub light_pass_pipeline: Option<wgpu::RenderPipeline>,
pub light_pass_bgl:      Option<wgpu::BindGroupLayout>,
```

### Geometry Pass

The geometry pass renders all visible meshes into the GBuffer attachments. The shader writes material data to the 4 render targets instead of computing lighting.

**`crates/rython-renderer/src/shaders.rs`** — New `DEFERRED_GEOMETRY_WGSL`:

```wgsl
struct GBufferOutput {
    @location(0) albedo_ao:          vec4<f32>,
    @location(1) normal:             vec4<f32>,
    @location(2) metallic_roughness: vec4<f32>,
};

@fragment
fn fs_geometry(in: VertexOutput) -> GBufferOutput {
    var out: GBufferOutput;

    // Albedo
    var albedo = model.color.rgb;
    if (model.has_texture != 0u) {
        albedo *= textureSample(t_diffuse, s_diffuse, in.uv).rgb;
    }

    // Normal (world-space, packed to [0,1])
    var N = normalize(in.world_normal);
    if (model.has_normal_map != 0u) {
        N = compute_tbn_normal(in.uv, in.world_tangent, in.world_bitangent, N);
    }

    // AO (from AO map if present; else 1.0)
    let ao = select(1.0, textureSample(t_ao, s_ao, in.uv).r, model.has_ao != 0u);

    out.albedo_ao          = vec4(albedo, ao);
    out.normal             = vec4(N * 0.5 + 0.5, 0.0);
    out.metallic_roughness = vec4(model.metallic, model.roughness, 0.0, 0.0);
    return out;
}
```

### Light Pass

A full-screen quad that reads the GBuffer and accumulates all lights:

**`crates/rython-renderer/src/shaders.rs`** — New `DEFERRED_LIGHT_WGSL`:

```wgsl
@group(0) @binding(0) var t_albedo_ao:    texture_2d<f32>;
@group(0) @binding(1) var t_normal:       texture_2d<f32>;
@group(0) @binding(2) var t_mr:           texture_2d<f32>;
@group(0) @binding(3) var t_depth:        texture_depth_2d;
@group(0) @binding(4) var s_gbuf:         sampler;
@group(1) @binding(0) var<uniform> light_buf: LightBuffer;
@group(2) @binding(0) var<uniform> camera: CameraUniform;

@fragment
fn fs_light(in: FullscreenOutput) -> @location(0) vec4<f32> {
    // Reconstruct world position from depth + inverse VP
    let depth = textureSample(t_depth, s_gbuf, in.uv);
    let ndc   = vec4(in.uv * 2.0 - 1.0, depth, 1.0);
    let world = camera.inv_view_proj * ndc;
    let world_pos = world.xyz / world.w;

    // Read GBuffer
    let albedo_ao = textureSample(t_albedo_ao, s_gbuf, in.uv);
    let albedo = albedo_ao.rgb;
    let ao = albedo_ao.a;
    let N = textureSample(t_normal, s_gbuf, in.uv).rgb * 2.0 - 1.0;
    let mr = textureSample(t_mr, s_gbuf, in.uv);
    let metallic  = mr.r;
    let roughness = mr.g;

    // Accumulate all lights using PBR BRDF (same as pbr.spec.md)
    // ...
    return vec4(final_color, 1.0);
}
```

**`CameraUniform` extension** — Add inverse view-projection for world position reconstruction:

```rust
pub struct CameraUniform {
    pub view_proj:     [[f32; 4]; 4],
    pub inv_view_proj: [[f32; 4]; 4],   // NEW — needed for deferred world pos reconstruct
    pub view:          [[f32; 4]; 4],
    pub proj:          [[f32; 4]; 4],
    pub eye_position:  [f32; 3],
    pub _pad:          f32,
}
```

### Transparency in Deferred

Transparent geometry cannot be deferred (blending incompatible with MRT). Transparent entities are always rendered in a forward pass after the deferred light accumulation, reusing the depth buffer from the GBuffer for correct occlusion.

### Key Files to Modify

| File | Change |
|------|--------|
| `crates/rython-renderer/src/deferred.rs` | New — `GBuffer`, `GBufferViews`, `RenderPath` |
| `crates/rython-renderer/src/shaders.rs` | `DEFERRED_GEOMETRY_WGSL`, `DEFERRED_LIGHT_WGSL` |
| `crates/rython-renderer/src/gpu.rs` | `render_path`, `gbuffer`, `light_pass_pipeline`; `inv_view_proj` |
| `crates/rython-renderer/src/camera.rs` | `inv_view_proj` in `CameraUniform` |
| `crates/rython-renderer/src/lib.rs` | Two-pass frame loop: geometry → light |

---

## Python API

```python
# Select rendering path at startup (before first frame)
rython.renderer.set_render_path("deferred")   # "forward" | "deferred"

# Query active path
path = rython.renderer.get_render_path()      # returns "forward" or "deferred"
```

---

## Test Cases

### Test 1: GBuffer textures have correct formats

- **Setup:** `GBuffer::new(device, 1920, 1080)`.
- **Expected:** `albedo_ao.format == Rgba8Unorm`, `normal.format == Rgba16Float`, `depth.format == Depth32Float`.

### Test 2: GBuffer textures match swap chain dimensions

- **Setup:** Window 1280×720. Create GBuffer.
- **Expected:** All GBuffer textures are exactly 1280×720.

### Test 3: Normal GBuffer encodes to [0,1] and decodes back

- **Setup:** World normal `(0, 1, 0)`. Encode in geometry pass. Decode in light pass.
- **Expected:** Decoded normal ≈ `(0, 1, 0)` within float tolerance.

### Test 4: World position reconstructed from depth

- **Setup:** Fragment at known world position `(3, 2, 1)`. Record depth. Reconstruct.
- **Expected:** Reconstructed position ≈ `(3, 2, 1)` within 0.01 units.

### Test 5: `inv_view_proj` is correct inverse

- **Setup:** Compute `view_proj` then `inv_view_proj`. Verify `view_proj * inv_view_proj ≈ Identity`.
- **Expected:** Matrix product within 1e-5 of identity.

### Test 6: Transparent entities fall back to forward pass

- **Setup:** Deferred mode. Blend entity.
- **Expected:** Blend entity not written to GBuffer; rendered in forward sub-pass after light accumulation.

### Test 7: GBuffer resize works after window resize

- **Setup:** Resize window from 800×600 to 1920×1080.
- **Expected:** `GBuffer::resize()` called; new textures are 1920×1080.

### Test 8: Light pass reads all 4 GBuffer textures

- **Setup:** Inspect light pass bind group.
- **Expected:** 4 texture bindings + 1 sampler (albedo_ao, normal, mr, depth) all present.

### Test 9: Forward path unaffected when deferred not selected

- **Setup:** Default `render_path = Forward`.
- **Expected:** `gbuffer == None`; no geometry pre-pass runs; existing forward pipeline used.

### Test 10: Switching render paths mid-session reallocates GBuffer

- **Setup:** Start forward. Call `set_render_path("deferred")` after first frame.
- **Expected:** GBuffer allocated; subsequent frames use deferred path.

### Test 11: Empty scene light pass outputs ambient-only color

- **Setup:** Deferred mode. No geometry. Light pass runs on empty GBuffer.
- **Expected:** Output pixel = ambient light color (sky color or ambient term), no NaN.

---

## Gauntlet of Cubes Demo

**Where:** `game/scripts/main.py` — `init()` (render path selection); `game/scripts/levels/arena_3.py` — multiple dynamic lights.

**Effect:** The primary motivation for deferred rendering in this demo is Arena 3's boss fight. Wave 2 spawns 11 enemies simultaneously. If each enemy has an accompanying point light (a faint purple glow around each skeleton) that would be 11+ lights — well beyond what forward rendering handles efficiently. With deferred, adding 11 more point lights in the light pass has negligible CPU cost.

**Example — select deferred at startup in `game/scripts/main.py`:**

```python
def init():
    rython.renderer.set_render_path("deferred")
    rython.physics.set_gravity(0, -20, 0)
    # ... rest of init ...
```

**Example — per-enemy ambient glow light in `game/scripts/level_builder.py`:**

```python
def spawn_enemy(x, y, z, enemy_type, is_boss=False, tags=None):
    scale = (1.5, 2.5, 1.5) if is_boss else (1.0, 2.0, 1.0)
    entity = rython.scene.spawn(
        transform=rython.Transform(x, y, z,
            scale_x=scale[0], scale_y=scale[1], scale_z=scale[2]),
        mesh={
            "mesh_id":    "cube",
            "texture_id": "game/assets/textures/Purple/purple_box.png",
        },
        # Attach a co-located point light — only feasible under deferred
        light={
            "type":      "point",
            "color":     (0.6, 0.0, 0.9) if not is_boss else (1.0, 0.0, 0.5),
            "intensity": 3.0 if is_boss else 1.2,
            "radius":    4.0 if is_boss else 2.5,
        },
        rigid_body={"body_type": "dynamic", "mass": 5.0 if is_boss else 2.0},
        collider={"shape": "box", "size": list(scale)},
        tags={"tags": tags or ["enemy"]},
    )
    enemies.register(entity, enemy_type, is_boss)
    _registered.append(entity)
    return entity
```

**Result in Arena 3 wave 2:** 6 regular enemies emit purple ambient light + 1 boss emits pink light + lava point light + arena directional = 8 simultaneous dynamic lights, all resolved in a single light pass over the GBuffer. Each enemy's light moves with it as the AI chases the player — creating a dynamic, moody lighting scene that was impossible in forward mode.

# Parallax / Relief Mapping

**Status:** Pending
**Priority:** Advanced, Higher Effort
**SPEC.md entry:** §17
**Depends on:** normal-mapping.spec.md (§1) — requires TBN matrix in fragment shader

---

## Overview

Parallax mapping offsets the texture UV based on a height map to create an illusion of depth without extra geometry. Three quality levels are supported:

- **Parallax Offset Mapping (POM)** — Single-sample UV offset. Cheapest.
- **Steep Parallax Mapping** — Iterative ray march with N steps. Decent quality.
- **Parallax Occlusion Mapping (POM Full)** — Steep + binary-search refinement + self-shadowing. Best quality.

Quality level is a per-material setting.

---

## Rust Implementation

### Modified Types

**`crates/rython-ecs/src/component.rs` — `MeshComponent`**

```rust
#[derive(Clone, Debug)]
pub enum ParallaxQuality {
    Off,
    Offset,   // Simple parallax offset
    Steep,    // N-step ray march
    Full,     // Steep + binary search + self-shadow
}

pub struct MeshComponent {
    // ... existing fields ...
    pub height_map_id:      Option<String>,   // NEW — grayscale height map (R channel)
    pub parallax_scale:     f32,              // NEW — depth illusion strength; default 0.05
    pub parallax_quality:   ParallaxQuality,  // NEW — default Off
    pub parallax_steps:     u32,              // NEW — ray march steps (Steep/Full only); default 16
}
```

Height map convention: `0.0 = lowest (deepest), 1.0 = highest (surface level)`. Negative scale inverts (useful when height is stored as depth).

### GPU Binding

**`crates/rython-renderer/src/gpu.rs`**

```rust
pub mesh_height_map_bgl: wgpu::BindGroupLayout,  // @group(11) binding(0,1)
```

Fallback texture for `height_map_id = None`: 1×1 flat `R=128` (mid-height, no parallax displacement).

### Model Uniform Changes

**`crates/rython-renderer/src/shaders.rs`**

```wgsl
struct ModelUniform {
    // ... existing ...
    parallax_scale:   f32,    // NEW
    parallax_steps:   u32,    // NEW
    parallax_quality: u32,    // NEW — 0=off, 1=offset, 2=steep, 3=full
    has_height_map:   u32,    // NEW
};
```

### Shader Changes

**`crates/rython-renderer/src/shaders.rs` — `MESH_WGSL`**

Parallax functions in the fragment shader. UV must be computed in tangent space:

```wgsl
@group(11) @binding(0) var t_height_map: texture_2d<f32>;
@group(11) @binding(1) var s_height_map: sampler;

/// Simple parallax offset — single sample
fn parallax_offset(uv: vec2<f32>, view_ts: vec3<f32>, scale: f32) -> vec2<f32> {
    let h = textureSample(t_height_map, s_height_map, uv).r;
    let offset = view_ts.xy / view_ts.z * (h * scale);
    return uv + offset;
}

/// Steep parallax — N-step ray march
fn parallax_steep(uv: vec2<f32>, view_ts: vec3<f32>, scale: f32, steps: u32) -> vec2<f32> {
    let layer_depth = 1.0 / f32(steps);
    let delta_uv = view_ts.xy / view_ts.z * scale / f32(steps);
    var current_uv = uv;
    var current_depth = 0.0;
    for (var i = 0u; i < steps; i++) {
        current_uv -= delta_uv;
        current_depth += layer_depth;
        let map_depth = 1.0 - textureSample(t_height_map, s_height_map, current_uv).r;
        if (current_depth >= map_depth) { break; }
    }
    return current_uv;
}

/// Full POM — steep + binary search refinement
fn parallax_full(uv: vec2<f32>, view_ts: vec3<f32>, scale: f32, steps: u32) -> vec2<f32> {
    // Run steep pass first
    // Then binary search between last two samples for sub-step precision
    // ... (5 binary refinement iterations)
}

// In fs_main: compute tangent-space view direction, then select quality:
let view_ts = normalize(TBN_inv * view_dir);  // TBN inverse = transpose for orthonormal
var displaced_uv = in.uv;
if (model.has_height_map != 0u) {
    switch (model.parallax_quality) {
        case 1u: { displaced_uv = parallax_offset(in.uv, view_ts, model.parallax_scale); }
        case 2u: { displaced_uv = parallax_steep(in.uv, view_ts, model.parallax_scale, model.parallax_steps); }
        case 3u: { displaced_uv = parallax_full(in.uv, view_ts, model.parallax_scale, model.parallax_steps); }
        default: {}
    }
}
// Use displaced_uv for all subsequent texture samples (albedo, normal, specular)
```

The displaced UV is used for **all** texture samples in the fragment shader, including the normal map — this is critical for correct results.

### Key Files to Modify

| File | Change |
|------|--------|
| `crates/rython-ecs/src/component.rs` | `height_map_id`, `parallax_scale`, `parallax_quality`, `parallax_steps` |
| `crates/rython-renderer/src/shaders.rs` | Parallax functions in `MESH_WGSL`; `displaced_uv` |
| `crates/rython-renderer/src/gpu.rs` | `mesh_height_map_bgl`, flat fallback texture |
| `crates/rython-renderer/src/lib.rs` | Bind height map per entity; populate new `ModelUniform` fields |

---

## Python API

### Scene Spawn Changes

```python
entity = rython.scene.spawn(
    transform=rython.Transform(0, 0, 0),
    mesh={
        "mesh_id":          "models/wall.glb",
        "texture_id":       "textures/brick_diffuse.png",
        "normal_map":       "textures/brick_normal.png",
        "height_map":       "textures/brick_height.png",   # NEW
        "parallax_scale":   0.07,                          # NEW
        "parallax_quality": "full",                        # NEW — "off"|"offset"|"steep"|"full"
        "parallax_steps":   32,                            # NEW — optional, default 16
    },
)
```

---

## Test Cases

### Test 1: Parallax off by default

- **Setup:** Spawn mesh without height map.
- **Expected:** `parallax_quality == Off`; `has_height_map == 0`; UV unchanged.

### Test 2: Parallax offset UV shifts proportionally to height

- **Setup:** Flat height map (all `R=0.8`), `parallax_scale=0.1`, view from 45°.
- **Expected:** `displaced_uv` shifts by `~0.08` in the view direction.

### Test 3: `parallax_quality="full"` requires `height_map` — error if absent

- **Setup:** Set `parallax_quality="full"` without `height_map`.
- **Expected:** Warning logged; quality falls back to `"off"`.

### Test 4: Parallax UV clamped to `[0,1]` (no border sampling)

- **Setup:** Height map with high displacement near UV edge `(0.98, 0.5)`.
- **Expected:** Displaced UV clamped to valid range; no out-of-bounds sample.

### Test 5: Steep parallax step count affects quality

- **Setup:** Compare `parallax_steps=4` vs `parallax_steps=32` with a sharp depth step in height map.
- **Expected:** Lower step count shows more stairstepping; higher step count approximates ground truth.

### Test 6: Full POM binary search reduces aliasing vs steep

- **Setup:** Height map with a sharp diagonal edge. Compare `steep` vs `full` output.
- **Expected:** `full` has sub-step precision; edge is smoother.

### Test 7: Displaced UV used for normal map sample

- **Setup:** Height map at UV `(0.5,0.5)` causes offset to `(0.55,0.5)`. Verify normal map sample UV.
- **Expected:** Normal map is sampled at `(0.55,0.5)`, not `(0.5,0.5)`.

### Test 8: `parallax_scale` negative inverts displacement direction

- **Setup:** Same height map, `parallax_scale=-0.05`.
- **Expected:** UV offsets in opposite direction vs positive scale.

### Test 9: Fallback height texture is mid-grey

- **Setup:** Headless startup.
- **Expected:** 1×1 fallback texture `R=128` (height=0.5; zero net displacement for symmetric scale).

### Test 10: Large `parallax_steps` does not exceed WGSL loop limit

- **Setup:** `parallax_steps=256`.
- **Expected:** Clamped to maximum allowed (`64` or shader-defined limit); warning logged.

---

## Gauntlet of Cubes Demo

**Where:** `game/scripts/levels/arena_1.py` — floor and platform surfaces; `game/scripts/levels/arena_3.py` — dark floor and perimeter walls.

**Effect:** Every surface in the game is currently a flat cube face. Parallax mapping makes the Arena 1 stone-tile floor feel like actual recessed tiles — the edges have visible depth relative to the grout lines as the camera angle changes. Arena 3's dark floor tiles get a rough stone relief that makes the boss arena feel ancient and heavy.

The effect is most visible when the player is moving; parallax shifts the surface texture with camera angle in a way normal mapping alone cannot.

**Example — Arena 1 floor with parallax in `game/scripts/levels/arena_1.py`:**

```python
def load():
    # Floor — full POM for maximum detail
    rython.scene.spawn(
        transform=rython.Transform(0, -0.5, 0, scale_x=20, scale_y=0.5, scale_z=20),
        mesh={
            "mesh_id":          "cube",
            "texture_id":       "game/assets/textures/Light/light_floor_grid.png",
            "normal_map":       "game/assets/textures/Light/light_floor_grid_n.png",
            "height_map":       "game/assets/textures/Light/light_floor_grid_h.png",
            "parallax_scale":   0.06,
            "parallax_quality": "full",
            "parallax_steps":   24,
        },
        tags={"tags": ["static"]},
    )

    # Raised platforms — simpler offset parallax (fewer steps, moving geometry)
    for (x, y, z, sx, sz) in platform_defs:
        rython.scene.spawn(
            transform=rython.Transform(x, y, z, scale_x=sx, scale_y=0.3, scale_z=sz),
            mesh={
                "mesh_id":          "cube",
                "texture_id":       "game/assets/textures/Light/light_box.png",
                "height_map":       "game/assets/textures/Light/light_box_h.png",
                "parallax_scale":   0.04,
                "parallax_quality": "offset",   # cheap; good enough for small platforms
            },
            tags={"tags": ["static", "platform"]},
        )
```

**Example — Arena 3 dark floor in `game/scripts/levels/arena_3.py`:**

```python
rython.scene.spawn(
    transform=rython.Transform(0, -0.5, 0, scale_x=22, scale_y=0.5, scale_z=22),
    mesh={
        "mesh_id":          "cube",
        "texture_id":       "game/assets/textures/Dark/dark_floor_grid.png",
        "normal_map":       "game/assets/textures/Dark/dark_floor_grid_n.png",
        "height_map":       "game/assets/textures/Dark/dark_floor_grid_h.png",
        "parallax_scale":   0.08,          # deeper relief for dramatic boss-arena feel
        "parallax_quality": "steep",
        "parallax_steps":   16,
    },
    tags={"tags": ["static"]},
)
```

**Assets to add:** `*_h.png` grayscale height maps alongside each existing diffuse texture. A simple 64×64 tile with dark grout and bright tile-face is sufficient; the shader handles the rest.

# Emissive Materials

**Status:** Pending
**Priority:** High-Impact, Moderate Effort
**SPEC.md entry:** §4

---

## Overview

Per-mesh emissive light: surfaces can glow without being affected by scene lighting. Emissive is added on top of the final lit color. Two sources are supported:

- **Emissive color** — flat RGBA applied to the entire surface.
- **Emissive texture** — per-texel emissive stored in a separate map; multiplied by `emissive_intensity`.

Both may be active simultaneously; the texture is modulated by the color.

---

## Rust Implementation

### Modified Types

**`crates/rython-ecs/src/component.rs` — `MeshComponent`**

```rust
pub struct MeshComponent {
    pub mesh_id:           String,
    pub texture_id:        String,
    pub normal_map_id:     Option<String>,
    pub specular_map_id:   Option<String>,
    pub emissive_map_id:   Option<String>,   // NEW — optional emissive texture (RGB)
    pub emissive_color:    [f32; 4],         // NEW — RGBA linear; default [0,0,0,0] (off)
    pub emissive_intensity: f32,             // NEW — scalar multiplier; default 1.0
    pub shininess:         f32,
    pub specular_color:    [f32; 3],
    pub yaw_offset:        f32,
    pub visible:           bool,
}
```

`emissive_color[3]` (alpha) is currently unused but reserved for future bloom threshold masking.

### New GPU Binding

**`crates/rython-renderer/src/gpu.rs` — `BindGroupLayouts`**

```rust
pub mesh_emissive_map: wgpu::BindGroupLayout,  // @group(6) binding(0,1)
```

Fallback texture for `emissive_map_id = None`: 1×1 black `(0, 0, 0)`.

### Model Uniform Changes

**`crates/rython-renderer/src/shaders.rs`**

```wgsl
struct ModelUniform {
    model:              mat4x4<f32>,
    color:              vec4<f32>,
    specular_color:     vec4<f32>,
    emissive_color:     vec4<f32>,    // NEW — xyz = color, w = intensity
    has_texture:        u32,
    has_normal_map:     u32,
    has_specular_map:   u32,
    has_emissive_map:   u32,          // NEW
};
```

`emissive_color.w` stores `emissive_intensity` to pack both into one vec4.

### Shader Changes

**`crates/rython-renderer/src/shaders.rs` — `MESH_WGSL`**

```wgsl
@group(6) @binding(0) var t_emissive: texture_2d<f32>;
@group(6) @binding(1) var s_emissive: sampler;

// In fs_main, after computing lit_color:
var emissive = model.emissive_color.xyz * model.emissive_color.w;
if (model.has_emissive_map != 0u) {
    let emissive_sample = textureSample(t_emissive, s_emissive, in.uv).rgb;
    emissive = emissive * emissive_sample;
}
let final_color = lit_color + emissive;
return vec4(final_color, base_alpha);
```

The emissive term is added **after** shadow and lighting, ensuring it is unaffected by shadows.

### Key Files to Modify

| File | Change |
|------|--------|
| `crates/rython-ecs/src/component.rs` | Add `emissive_map_id`, `emissive_color`, `emissive_intensity` |
| `crates/rython-renderer/src/shaders.rs` | Emissive bind group + additive term in `MESH_WGSL` |
| `crates/rython-renderer/src/gpu.rs` | `mesh_emissive_map` layout, black fallback texture |
| `crates/rython-renderer/src/lib.rs` | Bind emissive texture; populate `ModelUniform.emissive_color` |

---

## Python API

### Scene Spawn Changes

```python
entity = rython.scene.spawn(
    transform=rython.Transform(0, 0, 0),
    mesh={
        "mesh_id":            "models/lantern.glb",
        "texture_id":         "textures/lantern_diffuse.png",
        "emissive_map":       "textures/lantern_emissive.png",  # NEW
        "emissive_color":     (1.0, 0.8, 0.2),                  # warm yellow glow
        "emissive_intensity": 2.5,                              # NEW — multiplier
    },
)
```

### Dict Schema Addition

```python
{
    "emissive_map":       str | None,                    # optional
    "emissive_color":     tuple[float,float,float],      # optional, default (0,0,0)
    "emissive_intensity": float,                         # optional, default 1.0
}
```

### Runtime Updates

Emissive values may be changed per-frame through the component system:

```python
# Pulse emissive intensity on a heartbeat timer
def pulse(entity: rython.Entity, t: float):
    mesh = entity.get_component("MeshComponent")
    mesh.emissive_intensity = 1.0 + 0.5 * math.sin(t * 6.0)
```

(Requires `entity.get_component()` — see ECS Python bridge spec.)

---

## Test Cases

### Test 1: Default emissive is off

- **Setup:** Spawn mesh without any emissive keys.
- **Expected:** `emissive_color == [0,0,0,0]`, `emissive_intensity == 1.0`, `has_emissive_map == 0`.

### Test 2: Emissive color is written to ModelUniform

- **Setup:** Spawn with `emissive_color=(1.0, 0.5, 0.0)`, `emissive_intensity=3.0`.
- **Expected:** `ModelUniform.emissive_color == [1.0, 0.5, 0.0, 3.0]`.

### Test 3: Emissive map flag set correctly

- **Setup:** Spawn with `emissive_map="e.png"`.
- **Expected:** `has_emissive_map == 1`.

### Test 4: Black fallback texture has correct pixel value

- **Setup:** Headless startup.
- **Expected:** Fallback emissive texture is 1×1, all pixels `(0, 0, 0, 255)`.

### Test 5: Emissive is additive — lit_color is not reduced

- **Setup:** Headless render with a white mesh at `(0,0,0)` under full lighting. Compare final pixel with emissive off vs emissive color `(0.2, 0.2, 0.2)`.
- **Expected:** Emissive version is brighter; lit_color contribution identical in both.

### Test 6: Emissive not affected by shadow

- **Setup:** Mesh fully in shadow (shadow_factor ≈ 0) with `emissive_color=(1,0,0)`.
- **Expected:** Fragment color ≥ `(1,0,0)` — emissive bypasses shadow attenuation.

### Test 7: Emissive intensity = 0 produces no glow

- **Setup:** Set `emissive_color=(1,1,1)`, `emissive_intensity=0.0`.
- **Expected:** `emissive_color.w == 0`; added emissive contribution is zero.

### Test 8: Missing emissive map falls back gracefully

- **Setup:** `emissive_map_id = Some("missing_e.png")`.
- **Action:** Render frame.
- **Expected:** Black fallback used, warning logged, no panic.

### Test 9: Emissive survives scene round-trip serialization

- **Setup:** Spawn entity with emissive map, color, and intensity. Serialize → deserialize.
- **Expected:** All three fields preserved exactly.

### Test 10: Emissive intensity clamp behavior

- **Setup:** Set `emissive_intensity = -1.0`.
- **Expected:** Engine clamps to `0.0` (no negative emission); warning logged.

---

## Gauntlet of Cubes Demo

**Where:** `game/scripts/levels/arena_3.py` (lava pit) and `game/scripts/level_builder.py` (pickups).

**Effect 1 — Lava pit glow:** Arena 3's lava pit is currently just a red box sitting on the floor. Emissive makes it self-illuminate — it looks like real molten rock even in the shadow of surrounding walls, and the glow doesn't flicker with the directional light angle.

**Effect 2 — Pickups glow:** Green pickup boxes gain a constant soft glow, making them easy to spot from across the arena without relying on lighting position.

**Effect 3 — Damage flash:** When the player is hit, briefly spike the player cube's emissive intensity to `3.0` for 0.15 seconds, then lerp back to `0.0`. No shader changes needed — just update the component.

**Example — lava pit in `game/scripts/levels/arena_3.py`:**

```python
# Lava pit — self-illuminating red/orange
lava = rython.scene.spawn(
    transform=rython.Transform(0, 0.05, 0, scale_x=6, scale_y=0.1, scale_z=6),
    mesh={
        "mesh_id":            "cube",
        "texture_id":         "game/assets/textures/Red/red_box.png",
        "emissive_color":     (1.0, 0.3, 0.0),   # orange-red glow
        "emissive_intensity": 1.8,
    },
    tags={"tags": ["lava"]},
)
_registered.append(lava)
```

**Example — pickup glow in `game/scripts/level_builder.py`:**

```python
mesh={
    "mesh_id":            "cube",
    "texture_id":         "game/assets/textures/Green/green_box.png",
    "emissive_color":     (0.0, 1.0, 0.2),
    "emissive_intensity": 0.4,   # subtle ambient glow
},
```

**Example — damage flash in `game/scripts/player.py`:**

```python
_flash_timer = 0.0

def update(dt):
    global _flash_timer
    if _flash_timer > 0:
        _flash_timer -= dt
        intensity = max(0.0, _flash_timer / 0.15) * 3.0
        # update player MeshComponent emissive_intensity here
    # ... existing movement code ...

def _on_hit():
    global _flash_timer
    _flash_timer = 0.15
```

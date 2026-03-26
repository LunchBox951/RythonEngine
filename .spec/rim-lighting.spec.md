# Rim Lighting

**Status:** Pending
**Priority:** Medium-Impact, Lower Effort
**SPEC.md entry:** §10

---

## Overview

Rim lighting (also called back lighting or Fresnel highlight) brightens the silhouette of an object against its background, giving a visual "pop" that separates characters and objects from their environment. The effect is based on the Fresnel term: surfaces whose normal is nearly perpendicular to the view direction receive a highlight.

This is a purely shader-side addition — no new textures or GPU buffers are required beyond a small per-material uniform extension.

---

## Rust Implementation

### Modified Types

**`crates/rython-ecs/src/component.rs` — `MeshComponent`**

```rust
pub struct MeshComponent {
    // ... existing fields ...
    pub rim_color:    [f32; 3],   // NEW — linear RGB; default [1,1,1]
    pub rim_strength: f32,        // NEW — [0,1]; default 0.0 (disabled)
    pub rim_power:    f32,        // NEW — Fresnel exponent; default 3.0 (narrower = higher)
}
```

`rim_strength == 0.0` is the zero-cost disable path (the shader multiplies by `rim_strength`, so no branch needed).

### Model Uniform Extension

**`crates/rython-renderer/src/shaders.rs`**

```wgsl
struct ModelUniform {
    // ... existing fields ...
    rim_color:    vec4<f32>,   // NEW — xyz = color, w = strength
    rim_power:    f32,         // NEW
    _pad_rim:     vec3<f32>,
};
```

`rim_color.w` stores `rim_strength` to pack both into one vec4.

### Shader Changes

**`crates/rython-renderer/src/shaders.rs` — `MESH_WGSL`**

Fresnel rim term added in `fs_main` after diffuse/specular:

```wgsl
// Rim lighting — Schlick Fresnel approximation
fn fresnel_schlick(cos_theta: f32, strength: f32, power: f32) -> f32 {
    return strength * pow(clamp(1.0 - cos_theta, 0.0, 1.0), power);
}

// In fs_main:
let n_dot_v    = max(dot(N, view_dir), 0.0);
let rim_factor = fresnel_schlick(n_dot_v, model.rim_color.w, model.rim_power);
let rim_contrib = model.rim_color.xyz * rim_factor;

let final_color = lit_color + emissive + rim_contrib;
```

The rim term is additive, placed after shadow and lighting but before fog. If `rim_strength == 0`, `rim_factor == 0` and there is no performance impact.

### Key Files to Modify

| File | Change |
|------|--------|
| `crates/rython-ecs/src/component.rs` | `rim_color`, `rim_strength`, `rim_power` on `MeshComponent` |
| `crates/rython-renderer/src/shaders.rs` | Fresnel function + rim additive term in `MESH_WGSL` |
| `crates/rython-renderer/src/lib.rs` | Populate `ModelUniform` rim fields |

---

## Python API

### Scene Spawn Changes

```python
entity = rython.scene.spawn(
    transform=rython.Transform(0, 0, 0),
    mesh={
        "mesh_id":      "models/character.glb",
        "texture_id":   "textures/char_diffuse.png",
        "rim_color":    (0.3, 0.5, 1.0),   # NEW — cool blue rim
        "rim_strength": 0.8,               # NEW
        "rim_power":    4.0,               # NEW — narrow rim
    },
)
```

### Dict Schema Addition

```python
{
    "rim_color":    tuple[float,float,float],  # optional, default (1.0, 1.0, 1.0)
    "rim_strength": float,                      # optional, default 0.0
    "rim_power":    float,                      # optional, default 3.0
}
```

---

## Test Cases

### Test 1: Default rim is disabled

- **Setup:** Spawn mesh without rim keys.
- **Expected:** `ModelUniform.rim_color.w == 0.0` (strength=0.0); `rim_power == 3.0`.

### Test 2: Rim strength 0 produces zero rim contribution

- **Setup:** `rim_strength=0.0`, `rim_color=(1,1,1)`.
- **Expected:** `rim_factor == 0.0` for all `n_dot_v` values.

### Test 3: Rim factor is maximum at grazing angle

- **Setup:** `rim_strength=1.0`, `rim_power=1.0`, `n_dot_v=0.0` (90° grazing).
- **Expected:** `rim_factor == 1.0`.

### Test 4: Rim factor is zero at direct view

- **Setup:** `rim_strength=1.0`, `rim_power=1.0`, `n_dot_v=1.0` (face-on).
- **Expected:** `rim_factor == 0.0`.

### Test 5: Higher rim power narrows rim band

- **Setup:** `n_dot_v=0.5`. Compute with `rim_power=1.0` vs `rim_power=8.0`.
- **Expected:** `result_p8 < result_p1` (higher power → narrower rim at mid-angle).

### Test 6: `rim_color` tint is applied correctly

- **Setup:** `rim_color=(1,0,0)`, `rim_strength=1.0`, `n_dot_v=0.0`.
- **Expected:** Rim contribution is `(1,0,0)` red only, no blue or green.

### Test 7: `ModelUniform` rim fields are populated

- **Setup:** Spawn with `rim_color=(0.5, 0.5, 1.0)`, `rim_strength=0.6`, `rim_power=4.0`.
- **Expected:** `rim_color.xyz == [0.5, 0.5, 1.0]`, `rim_color.w == 0.6`, `rim_power == 4.0`.

### Test 8: `rim_strength` clamp behavior

- **Setup:** Set `rim_strength = 2.0` (above 1.0).
- **Expected:** Engine clamps to `1.0`; warning logged.

### Test 9: `rim_power` minimum is 0.1 (prevent division artifacts)

- **Setup:** Set `rim_power = 0.0`.
- **Expected:** Clamped to `0.1`; no NaN or infinity in shader output.

### Test 10: Rim is additive — does not reduce diffuse

- **Setup:** Identical meshes, one with rim enabled, one without. Compare center-pixel diffuse contribution.
- **Expected:** Diffuse value is identical; rim-enabled mesh is only brighter, never darker.

---

## Gauntlet of Cubes Demo

**Where:** `game/scripts/level_builder.py` — `spawn_enemy()`, `spawn_pickup()`, and `game/scripts/player.py` — `spawn()`.

**Effect:** Three targeted uses, each serving gameplay legibility:

1. **Player cube** — White rim at `strength=0.4` silhouettes the player against any background, especially the dark platforms and walls of Arenas 2 and 3. The player never visually disappears into same-tone geometry.

2. **Pickup boxes** — Bright green rim (`strength=0.6`, `power=2.5`) makes pickups visible at a glance even when they sit on a similar-coloured surface. Combined with emissive (§4) this is a very readable collectable.

3. **Enemies in CHASE state** — In `game/scripts/npc/skeleton.py`, when the state transitions from `PATROL` to `CHASE`, update the enemy's rim to bright orange (`strength=0.7`) as a visual alert. Reverts to zero on return to patrol. This is communicating AI state through material without any HUD changes.

**Example — player spawn in `game/scripts/player.py`:**

```python
def spawn(x, y, z):
    global _entity, _spawn_pos
    _spawn_pos = (x, y, z)
    _entity = rython.scene.spawn(
        transform=rython.Transform(x, y, z, scale_x=0.8, scale_y=1.8, scale_z=0.8),
        mesh={
            "mesh_id":      "cube",
            "texture_id":   "game/assets/textures/Light/light_box_alt1.png",
            "rim_color":    (1.0, 1.0, 1.0),
            "rim_strength": 0.4,
            "rim_power":    3.5,
        },
        rigid_body={"body_type": "dynamic", "mass": 1.0, "gravity_factor": 1.0},
        collider={"shape": "box", "size": [0.8, 1.8, 0.8]},
    )
```

**Example — enemy chase-state rim in `game/scripts/npc/skeleton.py`:**

```python
def _enter_chase(state):
    state["mode"] = "CHASE"
    # Visual alert: enemy lights up orange when it spots the player
    mesh = state["entity"].get_component("MeshComponent")
    mesh.rim_color    = (1.0, 0.45, 0.0)
    mesh.rim_strength = 0.7
    mesh.rim_power    = 3.0

def _enter_patrol(state):
    state["mode"] = "PATROL"
    mesh = state["entity"].get_component("MeshComponent")
    mesh.rim_strength = 0.0   # off during patrol
```

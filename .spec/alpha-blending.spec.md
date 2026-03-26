# Alpha Blending / Transparency

**Status:** Pending
**Priority:** Medium-Impact, Lower Effort
**SPEC.md entry:** §15

---

## Overview

Proper depth-sorted rendering of transparent objects. Currently transparency is not well supported. This spec adds:

- **Alpha cutout (Discard)** — Fragment discarded when `alpha < threshold`. No sorting required.
- **Alpha blend** — Classic `src_alpha * src + (1 - src_alpha) * dst`. Requires back-to-front depth sorting.
- **Premultiplied alpha** — For UI and particles.

Transparent objects are rendered in a second sub-pass after all opaque geometry.

---

## Rust Implementation

### Modified Types

**`crates/rython-ecs/src/component.rs` — `MeshComponent`**

```rust
#[derive(Clone, Debug, PartialEq)]
pub enum AlphaMode {
    Opaque,             // No transparency (default)
    Cutout { threshold: f32 },  // Discard if alpha < threshold
    Blend,              // Alpha blend; depth-sorted
    Premultiplied,      // Premultiplied alpha blend
}

pub struct MeshComponent {
    // ... existing fields ...
    pub alpha_mode:   AlphaMode,    // NEW; default Opaque
    pub opacity:      f32,          // NEW — scalar multiplier [0,1]; default 1.0
}
```

`opacity` multiplies the texture alpha and/or `color.a` before the alpha mode is applied.

### Render Passes

**`crates/rython-renderer/src/lib.rs`** — `render_meshes()` splits into two sub-passes:

1. **Opaque pass** — All entities with `AlphaMode::Opaque` or `Cutout`. Depth write ON. Sorted by mesh batch key for instancing.
2. **Transparent pass** — All entities with `AlphaMode::Blend` or `Premultiplied`. Depth write OFF. Sorted back-to-front by distance to camera.

### Pipeline Variants

**`crates/rython-renderer/src/gpu.rs`** — Three mesh pipeline variants:

```rust
pub mesh_opaque_pipeline:       wgpu::RenderPipeline,  // existing
pub mesh_cutout_pipeline:       wgpu::RenderPipeline,  // discard in shader
pub mesh_blend_pipeline:        wgpu::RenderPipeline,  // blend state: SrcAlpha/OneMinusSrcAlpha, depth write OFF
pub mesh_premul_pipeline:       wgpu::RenderPipeline,  // blend state: One/OneMinusSrcAlpha, depth write OFF
```

### Shader Changes

**`crates/rython-renderer/src/shaders.rs` — `MESH_WGSL`**

New flags in `ModelUniform`:

```wgsl
struct ModelUniform {
    // ... existing ...
    alpha_mode: u32,     // NEW — 0=opaque, 1=cutout, 2=blend, 3=premul
    cutout_threshold: f32,  // NEW — alpha discard threshold for cutout mode
    opacity:          f32,  // NEW
    _pad_alpha:       f32,
};
```

Fragment shader alpha handling:

```wgsl
// In fs_main:
var alpha = base_alpha * model.opacity;

if (model.alpha_mode == 1u) {
    // Cutout
    if (alpha < model.cutout_threshold) { discard; }
    alpha = 1.0;
}

return vec4(final_color, alpha);
```

For blend modes the pipeline's `ColorTargetState.blend` handles the GPU blending equation; the shader just outputs the alpha.

### Depth Sorting

**`crates/rython-renderer/src/lib.rs`**

```rust
/// Sort transparent entities by distance to camera (farthest first).
fn sort_transparent(
    entities: &mut Vec<(EntityId, DrawMesh)>,
    camera_pos: Vec3,
    world_transforms: &HashMap<EntityId, WorldTransform>,
);
```

Uses entity world-space position for the distance metric. For large meshes a bounding sphere center would be preferred but is deferred.

### Key Files to Modify

| File | Change |
|------|--------|
| `crates/rython-ecs/src/component.rs` | `AlphaMode` enum, `opacity` field on `MeshComponent` |
| `crates/rython-renderer/src/gpu.rs` | 3 additional pipeline variants with blend states |
| `crates/rython-renderer/src/shaders.rs` | `alpha_mode`, `cutout_threshold`, `opacity` in `ModelUniform`; discard in shader |
| `crates/rython-renderer/src/lib.rs` | Two-sub-pass render; depth sorting for transparent |

---

## Python API

### Scene Spawn Changes

```python
# Alpha cutout (e.g. foliage)
leaf = rython.scene.spawn(
    transform=rython.Transform(0, 1, 0),
    mesh={
        "mesh_id":    "models/leaf.glb",
        "texture_id": "textures/leaf.png",
        "alpha_mode": "cutout",     # NEW
        "cutout_threshold": 0.5,    # NEW
    },
)

# Alpha blend (e.g. glass)
glass = rython.scene.spawn(
    transform=rython.Transform(2, 0, 0),
    mesh={
        "mesh_id":    "models/window.glb",
        "texture_id": "textures/glass.png",
        "alpha_mode": "blend",      # NEW
        "opacity":    0.4,           # NEW
    },
)
```

### Dict Schema Addition

```python
{
    "alpha_mode":         "opaque" | "cutout" | "blend" | "premultiplied",  # default "opaque"
    "cutout_threshold":   float,    # 0.0–1.0; only for cutout mode; default 0.5
    "opacity":            float,    # 0.0–1.0 multiplier; default 1.0
}
```

---

## Test Cases

### Test 1: Default alpha mode is Opaque

- **Setup:** Spawn mesh without `alpha_mode`.
- **Expected:** `MeshComponent.alpha_mode == AlphaMode::Opaque`.

### Test 2: Opaque entity uses opaque pipeline

- **Setup:** Entity with `AlphaMode::Opaque`.
- **Expected:** Rendered with `mesh_opaque_pipeline`; depth write ON.

### Test 3: Blend entity uses blend pipeline

- **Setup:** Entity with `AlphaMode::Blend`.
- **Expected:** Rendered with `mesh_blend_pipeline`; depth write OFF.

### Test 4: Cutout entity discards at threshold

- **Setup:** `alpha_mode=Cutout{threshold:0.5}`. Texture pixel with `alpha=0.3`.
- **Expected:** Fragment discarded (not rendered).

### Test 5: Cutout entity keeps pixel above threshold

- **Setup:** Same cutout entity. Texture pixel with `alpha=0.7`.
- **Expected:** Fragment rendered with `alpha=1.0`.

### Test 6: Transparent entities sorted back-to-front

- **Setup:** Two blend entities at `z=5` and `z=10` from camera. Collect draw order.
- **Expected:** Entity at `z=10` drawn first (farthest first).

### Test 7: Opaque entities rendered before transparent

- **Setup:** Mix of opaque and blend entities.
- **Expected:** All opaque draw calls come before all blend draw calls in the command buffer.

### Test 8: `opacity=0.0` makes entity invisible

- **Setup:** `opacity=0.0`, `alpha_mode="blend"`.
- **Expected:** All fragment alphas = 0.0; entity effectively invisible.

### Test 9: `opacity` multiplies texture alpha

- **Setup:** Texture with `alpha=0.8`, `opacity=0.5`.
- **Expected:** Final alpha before blend = `0.8 * 0.5 = 0.4`.

### Test 10: Invalid `alpha_mode` string

- **Setup:** Python `"alpha_mode": "glitter"`.
- **Expected:** Error or warning logged; defaults to `"opaque"`.

### Test 11: Depth write disabled for blend entities

- **Setup:** Inspect pipeline descriptor for `mesh_blend_pipeline`.
- **Expected:** `DepthStencilState.depth_write_enabled == false`.

### Test 12: Premultiplied blend state uses `One / OneMinusSrcAlpha`

- **Setup:** Inspect `mesh_premul_pipeline` color target blend state.
- **Expected:** `src_factor = One`, `dst_factor = OneMinusSrcAlpha`.

---

## Gauntlet of Cubes Demo

**Where:** `game/scripts/level_builder.py` — `spawn_pickup()` (pickup vanish), and `game/scripts/levels/arena_3.py` — bonus ghost enemy.

**Effect 1 — Pickup collection dissolve:** When the player collects a pickup (tag: `"pickup"`), instead of immediately despawning the cube, briefly animate its `opacity` from `1.0` down to `0.0` over 0.25 seconds, then despawn. The cube dissolves on collection — a clear, satisfying visual reward.

**Effect 2 — Ghost enemy in Arena 3:** Introduce a "ghost" variant of the skeleton as a wave-2 bonus: a semi-transparent `opacity=0.45` purple cube. The player can see through it slightly, making it visually alien compared to solid enemies. It uses `alpha_mode="blend"`.

**Effect 3 — Lava damage overlay:** The existing lava hazard code in `arena_3.py` applies 5 HP/second. Add a fullscreen semi-transparent red quad (a `DrawRect` with `alpha=0.3`) that fades in while the player is standing in the lava zone — a heat haze effect without touching the 3D pipeline.

**Example — pickup dissolve in `game/scripts/level_builder.py`:**

```python
_dissolving = []   # list of (entity, remaining_time, start_time)

def start_pickup_dissolve(entity):
    _dissolving.append((entity, 0.25))

def update_dissolves(dt):
    still_alive = []
    for (entity, t) in _dissolving:
        t -= dt
        if t <= 0:
            entity.despawn()
        else:
            mesh = entity.get_component("MeshComponent")
            mesh.opacity = t / 0.25
            still_alive.append((entity, t))
    _dissolving[:] = still_alive
```

**Example — ghost enemy in `game/scripts/levels/arena_3.py` wave 2:**

```python
ghost = rython.scene.spawn(
    transform=rython.Transform(0, 1, -8, scale_x=1.0, scale_y=2.0, scale_z=1.0),
    mesh={
        "mesh_id":    "cube",
        "texture_id": "game/assets/textures/Purple/purple_box.png",
        "alpha_mode": "blend",
        "opacity":    0.45,
    },
    rigid_body={"body_type": "dynamic", "mass": 1.5},
    collider={"shape": "box", "size": [1.0, 2.0, 1.0]},
    tags={"tags": ["enemy"]},
)
_registered.append(ghost)
enemies.register(ghost, "skeleton", is_boss=False)
```

# Specular Mapping

**Status:** Pending
**Priority:** High-Impact, Moderate Effort
**SPEC.md entry:** §2

---

## Overview

Split the mesh material into a diffuse map (already present) and a specular map. Currently `MeshComponent.shininess` is a hardcoded uniform float that is never sampled in the shader. This spec replaces the single-shininess path with a per-texel specular intensity and gloss sampled from a specular texture, while preserving the scalar fallback for assets without one.

---

## Rust Implementation

### Modified Types

**`crates/rython-ecs/src/component.rs` — `MeshComponent`**

```rust
pub struct MeshComponent {
    pub mesh_id:         String,
    pub texture_id:      String,
    pub normal_map_id:   Option<String>,  // from normal-mapping spec
    pub specular_map_id: Option<String>,  // NEW — asset key for specular map (RGB/R channel)
    pub shininess:       f32,             // scalar fallback; ignored when specular_map_id is Some
    pub specular_color:  [f32; 3],        // NEW — tint applied to specular highlight; default [1,1,1]
    pub yaw_offset:      f32,
    pub visible:         bool,
}
```

The `specular_map_id` texture encodes:
- **R channel:** specular intensity (0 = matte, 1 = fully specular)
- **G channel:** glossiness / shininess exponent remapped from [0,1] → [1,128] via `exp2(g * 7.0)`

If `specular_map_id` is `None`, the shader uses the scalar `shininess` value with intensity `1.0`.

### New GPU Binding

**`crates/rython-renderer/src/gpu.rs` — `BindGroupLayouts`**

Add specular map layout at group 4 (to coexist with normal map at group 3):

```rust
pub mesh_specular_map: wgpu::BindGroupLayout,  // @group(4) binding(0,1)
```

Create a 1×1 fallback specular texture: `R: 255, G: 128` (full intensity, mid-gloss) for entities without a specular map.

### Model Uniform Changes

**`crates/rython-renderer/src/shaders.rs`**

```wgsl
struct ModelUniform {
    model:             mat4x4<f32>,
    color:             vec4<f32>,
    specular_color:    vec4<f32>,    // NEW — .xyz = tint, .w unused
    has_texture:       u32,
    has_normal_map:    u32,
    has_specular_map:  u32,          // NEW
    shininess:         f32,          // scalar fallback
};
```

### Shader Changes

**`crates/rython-renderer/src/shaders.rs` — `MESH_WGSL`**

Phong specular term:

```wgsl
// NEW bind groups:
@group(4) @binding(0) var t_specular: texture_2d<f32>;
@group(4) @binding(1) var s_specular: sampler;

// In fragment shader:
fn compute_specular(
    view_dir: vec3<f32>,
    N: vec3<f32>,
    light_dir: vec3<f32>,
    uv: vec2<f32>,
) -> vec3<f32> {
    var spec_intensity: f32 = 1.0;
    var spec_power:     f32 = model.shininess;

    if (model.has_specular_map != 0u) {
        let spec_sample  = textureSample(t_specular, s_specular, uv).rg;
        spec_intensity   = spec_sample.r;
        spec_power       = exp2(spec_sample.g * 7.0);  // [1, 128]
    }

    let reflect_dir  = reflect(-light_dir, N);
    let spec_factor  = pow(max(dot(view_dir, reflect_dir), 0.0), spec_power);
    return model.specular_color.xyz * spec_intensity * spec_factor;
}
```

Camera position is passed in `CameraUniform` (already contains `view_proj`; add `eye_position: vec3<f32>`).

### CameraUniform Extension

**`crates/rython-renderer/src/camera.rs`**

```rust
#[repr(C)]
#[derive(bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub view_proj:    [[f32; 4]; 4],
    pub eye_position: [f32; 3],  // NEW — needed for specular half-vector
    pub _pad:         f32,
}
```

Update `GpuContext::upload_camera()` to populate `eye_position` from `Camera::position`.

### Key Files to Modify

| File | Change |
|------|--------|
| `crates/rython-ecs/src/component.rs` | Add `specular_map_id`, `specular_color` |
| `crates/rython-renderer/src/shaders.rs` | Phong specular in `MESH_WGSL`, new bind groups |
| `crates/rython-renderer/src/gpu.rs` | `mesh_specular_map` layout, fallback texture, `CameraUniform.eye_position` |
| `crates/rython-renderer/src/camera.rs` | Extend `CameraUniform` |
| `crates/rython-renderer/src/lib.rs` | Bind specular texture per entity; write `eye_position` |

---

## Python API

### Scene Spawn Changes

```python
entity = rython.scene.spawn(
    transform=rython.Transform(0, 0, 0),
    mesh={
        "mesh_id":       "models/sword.glb",
        "texture_id":    "textures/sword_diffuse.png",
        "specular_map":  "textures/sword_specular.png",  # NEW — optional
        "specular_color": (1.0, 0.9, 0.7),               # NEW — warm tint, optional
        "shininess":     64.0,                            # scalar fallback
    },
)
```

### Dict Schema Addition

```python
{
    "specular_map":   str | None,          # optional, default None
    "specular_color": tuple[float,float,float],  # optional, default (1.0, 1.0, 1.0)
    "shininess":      float,               # optional, default 32.0
}
```

---

## Test Cases

### Test 1: Default specular is neutral (no map, shininess=32)

- **Setup:** Spawn mesh with no `specular_map`.
- **Action:** Inspect `ModelUniform` on GPU.
- **Expected:** `has_specular_map == 0`, `shininess == 32.0`, `specular_color == [1,1,1,0]`.

### Test 2: Specular map flag set correctly

- **Setup:** Spawn mesh with `specular_map="s.png"`.
- **Expected:** `has_specular_map == 1`.

### Test 3: Fallback specular texture is correct pixel value

- **Setup:** Headless GPU context startup.
- **Expected:** 1×1 fallback specular texture pixel is `R=255, G=128` (full intensity, mid-gloss).

### Test 4: `specular_color` default is white

- **Setup:** Spawn mesh without `specular_color` key.
- **Expected:** `ModelUniform.specular_color == [1.0, 1.0, 1.0, 0.0]`.

### Test 5: `specular_color` tint is written correctly

- **Setup:** Spawn with `specular_color=(0.5, 0.5, 1.0)`.
- **Expected:** `ModelUniform.specular_color.xyz == [0.5, 0.5, 1.0]`.

### Test 6: `shininess` scalar is written when no specular map

- **Setup:** Spawn with `shininess=128.0` and no specular map.
- **Expected:** `ModelUniform.shininess == 128.0`, `has_specular_map == 0`.

### Test 7: `shininess` ignored when specular map present

- **Setup:** Spawn with `shininess=128.0` AND `specular_map="s.png"`.
- **Action:** Check shader dispatch — `spec_power` comes from map.
- **Expected:** `has_specular_map == 1`; `ModelUniform.shininess` value is irrelevant (shader branch ignores it).

### Test 8: Camera `eye_position` is populated

- **Setup:** Set `camera.set_position(10, 5, 0)`.
- **Action:** Upload camera uniform.
- **Expected:** `CameraUniform.eye_position == [10.0, 5.0, 0.0]`.

### Test 9: Missing specular map asset falls back gracefully

- **Setup:** `specular_map_id = Some("nonexistent_spec.png")`.
- **Action:** Run frame.
- **Expected:** Fallback texture used, warning logged, no panic.

### Test 10: Specular component survives scene serialisation round-trip

- **Setup:** Spawn entity with specular map and color. Serialize scene to JSON. Deserialize.
- **Expected:** `specular_map_id` and `specular_color` values match originals.

---

## Gauntlet of Cubes Demo

**Where:** `game/scripts/level_builder.py` — `spawn_pickup()` and `spawn_enemy()`.

**Effect:** Green pickup boxes are the main collectable in the game. Making them visually distinct is important for gameplay. A bright specular highlight (white specular map, low roughness) makes the green cubes catch the light and appear to gleam, drawing the player's eye. The boss skeleton in Arena 3 gets a hard, bright specular to look menacing.

**Example — pickups in `game/scripts/level_builder.py`:**

```python
def spawn_pickup(x, y, z, pickup_type="score", value=100, tags=None):
    entity = rython.scene.spawn(
        transform=rython.Transform(x, y, z, scale_x=0.5, scale_y=0.5, scale_z=0.5),
        mesh={
            "mesh_id":       "cube",
            "texture_id":    "game/assets/textures/Green/green_box.png",
            "specular_map":  "game/assets/textures/Green/green_box_s.png",
            "specular_color": (0.9, 1.0, 0.9),   # slightly tinted green
            "shininess":     80.0,
        },
        tags={"tags": tags or ["pickup", pickup_type]},
    )
    _registered.append(entity)
    return entity
```

**Example — boss skeleton in `game/scripts/levels/arena_3.py`:**

```python
# Boss uses higher shininess and a warm specular tint to look more dangerous
boss = level_builder.spawn_enemy(8, 1, 0, "skeleton", is_boss=True)
# (Requires spawn_enemy to accept a specular_map kwarg, or set directly after spawn)
```

**Assets to add:** `green_box_s.png` (R=255 intensity, G=200 gloss for moderate shininess) and `purple_box_s.png` (R=180, G=220 for boss enemy highlight). Both can be tiny 16×16 constant-color images for the demo.

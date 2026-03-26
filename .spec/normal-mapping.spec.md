# Normal Mapping

**Status:** Pending
**Priority:** High-Impact, Moderate Effort
**SPEC.md entry:** §1

---

## Overview

Add tangent-space normal map support to the mesh pipeline. Per-fragment normals are read from a texture and transformed into world space via a TBN matrix, producing surface detail without extra geometry. Currently the mesh shader uses interpolated vertex normals only.

---

## Rust Implementation

### Modified Types

**`crates/rython-ecs/src/component.rs` — `MeshComponent`**

Add one field:

```rust
pub struct MeshComponent {
    pub mesh_id: String,
    pub texture_id: String,
    pub normal_map_id: Option<String>,   // NEW — asset key for normal map texture
    pub yaw_offset: f32,
    pub shininess: f32,
    pub visible: bool,
}
```

Default for `normal_map_id` is `None`, meaning flat normals fall through to vertex normals (backwards-compatible).

**`crates/rython-resources/src/lib.rs` — `Vertex`**

Extend vertex layout with tangent and bitangent:

```rust
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position:  [f32; 3],
    pub normal:    [f32; 3],
    pub uv:        [f32; 2],
    pub tangent:   [f32; 3],   // NEW — tangent vector (surface u-axis)
    pub bitangent: [f32; 3],   // NEW — bitangent vector (surface v-axis)
    pub _pad:      [f32; 2],   // NEW — align to 16-byte stride → 64 bytes total
}
```

Stride changes from 32 → 64 bytes. All existing vertex buffer bindings must be regenerated with the new `wgpu::VertexBufferLayout`.

**`crates/rython-renderer/src/gpu.rs` — `BindGroupLayouts`**

Add a new layout for the normal map texture:

```rust
pub struct BindGroupLayouts {
    // ... existing fields ...
    pub mesh_normal_map: wgpu::BindGroupLayout,   // NEW — @group(3) binding(0,1)
}
```

The `mesh_texture` layout stays at group 2 (diffuse); normal map is group 3.

### Tangent Generation

**New file: `crates/rython-resources/src/tangents.rs`**

```rust
/// Compute tangent and bitangent for every vertex in a mesh.
/// Uses Lengyel's method: accumulate per-triangle TB contributions,
/// then orthogonalize against the normal with Gram-Schmidt.
pub fn compute_tangents(vertices: &mut [Vertex], indices: &[u32]);
```

Called at mesh load time (both glTF importer and procedural generators).
If the glTF asset includes `TANGENT` attributes they are used directly; otherwise `compute_tangents` runs.

### Shader Changes

**`crates/rython-renderer/src/shaders.rs` — `MESH_WGSL`**

Replace the existing normal interpolation with TBN-space sampling:

```wgsl
// NEW bind groups:
@group(2) @binding(0) var t_diffuse:    texture_2d<f32>;
@group(2) @binding(1) var s_diffuse:    sampler;
@group(3) @binding(0) var t_normal_map: texture_2d<f32>;
@group(3) @binding(1) var s_normal_map: sampler;

// In model uniform (group 1):
struct ModelUniform {
    model:          mat4x4<f32>,
    color:          vec4<f32>,
    has_texture:    u32,
    has_normal_map: u32,   // NEW
    _pad:           vec2<u32>,
};

// Vertex output — add tangent/bitangent world vectors:
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal:    vec3<f32>,
    @location(1) world_tangent:   vec3<f32>,   // NEW
    @location(2) world_bitangent: vec3<f32>,   // NEW
    @location(3) uv:              vec2<f32>,
};

// Fragment shader TBN usage:
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var N: vec3<f32>;
    if (model.has_normal_map != 0u) {
        let tbn_normal = textureSample(t_normal_map, s_normal_map, in.uv).rgb;
        let tangent_normal = tbn_normal * 2.0 - 1.0;
        let TBN = mat3x3(
            normalize(in.world_tangent),
            normalize(in.world_bitangent),
            normalize(in.world_normal),
        );
        N = normalize(TBN * tangent_normal);
    } else {
        N = normalize(in.world_normal);
    }
    // ... existing diffuse calc using N ...
}
```

### Render Pipeline Changes

**`crates/rython-renderer/src/gpu.rs` — `create_mesh_pipeline()`**

- Update `VertexBufferLayout` stride to 64 bytes with attributes for tangent and bitangent.
- Add `mesh_normal_map` bind group layout at index 3 in the pipeline layout.
- Create a 1×1 flat-normal fallback texture (`RGB: 127, 127, 255`) uploaded once at startup for entities with `normal_map_id = None`.

**`crates/rython-renderer/src/lib.rs` — `render_meshes()`**

When building per-entity bind groups:
1. Resolve `normal_map_id` from `AssetStore` → `wgpu::Texture` (or use flat fallback).
2. Set `has_normal_map` in `ModelUniform` accordingly.
3. Set bind group 3 with the resolved texture.

### Key Files to Modify

| File | Change |
|------|--------|
| `crates/rython-ecs/src/component.rs` | Add `normal_map_id: Option<String>` to `MeshComponent` |
| `crates/rython-resources/src/lib.rs` | Extend `Vertex` with `tangent`, `bitangent`, `_pad` |
| `crates/rython-resources/src/tangents.rs` | New file — `compute_tangents()` |
| `crates/rython-resources/src/loaders/gltf.rs` | Call `compute_tangents` if no TANGENT attrib |
| `crates/rython-renderer/src/shaders.rs` | TBN shader code in `MESH_WGSL` |
| `crates/rython-renderer/src/gpu.rs` | New bind group layout, updated vertex layout, fallback texture |
| `crates/rython-renderer/src/lib.rs` | Resolve normal map per-entity, set `has_normal_map` |

---

## Python API

### Scene Spawn Changes

The `mesh` kwarg dict accepts a new `normal_map` key:

```python
entity = rython.scene.spawn(
    transform=rython.Transform(0, 0, 0),
    mesh={
        "mesh_id":    "models/rock.glb",
        "texture_id": "textures/rock_diffuse.png",
        "normal_map": "textures/rock_normal.png",  # NEW — optional
    },
)
```

Passing `normal_map=None` or omitting it falls back to vertex normals.

### MeshComponent Dict Schema

```python
{
    "mesh_id":    str,           # required
    "texture_id": str,           # required
    "normal_map": str | None,    # optional, default None
    "shininess":  float,         # optional, default 32.0
    "visible":    bool,          # optional, default True
}
```

### Python Stub Updates

**`rython/_scene.py`** — `spawn()` docstring updated with `normal_map` kwarg.
**`rython/_components.py`** (if it exists) — `MeshComponent` stub adds `normal_map: str | None = None`.

---

## Test Cases

### Test 1: Normal map field round-trips through ECS

- **Setup:** Spawn entity with `normal_map="textures/bricks_n.png"`.
- **Action:** Read back `MeshComponent` from scene.
- **Expected:** `component.normal_map_id == Some("textures/bricks_n.png")`.
- **Edge case:** Verify spawn without `normal_map` sets `normal_map_id = None`.

### Test 2: Vertex tangent generation for a unit cube

- **Setup:** Generate `generate_cube()` and run `compute_tangents()`.
- **Action:** Inspect tangent vectors on all 24 vertices.
- **Expected:** For each face, `dot(tangent, normal) < 1e-5` (orthogonal), `tangent` is unit length, `cross(tangent, bitangent)` aligns with `normal` (right-handed).

### Test 3: Tangent generation is idempotent

- **Setup:** Load a glTF mesh that already has TANGENT attributes.
- **Action:** Run `compute_tangents()` again on the same data.
- **Expected:** Resulting tangent vectors differ from original by less than 1e-4 (numerically stable).

### Test 4: Flat-normal fallback texture is neutral

- **Setup:** GpuContext in headless mode.
- **Action:** Sample the auto-created fallback normal texture.
- **Expected:** All pixels are `(127, 127, 255)` (encodes `(0, 0, 1)` tangent-space up vector).

### Test 5: ModelUniform `has_normal_map` flag is 0 without a map

- **Setup:** Spawn `MeshComponent { normal_map_id: None, ... }`.
- **Action:** Record the `ModelUniform` written to the GPU buffer.
- **Expected:** `has_normal_map == 0`.

### Test 6: ModelUniform `has_normal_map` flag is 1 with a map

- **Setup:** Spawn `MeshComponent { normal_map_id: Some("n.png"), ... }`.
- **Action:** Record the `ModelUniform` written.
- **Expected:** `has_normal_map == 1`.

### Test 7: Missing normal map asset falls back to flat texture, no panic

- **Setup:** Set `normal_map_id = Some("nonexistent.png")`.
- **Action:** Run a full render frame.
- **Expected:** No panic; flat fallback texture is used; a warning is logged.

### Test 8: Vertex buffer stride is exactly 64 bytes

- **Setup:** Inspect `wgpu::VertexBufferLayout` created for the mesh pipeline.
- **Expected:** `array_stride == 64`.

### Test 9: Python `normal_map=None` equivalent to omitting the field

- **Setup:** Spawn entity A with `mesh={"mesh_id": "m", "texture_id": "t", "normal_map": None}` and entity B without `normal_map` key.
- **Expected:** Both produce `MeshComponent` with `normal_map_id == None`.

### Test 10: Normal map does not affect 2D/UI draw commands

- **Setup:** Queue `DrawRect`, `DrawText`, `DrawImage` commands alongside a mesh with a normal map.
- **Action:** Execute a frame.
- **Expected:** 2D commands complete successfully; no bind group conflict.

---

## Gauntlet of Cubes Demo

**Where:** `game/scripts/level_builder.py` — `spawn_static_block()`, and all three arena files.

**Effect:** The arena floors (light, orange, dark textures) and perimeter walls are flat-shaded cubes today. Adding normal maps makes them read as carved stone or scuffed metal rather than coloured candy boxes. The difference is especially noticeable in Arena 3's dark floor, where the directional light rakes across the surface.

**Example — arena floors and walls in `game/scripts/levels/arena_1.py`:**

```python
# Before — flat diffuse only
level_builder.spawn_static_block(
    0, -0.5, 0, 20, 0.5, 20,
    texture="game/assets/textures/Light/light_floor_grid.png",
)

# After — add normal map to every floor/wall block
level_builder.spawn_static_block(
    0, -0.5, 0, 20, 0.5, 20,
    texture="game/assets/textures/Light/light_floor_grid.png",
    normal_map="game/assets/textures/Light/light_floor_grid_n.png",
)
```

**Example — enemy skeletons in `game/scripts/level_builder.py`:**

```python
# Enemies already share mesh_id="cube"; giving them a dented-metal normal map
# makes the purple boxes look battle-scarred without extra geometry.
rython.scene.spawn(
    transform=rython.Transform(x, y, z, scale_x=1.0, scale_y=2.0, scale_z=1.0),
    mesh={
        "mesh_id":    "cube",
        "texture_id": "game/assets/textures/Purple/purple_box.png",
        "normal_map": "game/assets/textures/Purple/purple_box_n.png",
    },
    tags={"tags": ["enemy"]},
)
```

**Assets to add:** `*_n.png` normal maps alongside each existing diffuse texture (five texture sets: Light, Orange, Dark, Red, Purple). A single 256×256 stone-tile normal map reused across all floor types demonstrates the feature with minimal new art.

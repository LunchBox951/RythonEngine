# Instanced Rendering

**Status:** Pending
**Priority:** Medium-Impact, Lower Effort
**SPEC.md entry:** §11

---

## Overview

Batch similar meshes with different transforms into a single draw call using `wgpu` instancing. Currently every `MeshComponent` entity issues an individual `DrawMesh` command → individual draw call. With instancing, entities sharing the same `(mesh_id, texture_id)` pair are grouped and rendered with one `draw_indexed(instance_count)`.

This spec does not introduce a new user-facing component. The batching is automatic inside `render_meshes()`.

---

## Rust Implementation

### Instance Data Layout

**`crates/rython-renderer/src/instance.rs`** (new file)

```rust
/// Per-instance data sent as a second vertex buffer (vertex step mode = Instance).
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceData {
    /// Column-major model matrix (4 × vec4)
    pub model_col0: [f32; 4],
    pub model_col1: [f32; 4],
    pub model_col2: [f32; 4],
    pub model_col3: [f32; 4],
    /// Object color (RGBA linear)
    pub color:      [f32; 4],
    /// Packed flags: bit0 = has_texture, bit1 = has_normal_map, etc.
    pub flags:      u32,
    pub _pad:       [f32; 3],
}
// Total: 96 bytes. Column-major mat4 = 4×16 = 64; color = 16; flags+pad = 16.
```

### Instancing Buffer Management

**`crates/rython-renderer/src/instance.rs`**

```rust
pub struct InstanceBuffer {
    /// Key: (mesh_id, texture_id) — defines a batch
    pub batches:        HashMap<InstanceBatchKey, Vec<InstanceData>>,
    /// Uploaded GPU buffer per batch (resized as needed)
    pub gpu_buffers:    HashMap<InstanceBatchKey, wgpu::Buffer>,
    pub max_instances:  usize,   // hard limit; default 4096
}

#[derive(Hash, Eq, PartialEq, Clone)]
pub struct InstanceBatchKey {
    pub mesh_id:         String,
    pub texture_id:      String,
    pub normal_map_id:   Option<String>,
    pub specular_map_id: Option<String>,
    pub emissive_map_id: Option<String>,
}

impl InstanceBuffer {
    pub fn clear(&mut self);

    /// Append one instance to its batch, creating the batch if needed.
    pub fn push(&mut self, key: InstanceBatchKey, data: InstanceData);

    /// Upload all batches to GPU; resize buffers if needed.
    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue);
}
```

### Render Pipeline Changes

**`crates/rython-renderer/src/gpu.rs`**

The mesh pipeline's `VertexState` gains a second `VertexBufferLayout`:

```rust
VertexBufferLayout {
    array_stride: 96,              // sizeof(InstanceData)
    step_mode: VertexStepMode::Instance,
    attributes: [
        // model mat cols at locations 5,6,7,8; color at 9; flags at 10
        // (locations 0-4 are per-vertex position,normal,uv,tangent,bitangent)
    ],
},
```

Remove the `@group(1) @binding(0) model` uniform — per-instance data now comes from the instance buffer directly.

### Shader Changes

**`crates/rython-renderer/src/shaders.rs` — `MESH_WGSL`**

```wgsl
// Per-instance vertex inputs (replacing model uniform):
struct InstanceInput {
    @location(5) model_col0: vec4<f32>,
    @location(6) model_col1: vec4<f32>,
    @location(7) model_col2: vec4<f32>,
    @location(8) model_col3: vec4<f32>,
    @location(9) color:      vec4<f32>,
    @location(10) flags:     u32,
};

@vertex
fn vs_mesh(vertex: VertexInput, inst: InstanceInput) -> VertexOutput {
    let model = mat4x4(inst.model_col0, inst.model_col1, inst.model_col2, inst.model_col3);
    // ... rest of existing VS logic using `model` ...
}
```

The `@group(1)` model uniform bind group layout is removed. The mesh pipeline layout shrinks from 3+ groups to 2+ groups (camera + texture groups).

### Render Loop Integration

**`crates/rython-renderer/src/lib.rs` — `render_meshes()`**

1. Build `InstanceBuffer` from `Vec<DrawMesh>` — collect all draw commands, sort by `InstanceBatchKey`.
2. Upload all batches.
3. For each batch: bind shared textures once, issue `draw_indexed(0..index_count, 0, 0..instance_count)`.

### Key Files to Modify

| File | Change |
|------|--------|
| `crates/rython-renderer/src/instance.rs` | New — `InstanceData`, `InstanceBuffer`, `InstanceBatchKey` |
| `crates/rython-renderer/src/shaders.rs` | Instance vertex input in `MESH_WGSL`; remove model uniform group |
| `crates/rython-renderer/src/gpu.rs` | Second vertex buffer layout; remove model BGL |
| `crates/rython-renderer/src/lib.rs` | Batch collection + instanced draw loop |

---

## Python API

No API changes — instancing is automatic. From the developer's perspective, spawning 1000 entities with the same `mesh_id` and `texture_id` automatically batches into one draw call.

```python
# These 500 trees are rendered in one draw call automatically
for i in range(500):
    rython.scene.spawn(
        transform=rython.Transform(
            random.uniform(-50, 50), 0, random.uniform(-50, 50)
        ),
        mesh={"mesh_id": "models/tree.glb", "texture_id": "textures/tree.png"},
    )
```

### Performance Query (optional exposure)

```python
stats = rython.renderer.get_render_stats()
print(stats.draw_calls)    # should be much lower with instancing
print(stats.instance_count)
```

---

## Test Cases

### Test 1: Two entities with same key → one draw call

- **Setup:** Spawn 2 meshes with identical `mesh_id` and `texture_id`.
- **Expected:** `InstanceBuffer.batches` has 1 key with 2 entries. GPU issues 1 draw call.

### Test 2: Different texture IDs → separate batches

- **Setup:** Spawn mesh A with `texture_id="t1.png"` and mesh B with `texture_id="t2.png"` (same `mesh_id`).
- **Expected:** 2 batches, 2 draw calls.

### Test 3: `InstanceBuffer.clear()` empties all batches

- **Setup:** Push 100 instances. Call `clear()`.
- **Expected:** `batches` is empty; no stale data in next frame.

### Test 4: `max_instances` limit respected

- **Setup:** Push `max_instances + 1 = 4097` instances to one batch.
- **Expected:** Warning logged; only 4096 instances uploaded; excess dropped.

### Test 5: Instance data matrix matches entity world transform

- **Setup:** Spawn entity at `(3, 2, 1)`. Retrieve `InstanceData.model_col3`.
- **Expected:** `model_col3.xyz ≈ (3, 2, 1)` (translation column).

### Test 6: Instanced rendering produces same visual output as per-draw

- **Setup:** Headless render with 3 entities. Compare pixel output between old per-draw and new instanced path.
- **Expected:** Pixel-level match (or within GPU rounding).

### Test 7: Normal map batching key

- **Setup:** Same `mesh_id`/`texture_id`, but one entity has `normal_map_id=Some("n.png")` and one has `None`.
- **Expected:** Two separate batches (different `InstanceBatchKey`).

### Test 8: GPU buffer resizes when batch grows

- **Setup:** Initially push 10 instances. Next frame push 100.
- **Expected:** `gpu_buffers` entry for that key is reallocated at the larger size.

### Test 9: Zero instances skips draw call

- **Setup:** Push 0 instances to a batch.
- **Expected:** No draw call issued for that batch.

### Test 10: `InstanceData` size is 96 bytes

- **Expected:** `std::mem::size_of::<InstanceData>() == 96`.

### Test 11: Flags bit0 set correctly for textured mesh

- **Setup:** Instance with `texture_id != ""`.
- **Expected:** `InstanceData.flags & 1 == 1`.

---

## Gauntlet of Cubes Demo

**Where:** `game/scripts/levels/arena_3.py` (perimeter walls) and `game/scripts/levels/arena_1.py` (floor tiles).

**Effect — Arena 3 perimeter walls:** The boss arena spawns 18 circular wall segments arranged at `radius=11`. All 18 use `mesh_id="cube"` and `texture_id="game/assets/textures/Red/red_wall.png"` — a perfect instancing candidate. Currently they issue 18 separate draw calls. After instancing: 1 draw call.

**Effect — Arena 1 floor:** The 20×20 ground plane is a single large static block today. If it were broken into individual 1×1 tile cubes (better for per-tile variation and future decoration), that would be 400 cubes — all identical mesh and texture. Instancing batches them to 1 draw call.

**Example — verify batching works (no code change required, just a render stats check):**

```python
# game/scripts/main.py — add a debug readout to HUD
def _game_tick():
    # ... existing tick ...
    if _debug_mode:
        stats = rython.renderer.get_render_stats()
        rython.renderer.draw_text(
            f"Draw calls: {stats.draw_calls}  Instances: {stats.instance_count}",
            x=0.01, y=0.95, size=12,
            r=200, g=200, b=200,
        )
```

**Expected draw call reduction in Arena 3:**

| Before instancing | After instancing |
|-------------------|-----------------|
| 18 wall draws | 1 wall draw |
| 3 pickup draws | 1 pickup draw (same mesh+texture) |
| 6 enemy draws (wave 1) | 1 enemy draw (all purple cubes) |
| ≈ 30+ total | ≈ 8 batches |

The entire enemy wave — 11 enemies at peak in wave 2 — becomes a single instanced draw call because they all use `mesh_id="cube"` and `texture_id="game/assets/textures/Purple/purple_box.png"`. No Python code changes needed; the reduction happens automatically inside `render_meshes()`.

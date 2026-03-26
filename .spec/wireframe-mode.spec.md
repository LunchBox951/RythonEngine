# Wireframe Mode

**Status:** Pending
**Priority:** Medium-Impact, Lower Effort
**SPEC.md entry:** §14

---

## Overview

Debug visualization that draws mesh geometry as wireframe lines instead of filled triangles. Useful in the editor for inspecting geometry topology and checking mesh normals. Supports three scopes:

- **Global wireframe** — All meshes rendered as wireframe.
- **Per-entity overlay** — Wireframe lines drawn on top of the shaded mesh (without replacing it).
- **Editor-only** — Wireframe mode active only in `rython-editor` build, not in game builds.

Implementation uses `wgpu::PolygonMode::Line` where available, with a fallback barycentric wireframe shader for backends that do not support `Features::POLYGON_MODE_LINE`.

---

## Rust Implementation

### Settings

**`crates/rython-renderer/src/wireframe.rs`** (new file)

```rust
pub enum WireframeScope {
    Off,
    Global,          // All meshes become wireframe
    SelectedOnly,    // Only entities in `selected_entities` set
}

pub struct WireframeSettings {
    pub scope:              WireframeScope,
    pub color:              [f32; 4],   // RGBA; default [0.0, 1.0, 0.0, 1.0] (green)
    pub line_width:         f32,        // wgpu only honors this on some backends; default 1.0
    pub overlay:            bool,       // true = draw wireframe on top of shaded mesh
    pub selected_entities:  HashSet<EntityId>,
}
```

### Dual-Pipeline Approach

**`crates/rython-renderer/src/gpu.rs`**

Two mesh pipelines:
1. `mesh_pipeline` — existing, `PolygonMode::Fill`.
2. `wireframe_pipeline` — same vertex/fragment shader, `PolygonMode::Line` (requires `wgpu::Features::POLYGON_MODE_LINE`).

At `GpuContext` creation: check `adapter.features().contains(POLYGON_MODE_LINE)`. If not available, set `wireframe_fallback = true` and use the barycentric shader.

### Fallback: Barycentric Wireframe Shader

When `POLYGON_MODE_LINE` is unavailable (e.g. on Metal or WebGPU), use a geometry-shader-free barycentric technique:

**`crates/rython-renderer/src/shaders.rs`** — New `WIREFRAME_BARYCENTRIC_WGSL`:

- Vertices carry a `barycentric: vec3<f32>` attribute: `(1,0,0)`, `(0,1,0)`, `(0,0,1)` cycling per triangle.
- Fragment shader computes minimum barycentric component; draws a line if `min(bary) < line_thickness`.

```wgsl
// Vertex: add barycentric attribute at location 11
@location(11) barycentric: vec3<f32>,

// Fragment: wireframe edge detection
fn edge_factor(bary: vec3<f32>, width: f32) -> f32 {
    let d = fwidth(bary) * width;
    let a = smoothstep(d * 0.0, d, bary);
    return min(a.x, min(a.y, a.z));
}

// In fs_main for wireframe overlay:
let ef = edge_factor(in.barycentric, 1.5);
if (ef > 0.99) { discard; }  // not on edge
return vec4(wire_color.rgb, 1.0 - ef);  // anti-aliased edge
```

### Barycentric Attribute Generation

**`crates/rython-resources/src/lib.rs`**

When wireframe mode uses barycentric fallback, vertices need the attribute. Since it cycles per-triangle (not per-vertex), the mesh must be converted to a non-indexed triangle list:

```rust
pub fn generate_barycentric_coords(mesh: &MeshData) -> Vec<[f32; 3]>;
// Returns one barycentric vec3 per triangle vertex (3 × num_triangles values).
```

A secondary vertex buffer is created for barycentric data only when wireframe mode is active.

### Entity-Level Toggle

**`crates/rython-ecs/src/component.rs` — `MeshComponent`**

```rust
pub struct MeshComponent {
    // ... existing fields ...
    pub wireframe_overlay: bool,  // NEW — draw wireframe on top; default false
}
```

### Key Files to Modify

| File | Change |
|------|--------|
| `crates/rython-renderer/src/wireframe.rs` | New — `WireframeSettings`, `WireframeScope` |
| `crates/rython-renderer/src/shaders.rs` | `WIREFRAME_BARYCENTRIC_WGSL` |
| `crates/rython-renderer/src/gpu.rs` | `wireframe_pipeline`, feature detection |
| `crates/rython-renderer/src/lib.rs` | Conditional wireframe draw pass |
| `crates/rython-ecs/src/component.rs` | `wireframe_overlay` on `MeshComponent` |
| `crates/rython-scripting/src/bridge/renderer.rs` | Expose wireframe API |

---

## Python API

```python
# Global wireframe
rython.renderer.set_wireframe("global")            # "off" | "global" | "selected"
rython.renderer.set_wireframe_color(0.0, 1.0, 0.0, 1.0)
rython.renderer.set_wireframe_overlay(True)        # keep shaded mesh visible underneath

# Per-entity overlay
entity = rython.scene.spawn(
    transform=rython.Transform(0, 0, 0),
    mesh={
        "mesh_id":           "cube",
        "texture_id":        "white.png",
        "wireframe_overlay": True,   # NEW
    },
)

# Selected-only mode (editor use)
rython.renderer.set_wireframe("selected")
rython.renderer.set_wireframe_selected([entity.id, other_entity.id])
```

---

## Test Cases

### Test 1: Default wireframe is off

- **Expected:** `WireframeSettings.scope == Off`.

### Test 2: `POLYGON_MODE_LINE` feature detection

- **Setup:** Query adapter at startup.
- **Expected:** `wireframe_fallback` is set to `true` or `false` based on actual adapter capability; no panic either way.

### Test 3: Barycentric coords cycle correctly

- **Setup:** `generate_barycentric_coords` for a 2-triangle mesh (6 vertices).
- **Expected:** Returns `[(1,0,0),(0,1,0),(0,0,1),(1,0,0),(0,1,0),(0,0,1)]`.

### Test 4: `edge_factor` returns 0 at triangle centroid

- **Setup:** `bary=(1/3, 1/3, 1/3)`, `width=1.5`.
- **Expected:** `edge_factor > 0.99` (centroid is interior, not an edge → discard).

### Test 5: `edge_factor` returns < 0.5 near vertex (edge pixel)

- **Setup:** `bary=(0.01, 0.5, 0.49)`, `width=1.5`.
- **Expected:** `edge_factor < 0.5` (near edge → keep, draw wire color).

### Test 6: Global wireframe applies to all visible meshes

- **Setup:** 5 entities. `set_wireframe("global")`.
- **Expected:** 5 wireframe draw calls encoded; 0 fill draw calls (if not overlay).

### Test 7: Overlay draws two passes per entity

- **Setup:** 2 entities, `scope=Global`, `overlay=True`.
- **Expected:** 4 draw calls total (2 fill + 2 wireframe).

### Test 8: Selected-only mode skips non-selected entities

- **Setup:** 3 entities; 1 in `selected_entities`.
- **Expected:** Only 1 wireframe draw call.

### Test 9: `wireframe_overlay=False` entity skips wireframe even in global mode

- **Setup:** 3 entities; `scope=Global`, one entity with `wireframe_overlay=False`.
- **Action:** Verify — global wireframe should still apply. `wireframe_overlay` only controls the per-entity overlay, not global mode.
- **Expected:** All 3 entities rendered in wireframe under global mode. Per-entity `wireframe_overlay` means "always add wireframe regardless of global mode".

### Test 10: Wireframe color is passed to fragment shader

- **Setup:** `set_wireframe_color(1.0, 0.0, 0.0, 1.0)` (red).
- **Action:** Read back uniform buffer.
- **Expected:** `WireframeUniform.color == [1.0, 0.0, 0.0, 1.0]`.

---

## Gauntlet of Cubes Demo

**Where:** `game/scripts/main.py` — debug key handler; `game/scripts/npc/skeleton.py` — enemy entity.

**Effect 1 — Global debug toggle:** Bind a key (e.g. F3) to cycle through `"off"` → `"global"` → `"off"`. In Arena 1 the 20×20 floor becomes a green wireframe grid; the platform edges are immediately legible. In Arena 3 the 18-wall circle resolves as polygon outlines — confirming the geometry is correct without opening the editor.

**Effect 2 — Enemy hitbox visualisation:** Add `wireframe_overlay: True` to enemy mesh components. The wireframe is drawn on top of the purple texture, showing the exact collision box during gameplay — useful when tuning `ATTACK_RANGE` in `skeleton.py`.

**Example — F3 debug toggle in `game/scripts/main.py`:**

```python
_wireframe_on = False

def _game_tick():
    global _wireframe_on
    if rython.input.key_just_pressed("F3"):
        _wireframe_on = not _wireframe_on
        if _wireframe_on:
            rython.renderer.set_wireframe("global")
            rython.renderer.set_wireframe_color(0.0, 1.0, 0.0, 0.9)
            rython.renderer.set_wireframe_overlay(True)
        else:
            rython.renderer.set_wireframe("off")
    # ... existing tick ...
```

**Example — enemy hitbox wireframe in `game/scripts/level_builder.py`:**

```python
def spawn_enemy(x, y, z, enemy_type, is_boss=False, tags=None):
    scale = (1.5, 2.5, 1.5) if is_boss else (1.0, 2.0, 1.0)
    entity = rython.scene.spawn(
        transform=rython.Transform(x, y, z,
            scale_x=scale[0], scale_y=scale[1], scale_z=scale[2]),
        mesh={
            "mesh_id":           "cube",
            "texture_id":        "game/assets/textures/Purple/purple_box.png",
            "wireframe_overlay": True,    # always-on hitbox visualisation
        },
        rigid_body={"body_type": "dynamic", "mass": 5.0 if is_boss else 2.0},
        collider={"shape": "box", "size": list(scale)},
    )
    enemies.register(entity, enemy_type, is_boss)
    _registered.append(entity)
    return entity
```

**Tip:** The `wireframe_overlay` flag on enemies can be controlled by a `DEBUG_HITBOXES = True` constant at the top of `level_builder.py`, making it a one-line toggle for QA sessions.

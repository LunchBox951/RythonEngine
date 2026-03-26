# Level of Detail (LOD)

**Status:** Pending
**Priority:** Medium-Impact, Lower Effort
**SPEC.md entry:** §12

---

## Overview

Reduce geometry complexity for distant objects by switching between pre-authored mesh variants at configurable distance thresholds. The system operates as an ECS system that selects the correct mesh ID per entity each frame before the render system runs.

---

## Rust Implementation

### New Component

**`crates/rython-ecs/src/component.rs` — `LodComponent`**

```rust
pub struct LodLevel {
    pub mesh_id:           String,
    pub max_distance:      f32,    // switch to next level beyond this distance; f32::INFINITY for last
}

pub struct LodComponent {
    pub levels:            Vec<LodLevel>,  // sorted ascending by max_distance
    pub current_level:     usize,          // index into levels; updated by LodSystem
    pub cull_distance:     f32,            // distance beyond which entity is fully invisible; default f32::INFINITY
}
```

Invariants enforced at construction:
- `levels.len() >= 2` (at least one LOD transition).
- `levels` sorted by `max_distance` ascending.
- Last level has `max_distance == f32::INFINITY`.

### LOD System

**`crates/rython-ecs/src/systems/lod.rs`** (new file)

```rust
pub struct LodSystem;

impl LodSystem {
    /// For each entity with both LodComponent and MeshComponent:
    /// 1. Compute distance from camera.
    /// 2. Select the LodLevel whose max_distance >= dist.
    /// 3. Write selected mesh_id into MeshComponent.mesh_id.
    /// 4. Set MeshComponent.visible = false if dist > cull_distance.
    pub fn run(
        scene: &Scene,
        world_transforms: &HashMap<EntityId, WorldTransform>,
        camera_position: Vec3,
    );
}
```

`LodSystem::run()` is inserted between `TransformSystem::run()` and `RenderSystem::run()` in the frame loop.

Distance is computed as Euclidean distance from entity world position to `camera_position`. For large objects, distance to the nearest bounding sphere edge is preferred — but since there's no bounding sphere support yet, center-to-center distance is used.

### Frame Loop Insertion

**`crates/rython-renderer/src/lib.rs`** or **`crates/rython-engine/src/lib.rs`**:

```
7. TransformSystem::run()         → world_transforms
7.5 LodSystem::run()              → updates MeshComponent.mesh_id and .visible  (NEW)
8. RenderSystem::run()            → DrawCommands (uses updated mesh_id)
```

### Key Files to Modify

| File | Change |
|------|--------|
| `crates/rython-ecs/src/component.rs` | New `LodComponent`, `LodLevel` |
| `crates/rython-ecs/src/systems/lod.rs` | New — `LodSystem::run()` |
| Frame loop entry point | Insert `LodSystem::run()` before `RenderSystem` |
| `crates/rython-scripting/src/bridge/scene.rs` | `lod` kwarg in `spawn()` |

---

## Python API

### Scene Spawn Changes

```python
tree = rython.scene.spawn(
    transform=rython.Transform(20, 0, 0),
    mesh={
        "mesh_id":    "models/tree_high.glb",   # default / LOD0
        "texture_id": "textures/tree.png",
    },
    lod={
        "levels": [
            {"mesh_id": "models/tree_high.glb",  "max_distance": 30.0},
            {"mesh_id": "models/tree_mid.glb",   "max_distance": 80.0},
            {"mesh_id": "models/tree_low.glb",   "max_distance": float("inf")},
        ],
        "cull_distance": 200.0,   # optional
    },
)
```

### Dict Schema

```python
{
    "levels": [
        {"mesh_id": str, "max_distance": float},  # at least 2 entries
        # ...
        {"mesh_id": str, "max_distance": float("inf")},  # last entry
    ],
    "cull_distance": float,  # optional, default inf
}
```

---

## Test Cases

### Test 1: LOD0 selected when within first threshold

- **Setup:** Entity at `(0,0,0)`, camera at `(0,0,15)` (dist=15). LOD0 threshold `max_distance=30`.
- **Expected:** After `LodSystem::run()`, `MeshComponent.mesh_id == LOD0.mesh_id`.

### Test 2: LOD1 selected just beyond first threshold

- **Setup:** Same entity. Camera at `(0,0,35)` (dist=35). LOD1 threshold `max_distance=80`.
- **Expected:** `MeshComponent.mesh_id == LOD1.mesh_id`.

### Test 3: Entity culled beyond `cull_distance`

- **Setup:** `cull_distance=200.0`. Camera at `(0,0,210)`.
- **Expected:** `MeshComponent.visible == false`.

### Test 4: Entity re-appears when camera approaches

- **Setup:** Entity culled (test 3). Move camera to `(0,0,100)`.
- **Expected:** `MeshComponent.visible == true`.

### Test 5: LOD construction rejects fewer than 2 levels

- **Setup:** `LodComponent { levels: vec![LodLevel { mesh_id: "m".into(), max_distance: f32::INFINITY }] }`.
- **Expected:** `Err` or panic with `LodComponentError::TooFewLevels`.

### Test 6: LOD construction rejects unsorted levels

- **Setup:** Levels: `[{max_distance:80}, {max_distance:30}, {max_distance:inf}]` (out of order).
- **Expected:** Error or auto-sort with warning.

### Test 7: LOD construction rejects last level not at infinity

- **Setup:** All levels have finite `max_distance`.
- **Expected:** Error: last level must have `max_distance == f32::INFINITY`.

### Test 8: Entity without LOD component is unaffected by LodSystem

- **Setup:** Entity with only `MeshComponent`.
- **Expected:** `LodSystem::run()` does not modify its `mesh_id` or `visible`.

### Test 9: `current_level` index updated correctly

- **Setup:** Move camera from LOD0 zone to LOD2 zone in one frame.
- **Expected:** `LodComponent.current_level` jumps directly to 2 (no transition frames required).

### Test 10: LOD with `cull_distance=infinity` never culls

- **Setup:** Default `cull_distance`. Camera at `(0,0,999999)`.
- **Expected:** `MeshComponent.visible` remains true; last LOD level is selected.

### Test 11: Python `float("inf")` accepted for `max_distance`

- **Setup:** Pass `"max_distance": float("inf")` from Python.
- **Expected:** Stored as `f32::INFINITY` in `LodLevel`.

---

## Gauntlet of Cubes Demo

**Where:** `game/scripts/levels/arena_3.py` — perimeter wall segments; `game/scripts/level_builder.py` — `spawn_static_block()`.

**Effect:** Arena 3's 18 perimeter wall segments form a ring at `radius=11`. When the camera is at the centre of the arena (which `camera_follow.py` targets), wall segments on the far side are ~22 units away. At that distance a low-poly variant (e.g. a simple flat face instead of a subdivided cube) is visually indistinguishable. LOD silently reduces the triangle count of distant walls each frame.

**Example — LOD on perimeter walls in `game/scripts/levels/arena_3.py`:**

```python
import math

def _spawn_perimeter_walls():
    for i in range(18):
        angle = (i / 18) * 2 * math.pi
        x = math.cos(angle) * 11
        z = math.sin(angle) * 11
        wall = rython.scene.spawn(
            transform=rython.Transform(x, 1.0, z, scale_x=3.5, scale_y=2.0, scale_z=0.5),
            mesh={
                "mesh_id":    "cube",
                "texture_id": "game/assets/textures/Red/red_wall.png",
            },
            lod={
                "levels": [
                    {"mesh_id": "cube",      "max_distance": 15.0},  # full cube
                    {"mesh_id": "cube_low",  "max_distance": float("inf")},  # flat quad
                ],
                "cull_distance": 40.0,
            },
            tags={"tags": ["wall"]},
        )
        _registered.append(wall)
```

**Example — LOD on Arena 2 floating platforms:**

```python
# Arena 2 platforms span a long path; far ones are barely visible
platform = rython.scene.spawn(
    transform=rython.Transform(px, py, pz, scale_x=4, scale_y=0.5, scale_z=4),
    mesh={"mesh_id": "cube", "texture_id": "game/assets/textures/Orange/orange_box.png"},
    lod={
        "levels": [
            {"mesh_id": "cube",     "max_distance": 20.0},
            {"mesh_id": "cube_low", "max_distance": float("inf")},
        ],
        "cull_distance": 60.0,
    },
    tags={"tags": ["platform"]},
)
```

**Asset needed:** `cube_low` — a 6-face flat mesh with 8 vertices (no subdivisions), registered in the asset store. The performance saving in the demo is modest (simple geometry), but the system proves itself when `cube` is replaced by high-poly assets later.

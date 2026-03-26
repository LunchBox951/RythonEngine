# Multiple Light Sources

**Status:** Pending
**Priority:** High-Impact, Moderate Effort
**SPEC.md entry:** §5

---

## Overview

Extend the renderer beyond the single hardcoded directional light at `(0.5, 1.0, 0.5)` to support:

- **Directional lights** — Infinite parallel rays with direction, color, intensity.
- **Point lights** — Omni-directional; attenuate with distance (inverse square + linear falloff).
- **Spot lights** — Cone-shaped point light with inner/outer angle.

A maximum of `MAX_LIGHTS = 16` lights are sent to the GPU per frame via a uniform array. The existing hardcoded light is replaced by a scene-level `LightComponent`.

---

## Rust Implementation

### New Component

**`crates/rython-ecs/src/component.rs` — `LightComponent`**

```rust
#[derive(Clone, Debug)]
pub enum LightKind {
    Directional {
        direction: [f32; 3],
    },
    Point {
        radius: f32,       // effective range (beyond which contribution < threshold)
    },
    Spot {
        direction:    [f32; 3],
        inner_angle:  f32,   // radians — full intensity inside this cone
        outer_angle:  f32,   // radians — zero intensity beyond this cone
    },
}

pub struct LightComponent {
    pub kind:      LightKind,
    pub color:     [f32; 3],    // linear RGB; default [1,1,1]
    pub intensity: f32,         // default 1.0
    pub enabled:   bool,        // default true
    pub cast_shadows: bool,     // reserved for shadow spec §3; default false
}
```

The entity owning a `LightComponent` with `LightKind::Point` or `Spot` uses its `TransformComponent.position` as the light position.

### GPU Light Buffer

**`crates/rython-renderer/src/light.rs`** (new file)

```rust
pub const MAX_LIGHTS: usize = 16;

/// GPU-side light representation — must be 64 bytes (4×vec4) for alignment.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuLight {
    pub position_or_dir: [f32; 4],  // xyz = pos (point/spot) or dir (directional); w = type (0=dir,1=point,2=spot)
    pub color_intensity:  [f32; 4],  // xyz = color, w = intensity
    pub spot_params:      [f32; 4],  // x = inner_cos, y = outer_cos, z = radius, w = enabled (0 or 1)
    pub _pad:             [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightBuffer {
    pub lights:      [GpuLight; MAX_LIGHTS],
    pub light_count: u32,
    pub ambient:     [f32; 3],   // scene-wide ambient light color*intensity
    pub _pad:        [f32; 0],
}
```

### Light System

**`crates/rython-ecs/src/systems/light.rs`** (new file)

```rust
pub struct LightSystem;

impl LightSystem {
    /// Collect all LightComponents with their world positions, build a LightBuffer.
    /// If more than MAX_LIGHTS enabled lights exist, the MAX_LIGHTS brightest (by intensity) are chosen.
    pub fn run(
        scene: &Scene,
        world_transforms: &HashMap<EntityId, WorldTransform>,
        ambient: [f32; 3],
    ) -> LightBuffer;
}
```

Called in the frame loop between transform computation and render collection (step 8.5).

### Ambient Light Setting

**`crates/rython-renderer/src/settings.rs`** (or existing settings file)

```rust
pub struct SceneLightingSettings {
    pub ambient_color:     [f32; 3],   // default [0.1, 0.1, 0.1]
    pub ambient_intensity: f32,        // default 1.0
}
```

### GPU Changes

**`crates/rython-renderer/src/gpu.rs`**

```rust
pub light_bgl:    wgpu::BindGroupLayout,  // @group(7): LightBuffer uniform
pub light_buffer: wgpu::Buffer,           // dynamic, updated each frame
```

**`crates/rython-renderer/src/shaders.rs` — `MESH_WGSL`**

Remove hardcoded `light_dir`. Replace with loop over `LightBuffer`:

```wgsl
const MAX_LIGHTS: u32 = 16u;

struct GpuLight {
    position_or_dir: vec4<f32>,
    color_intensity:  vec4<f32>,
    spot_params:      vec4<f32>,
    _pad:             vec4<f32>,
};

struct LightBuffer {
    lights:      array<GpuLight, 16>,
    light_count: u32,
    ambient:     vec3<f32>,
    _pad:        f32,
};
@group(7) @binding(0) var<uniform> light_buf: LightBuffer;

fn compute_light_contribution(
    N: vec3<f32>,
    world_pos: vec3<f32>,
    view_dir: vec3<f32>,
    uv: vec2<f32>,
) -> vec3<f32> {
    var total = light_buf.ambient;

    for (var i = 0u; i < light_buf.light_count; i++) {
        let light = light_buf.lights[i];
        if (light.spot_params.w < 0.5) { continue; }  // disabled

        let kind = u32(light.position_or_dir.w);
        var L: vec3<f32>;
        var attenuation: f32 = 1.0;

        if (kind == 0u) {
            // Directional
            L = normalize(-light.position_or_dir.xyz);
        } else {
            // Point or Spot
            let to_light = light.position_or_dir.xyz - world_pos;
            let dist = length(to_light);
            L = to_light / dist;
            let radius = light.spot_params.z;
            attenuation = clamp(1.0 - (dist / radius), 0.0, 1.0);
            attenuation *= attenuation;  // smooth quadratic

            if (kind == 2u) {
                // Spot cone
                let cos_angle = dot(L, normalize(-light.spot_params.xyz));  // reuse field
                let inner_cos = light.spot_params.x;
                let outer_cos = light.spot_params.y;
                attenuation *= clamp(
                    (cos_angle - outer_cos) / (inner_cos - outer_cos),
                    0.0, 1.0
                );
            }
        }

        let diffuse = max(dot(N, L), 0.0);
        total += light.color_intensity.xyz * light.color_intensity.w
               * attenuation * diffuse;
    }
    return total;
}
```

### Key Files to Modify

| File | Change |
|------|--------|
| `crates/rython-ecs/src/component.rs` | New `LightComponent`, `LightKind` enum |
| `crates/rython-ecs/src/systems/light.rs` | New — `LightSystem::run()` |
| `crates/rython-renderer/src/light.rs` | New — `GpuLight`, `LightBuffer`, `MAX_LIGHTS` |
| `crates/rython-renderer/src/shaders.rs` | Replace hardcoded light with `LightBuffer` loop |
| `crates/rython-renderer/src/gpu.rs` | `light_bgl`, `light_buffer` field + upload |
| `crates/rython-renderer/src/lib.rs` | Call `LightSystem::run()`, upload `LightBuffer` |
| `crates/rython-scripting/src/bridge/scene.rs` | `light` kwarg in `spawn()` |

---

## Python API

### Scene Spawn Changes

```python
# Directional light
sun = rython.scene.spawn(
    transform=rython.Transform(0, 10, 0),
    light={
        "type":      "directional",
        "direction": (0.5, -1.0, 0.3),
        "color":     (1.0, 0.95, 0.8),   # warm sunlight
        "intensity": 1.5,
    },
)

# Point light
torch = rython.scene.spawn(
    transform=rython.Transform(2, 1, 0),
    light={
        "type":      "point",
        "color":     (1.0, 0.4, 0.1),
        "intensity": 3.0,
        "radius":    8.0,
    },
)

# Spot light
lamp = rython.scene.spawn(
    transform=rython.Transform(0, 5, 0),
    light={
        "type":        "spot",
        "direction":   (0.0, -1.0, 0.0),
        "color":       (1.0, 1.0, 1.0),
        "intensity":   2.0,
        "inner_angle": 15.0,   # degrees
        "outer_angle": 30.0,   # degrees
    },
)
```

### Ambient Light API

```python
rython.renderer.set_ambient_light(r=0.1, g=0.1, b=0.15, intensity=1.0)
```

### Toggle

```python
# Disable a light at runtime
light_comp = torch.get_component("LightComponent")
light_comp.enabled = False
```

---

## Test Cases

### Test 1: `LightSystem` with no lights produces ambient-only buffer

- **Setup:** Scene with no `LightComponent` entities, ambient `[0.1, 0.1, 0.1]`.
- **Expected:** `LightBuffer.light_count == 0`, `LightBuffer.ambient == [0.1, 0.1, 0.1]`.

### Test 2: Directional light populates GpuLight correctly

- **Setup:** Single directional light, `direction=(0,1,0)`, `color=(1,1,1)`, `intensity=1.0`.
- **Expected:** `GpuLight.position_or_dir.w == 0.0` (type=directional), `.xyz == (0,1,0)`.

### Test 3: Point light uses entity world position

- **Setup:** Point light entity at `TransformComponent { x:3, y:2, z:1 }`.
- **Expected:** `GpuLight.position_or_dir.xyz == (3,2,1)`, `.w == 1.0` (type=point).

### Test 4: Excess lights — only MAX_LIGHTS are submitted

- **Setup:** Spawn 20 enabled point lights.
- **Expected:** `LightBuffer.light_count == 16`; the 16 with highest `intensity` are chosen.

### Test 5: Disabled light is excluded

- **Setup:** 2 lights; one with `enabled = false`.
- **Expected:** `LightBuffer.light_count == 1`.

### Test 6: Spot light inner/outer angles converted to cosines

- **Setup:** Spot light `inner_angle=15°`, `outer_angle=30°`.
- **Expected:** `GpuLight.spot_params.x ≈ cos(15°) ≈ 0.9659`, `.y ≈ cos(30°) ≈ 0.866`.

### Test 7: Python `direction` in degrees for spot light

- **Setup:** Pass `inner_angle=0.0, outer_angle=90.0`.
- **Expected:** Inner cosine ≈ 1.0, outer cosine ≈ 0.0.

### Test 8: Ambient light setting is applied

- **Setup:** `set_ambient_light(r=0.2, g=0.3, b=0.4)`.
- **Expected:** `LightBuffer.ambient == [0.2, 0.3, 0.4]`.

### Test 9: Point light attenuation reaches zero at radius

- **Setup:** Point light at origin, `radius=5.0`. Fragment at distance `5.0`.
- **Expected:** `attenuation == 0.0`.

### Test 10: Legacy hardcoded light removed from shader

- **Setup:** Inspect `MESH_WGSL` for the string `"0.5, 1.0, 0.5"`.
- **Expected:** Not found — hardcoded light direction has been removed.

### Test 11: Zero lights, zero ambient → fully dark scene

- **Setup:** `light_count=0`, `ambient=[0,0,0]`.
- **Expected:** All visible mesh fragments output `color = emissive_only`.

---

## Gauntlet of Cubes Demo

**Where:** `game/scripts/levels/arena_3.py` (boss arena) and each arena's level loader.

**Effect:** Each arena gets a distinct lighting mood. Arena 1 (tutorial) is bright and welcoming. Arena 2 (void gauntlet) gets blue-white overhead light to feel cold and exposed. Arena 3 (boss fight) switches to a single deep-red overhead directional and adds a point light rising from the lava pit — so the floor around the lava glows red while the perimeter stays dim.

**Example — per-arena ambient + directional in `_on_load_level()`:**

```python
def _on_load_level(data):
    level = data.get("level", 1)
    if level == 1:
        rython.renderer.set_ambient_light(0.25, 0.25, 0.28)
        # Sun remains default direction; warm white
    elif level == 2:
        rython.renderer.set_ambient_light(0.05, 0.05, 0.12)
        # Cold blue-white directional is set in arena_2.py
    elif level == 3:
        rython.renderer.set_ambient_light(0.08, 0.02, 0.02)
```

**Example — lava point light in `game/scripts/levels/arena_3.py`:**

```python
# Point light hovering above the lava pit — illuminates the floor in a red circle
lava_light = rython.scene.spawn(
    transform=rython.Transform(0, 1.5, 0),
    light={
        "type":      "point",
        "color":     (1.0, 0.15, 0.0),
        "intensity": 4.0,
        "radius":    7.0,
    },
    tags={"tags": ["lava_light"]},
)
_registered.append(lava_light)

# Directional sun override for boss fight: low angle from the back
sun = rython.scene.spawn(
    transform=rython.Transform(0, 0, 0),
    light={
        "type":      "directional",
        "direction": (0.2, -0.6, 0.8),
        "color":     (0.8, 0.1, 0.0),
        "intensity": 0.9,
    },
    tags={"tags": ["arena3_sun"]},
)
_registered.append(sun)
```

**Why it's compelling here:** Arena 3 already has the lava pit dealing damage — a red point light emanating from it makes the danger zone visually unmistakable and sells the atmosphere without any gameplay code changes.

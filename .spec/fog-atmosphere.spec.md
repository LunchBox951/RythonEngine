# Fog / Atmosphere

**Status:** Pending
**Priority:** Medium-Impact, Lower Effort | Quick Win (depth fog)
**SPEC.md entries:** §9, Quick Wins ("Depth fog")

---

## Overview

Distance-based or height-based fog for depth perception and outdoor atmosphere. Fog blends the final fragment color toward a fog color as a function of distance from the camera. Two modes are supported:

- **Linear fog** — Fog factor increases linearly between `near` and `far` distances.
- **Exponential fog** — `fog = 1 - exp(-density * distance)`. Denser near the camera.
- **Exponential-squared** — `fog = 1 - exp(-(density * distance)^2)`. Sharper onset.
- **Height fog** — Exponential density that falls off with world-space Y (altitude).

The "Quick Win" depth fog from SPEC.md is implemented here as Linear or Exponential mode with `enabled = true`.

---

## Rust Implementation

### New Types

**`crates/rython-renderer/src/fog.rs`** (new file)

```rust
#[repr(u32)]
#[derive(Copy, Clone)]
pub enum FogMode {
    Linear          = 0,
    Exponential     = 1,
    ExponentialSq   = 2,
    Height          = 3,
}

pub struct FogSettings {
    pub enabled:      bool,
    pub mode:         FogMode,
    pub color:        [f32; 3],    // linear RGB fog color; default [0.5, 0.5, 0.5]
    pub density:      f32,         // exponential modes; default 0.05
    pub near:         f32,         // linear mode — fog starts; default 10.0
    pub far:          f32,         // linear mode — fog reaches 100%; default 100.0
    pub height_start: f32,         // height mode — fog floor Y; default 0.0
    pub height_falloff: f32,       // height mode — density falloff per unit Y; default 0.5
}
```

### GPU Uniform

**`crates/rython-renderer/src/fog.rs`**

```rust
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct FogUniform {
    pub color:         [f32; 4],   // xyz=color, w=enabled (0.0 or 1.0)
    pub mode:          u32,
    pub density:       f32,
    pub near:          f32,
    pub far:           f32,
    pub height_start:  f32,
    pub height_falloff: f32,
    pub _pad:          [f32; 2],
}
```

Stored in a uniform buffer at `@group(10) @binding(0)` in the mesh pipeline.

**`crates/rython-renderer/src/gpu.rs`**

```rust
pub fog_bgl:     wgpu::BindGroupLayout,
pub fog_buffer:  wgpu::Buffer,
pub fog_settings: FogSettings,
```

### Shader Changes

**`crates/rython-renderer/src/shaders.rs` — `MESH_WGSL`**

Fragment shader receives `world_pos` (already needed for lighting). Add fog application:

```wgsl
struct FogUniform {
    color:          vec4<f32>,
    mode:           u32,
    density:        f32,
    near:           f32,
    far:            f32,
    height_start:   f32,
    height_falloff: f32,
    _pad:           vec2<f32>,
};
@group(10) @binding(0) var<uniform> fog: FogUniform;

fn compute_fog_factor(world_pos: vec3<f32>, eye_pos: vec3<f32>) -> f32 {
    if (fog.color.w < 0.5) { return 0.0; }  // disabled

    let dist = length(world_pos - eye_pos);

    switch (fog.mode) {
        case 0u: {
            // Linear
            return clamp((dist - fog.near) / (fog.far - fog.near), 0.0, 1.0);
        }
        case 1u: {
            // Exponential
            return 1.0 - exp(-fog.density * dist);
        }
        case 2u: {
            // Exponential squared
            return 1.0 - exp(-pow(fog.density * dist, 2.0));
        }
        case 3u: {
            // Height fog
            let height_above = max(world_pos.y - fog.height_start, 0.0);
            let height_density = fog.density * exp(-fog.height_falloff * height_above);
            return 1.0 - exp(-height_density * dist);
        }
        default: { return 0.0; }
    }
}

// In fs_main, after computing lit_color:
let fog_factor = compute_fog_factor(in.world_pos, camera.eye_position);
let final_color = mix(lit_color_with_emissive, fog.color.xyz, fog_factor);
```

Fog is applied **after** emissive but **before** any HDR post-processing. This matches the physical expectation that emissive objects still fog over at distance.

### Key Files to Modify

| File | Change |
|------|--------|
| `crates/rython-renderer/src/fog.rs` | New — `FogSettings`, `FogUniform`, `FogMode` |
| `crates/rython-renderer/src/shaders.rs` | Fog function + application in `MESH_WGSL` |
| `crates/rython-renderer/src/gpu.rs` | `fog_bgl`, `fog_buffer`, `FogSettings` |
| `crates/rython-renderer/src/lib.rs` | Upload `FogUniform` each frame |
| `crates/rython-scripting/src/bridge/renderer.rs` | Expose fog API |

---

## Python API

```python
# Enable simple depth fog (Quick Win)
rython.renderer.set_fog_enabled(True)
rython.renderer.set_fog_mode("linear")          # "linear" | "exp" | "exp2" | "height"
rython.renderer.set_fog_color(0.6, 0.65, 0.7)  # grey-blue outdoor fog
rython.renderer.set_fog_linear(near=20.0, far=80.0)

# Exponential fog
rython.renderer.set_fog_mode("exp")
rython.renderer.set_fog_density(0.03)

# Height fog
rython.renderer.set_fog_mode("height")
rython.renderer.set_fog_density(0.08)
rython.renderer.set_fog_height(start=0.0, falloff=0.4)
```

---

## Test Cases

### Test 1: Fog disabled by default

- **Expected:** `FogSettings.enabled == false`; `FogUniform.color.w == 0.0`.

### Test 2: Linear fog factor is 0 at near distance

- **Setup:** `mode=Linear`, `near=10.0`, `far=100.0`. Fragment at `dist=5.0`.
- **Expected:** `fog_factor == 0.0`.

### Test 3: Linear fog factor is 1 at far distance

- **Setup:** `mode=Linear`, `near=10.0`, `far=100.0`. Fragment at `dist=100.0`.
- **Expected:** `fog_factor == 1.0`.

### Test 4: Linear fog factor clamps at 1 beyond far

- **Setup:** `mode=Linear`, `near=10.0`, `far=100.0`. Fragment at `dist=200.0`.
- **Expected:** `fog_factor == 1.0` (clamped).

### Test 5: Exponential fog is always < 1 at finite distance

- **Setup:** `mode=Exponential`, `density=0.05`. Fragment at `dist=1000.0`.
- **Expected:** `fog_factor < 1.0` (asymptotic approach).

### Test 6: Exponential-squared rises faster than exponential

- **Setup:** Same `density=0.05`, `dist=20.0`. Compute both.
- **Expected:** `exp_sq_factor > exp_factor`.

### Test 7: Height fog — object at height above `height_start` has less fog

- **Setup:** `mode=Height`, `height_start=0.0`, `height_falloff=1.0`. Compare fragments at Y=0 and Y=5, same horizontal distance.
- **Expected:** Fragment at Y=5 has lower `fog_factor`.

### Test 8: `set_fog_enabled(false)` disables fog mid-session

- **Setup:** Enable fog. Render frame. Disable. Render frame.
- **Expected:** Second frame has `FogUniform.color.w == 0.0`.

### Test 9: Fog color round-trips through Python API

- **Setup:** `set_fog_color(0.3, 0.4, 0.5)`.
- **Action:** Read `fog_buffer` uniform.
- **Expected:** `FogUniform.color.xyz == [0.3, 0.4, 0.5]`.

### Test 10: FogUniform size is multiple of 16 bytes

- **Expected:** `std::mem::size_of::<FogUniform>() % 16 == 0`.

### Test 11: Fog is applied after emissive

- **Setup:** Headless render; mesh with `emissive_color=(1,0,0)` at high distance (fog_factor ≈ 1).
- **Expected:** Fragment color is predominantly fog color, not emissive red. (Emissive does not bypass fog.)

---

## Gauntlet of Cubes Demo

**Where:** Each arena's load function and `game/scripts/main.py` `_on_load_level()`.

**Effect:** Three fog configurations, one per arena — each serves a different gameplay and aesthetic purpose:

- **Arena 1 (Tutorial):** Subtle light-grey linear fog starting at distance 25. Makes the arena edges fade out softly; the player doesn't see hard clip-plane geometry pop-in.
- **Arena 2 (Gauntlet Run):** No distance fog, but **height fog** above the platforms. Wispy mist sits just above the orange surfaces, making the void below feel colder and the platform tops feel like islands emerging from cloud cover.
- **Arena 3 (Boss Arena):** Dense exponential fog in dark red-grey. The circular arena is 22 units across; enemies at the far wall are barely visible through the haze. The boss emerges from the fog in wave 2 — a pure staging moment.

**Example — fog per arena in each level file:**

```python
# arena_1.py — gentle fadeout at arena edges
def load():
    rython.renderer.set_fog_enabled(True)
    rython.renderer.set_fog_mode("linear")
    rython.renderer.set_fog_color(0.72, 0.72, 0.75)   # matches Arena 1 clear color
    rython.renderer.set_fog_linear(near=22.0, far=40.0)
    # ...

# arena_2.py — height fog, void below platforms
def load():
    rython.renderer.set_fog_enabled(True)
    rython.renderer.set_fog_mode("height")
    rython.renderer.set_fog_color(0.4, 0.5, 0.65)
    rython.renderer.set_fog_density(0.12)
    rython.renderer.set_fog_height(start=-1.0, falloff=0.6)
    # ...

# arena_3.py — thick atmospheric fog for boss tension
def load():
    rython.renderer.set_fog_enabled(True)
    rython.renderer.set_fog_mode("exp2")
    rython.renderer.set_fog_color(0.18, 0.04, 0.04)   # dark blood-red haze
    rython.renderer.set_fog_density(0.04)
    # ...
```

**Reset fog on level transition in `game/scripts/main.py`:**

```python
def _on_load_level(data):
    rython.renderer.set_fog_enabled(False)   # each arena sets its own
    level_builder.clear_level()
    # ...
```

# Material Properties — Quick Wins

**Status:** Pending
**Priority:** Quick Wins (1–2 hour implementations)
**SPEC.md entries:** Quick Wins section — Fixed-function material properties, Light direction editor, Background color picker

---

## Overview

Three small, self-contained improvements that address hardcoded values in the current renderer. Each is independent and can be implemented in any order.

---

## 1. Fixed-Function Material Properties

**Goal:** Expose `metallic` and `roughness` as uniform floats accessible from MeshComponent and Python. These are not full PBR (see §16), but scalar hints that allow shaders to incorporate them for basic material variation without textures.

### Rust

**`crates/rython-ecs/src/component.rs` — `MeshComponent`**

```rust
pub struct MeshComponent {
    // ... existing fields ...
    pub metallic:  f32,   // NEW — [0,1]; default 0.0
    pub roughness: f32,   // NEW — [0,1]; default 0.5
}
```

**`crates/rython-renderer/src/shaders.rs` — `MESH_WGSL`**

Add to `ModelUniform`:

```wgsl
struct ModelUniform {
    // ... existing ...
    metallic:  f32,   // NEW
    roughness: f32,   // NEW
    _pad_mr:   vec2<f32>,
};
```

These values are available in the fragment shader for use by future Phong-to-PBR bridge code. Until PBR (§16) is implemented, the shader may use `roughness` to modulate `shininess`: `effective_shininess = model.shininess * (1.0 - model.roughness)`.

### Python

```python
entity = rython.scene.spawn(
    transform=rython.Transform(0, 0, 0),
    mesh={
        "mesh_id":    "models/sphere.glb",
        "texture_id": "textures/metal.png",
        "metallic":   1.0,    # NEW
        "roughness":  0.2,    # NEW (shiny metal)
    },
)
```

### Test Cases

**QW-1.1:** Default `metallic=0.0`, `roughness=0.5` when not specified.
**QW-1.2:** Values round-trip through `ModelUniform` to GPU.
**QW-1.3:** `metallic` clamped to `[0,1]`; warning logged for out-of-range values.
**QW-1.4:** `roughness=0.0` maximizes `shininess`; `roughness=1.0` minimizes it.
**QW-1.5:** Existing scenes without these fields deserialize without error (defaults applied).

---

## 2. Light Direction Editor

**Goal:** Make the hardcoded directional light direction `(0.5, 1.0, 0.5)` configurable via the renderer API and editor UI. This is a prerequisite for the full multi-light spec (§5), but usable independently.

### Rust

**`crates/rython-renderer/src/settings.rs`** (or existing settings):

```rust
pub struct DirectionalLightSettings {
    pub direction: [f32; 3],   // world-space direction; default [0.5, 1.0, 0.5] normalized
    pub color:     [f32; 3],   // default [1.0, 1.0, 1.0]
    pub intensity: f32,        // default 1.0
}
```

**`crates/rython-renderer/src/shaders.rs` — `MESH_WGSL`**

Replace hardcoded `let light_dir = normalize(vec3(0.5, 1.0, 0.5))` with a uniform:

```wgsl
struct DirectionalLightUniform {
    direction: vec4<f32>,   // xyz = normalized direction, w = intensity
    color:     vec4<f32>,   // xyz = color, w = unused
};
@group(11) @binding(0) var<uniform> dir_light: DirectionalLightUniform;
```

Until multi-light (§5) is implemented, this single uniform replaces the hardcoded value. When §5 is implemented, this is superseded by `LightBuffer`.

**`crates/rython-renderer/src/gpu.rs`**:

```rust
pub dir_light_settings: DirectionalLightSettings,
pub dir_light_bgl:      wgpu::BindGroupLayout,
pub dir_light_buffer:   wgpu::Buffer,
```

Updated each frame if settings changed.

### Python

```python
rython.renderer.set_light_direction(0.5, -1.0, 0.3)    # NEW
rython.renderer.set_light_color(1.0, 0.9, 0.8)          # NEW — warm sunlight
rython.renderer.set_light_intensity(1.5)                 # NEW
```

### Editor Integration

`rython-editor` exposes three sliders in the Scene Settings panel:
- **Light Direction:** 3-component normalized direction picker (sphere widget or XYZ sliders).
- **Light Color:** Color picker.
- **Light Intensity:** Float slider `[0, 5]`.

### Test Cases

**QW-2.1:** Default direction is `normalize([0.5, 1.0, 0.5])` when not configured.
**QW-2.2:** `set_light_direction` normalizes the input vector before storing.
**QW-2.3:** Zero vector `(0,0,0)` is rejected; falls back to `(0,1,0)` with warning.
**QW-2.4:** `DirectionalLightUniform.direction.xyz` matches set value.
**QW-2.5:** Direction change takes effect on the next rendered frame.
**QW-2.6:** Old hardcoded `vec3(0.5, 1.0, 0.5)` literal no longer present in `MESH_WGSL`.

---

## 3. Background Color Picker

**Goal:** Make the clear color (currently hardcoded `(0.15, 0.15, 0.15)` grey) configurable.

### Rust

**`crates/rython-renderer/src/settings.rs`**:

```rust
pub struct SceneSettings {
    pub clear_color: [f32; 4],   // RGBA linear; default [0.15, 0.15, 0.15, 1.0]
}
```

**`crates/rython-renderer/src/lib.rs` — `render_frame()`**

Replace `wgpu::Color { r: 0.15, g: 0.15, b: 0.15, a: 1.0 }` with `SceneSettings.clear_color`.

### Python

```python
rython.renderer.set_clear_color(0.05, 0.05, 0.1, 1.0)   # NEW — dark blue night sky
rython.renderer.set_clear_color(0.53, 0.81, 0.98, 1.0)  # sky blue
```

### Editor Integration

`rython-editor` Scene Settings panel adds a **Background Color** RGBA color picker.

### Test Cases

**QW-3.1:** Default clear color is `[0.15, 0.15, 0.15, 1.0]`.
**QW-3.2:** `set_clear_color` round-trips through `SceneSettings`.
**QW-3.3:** Clear color used in `RenderPassColorAttachment.clear_value`.
**QW-3.4:** Alpha channel of clear color is honored (for transparent swap chains on supported platforms).
**QW-3.5:** Out-of-range values `< 0.0` or `> 1.0` are clamped with a warning.
**QW-3.6:** Hardcoded `0.15, 0.15, 0.15` literal removed from `lib.rs`.

---

## Key Files Summary

| Quick Win | Files Modified |
|-----------|---------------|
| Material Properties | `rython-ecs/component.rs`, `rython-renderer/shaders.rs`, `rython-renderer/lib.rs` |
| Light Direction | `rython-renderer/settings.rs`, `rython-renderer/shaders.rs`, `rython-renderer/gpu.rs`, `rython-renderer/lib.rs`, `rython-scripting/bridge/renderer.rs` |
| Background Color | `rython-renderer/settings.rs`, `rython-renderer/lib.rs`, `rython-scripting/bridge/renderer.rs` |

---

## Gauntlet of Cubes Demo

**Where:** `game/scripts/levels/arena_1.py`, `arena_2.py`, `arena_3.py` — each arena calls these settings on load; `game/scripts/main.py` — `_on_load_level()` orchestrates the transition.

All three quick wins working together create the most immediate visual upgrade in the game for the least implementation effort.

**Background color — each arena gets a matching sky tone:**

```python
# arena_1.py — light stone tutorial; pale blue-grey sky
rython.renderer.set_clear_color(0.62, 0.65, 0.70, 1.0)

# arena_2.py — floating platforms over void; deep midnight blue
rython.renderer.set_clear_color(0.05, 0.06, 0.14, 1.0)

# arena_3.py — hellish boss arena; near-black with red tint
rython.renderer.set_clear_color(0.08, 0.01, 0.01, 1.0)
```

**Light direction — per-arena sun angle:**

```python
# arena_1.py — friendly overhead sun, slightly warm
rython.renderer.set_light_direction(0.3, -1.0, 0.4)
rython.renderer.set_light_color(1.0, 0.96, 0.88)
rython.renderer.set_light_intensity(1.1)

# arena_2.py — cold side-light from the left, raking across platform edges
rython.renderer.set_light_direction(-0.8, -0.5, 0.2)
rython.renderer.set_light_color(0.85, 0.90, 1.0)
rython.renderer.set_light_intensity(0.95)

# arena_3.py — low backlight, red-tinged — makes enemies silhouette dramatically
rython.renderer.set_light_direction(0.1, -0.4, 0.9)
rython.renderer.set_light_color(0.9, 0.15, 0.05)
rython.renderer.set_light_intensity(0.85)
```

**Metallic/roughness — pickup boxes and boss:**

```python
# Pickups — slightly shiny plastic look
mesh={
    "mesh_id":    "cube",
    "texture_id": "game/assets/textures/Green/green_box.png",
    "metallic":   0.0,
    "roughness":  0.3,   # shiny but not mirror
}

# Boss skeleton — dull metal surface
mesh={
    "mesh_id":    "cube",
    "texture_id": "game/assets/textures/Purple/purple_box.png",
    "metallic":   0.85,
    "roughness":  0.45,
}
```

**Reset on transition in `game/scripts/main.py`:**

```python
def _on_load_level(data):
    # Each arena file sets its own values immediately after this reset
    rython.renderer.set_clear_color(0.15, 0.15, 0.15, 1.0)  # neutral default
    rython.renderer.set_light_direction(0.5, -1.0, 0.5)
    rython.renderer.set_light_color(1.0, 1.0, 1.0)
    rython.renderer.set_light_intensity(1.0)
    level_builder.clear_level()
    # ... load new level ...
```

**These three changes require zero new assets** and immediately differentiate the three arenas visually. They are the correct first implementation step before tackling any of the heavier features.

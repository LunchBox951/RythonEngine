# Skybox / Cubemap Reflections

**Status:** Pending
**Priority:** High-Impact, Moderate Effort
**SPEC.md entry:** §8

---

## Overview

Render a skybox using a cubemap texture loaded from 6 face images or an equirectangular HDR image. The cubemap can also be used for environment reflections on materials with a specular or metallic component. The skybox is rendered last in the 3D pass with the depth trick (write depth = 1.0) to appear behind all geometry.

---

## Rust Implementation

### New Types

**`crates/rython-resources/src/cubemap.rs`** (new file)

```rust
/// Six faces of a cubemap in order: +X, -X, +Y, -Y, +Z, -Z
pub struct CubemapData {
    pub faces: [ImageData; 6],   // each face must be square and equal size
    pub size:  u32,              // pixels per side
}

impl CubemapData {
    /// Load 6 separate image files (PNG/JPG) as faces.
    /// Face order: right, left, top, bottom, front, back.
    pub fn from_faces(paths: [&str; 6]) -> Result<Self, ResourceError>;

    /// Convert equirectangular (2:1 aspect) image to 6-face cubemap.
    /// `face_size` is the output resolution per face.
    pub fn from_equirectangular(image: &ImageData, face_size: u32) -> Self;
}
```

**`crates/rython-renderer/src/skybox.rs`** (new file)

```rust
pub struct SkyboxResources {
    pub cubemap_texture: wgpu::Texture,           // D6, CubeCompatible view
    pub cubemap_view:    wgpu::TextureView,       // ViewDimension::Cube
    pub sampler:         wgpu::Sampler,           // LinearClamp
    pub vertex_buffer:   wgpu::Buffer,            // 36 vertices (unit cube, no normals)
    pub pipeline:        wgpu::RenderPipeline,
    pub bgl:             wgpu::BindGroupLayout,   // @group(0): cubemap + sampler + camera
    pub bind_group:      wgpu::BindGroup,
    pub intensity:       f32,                     // environment contribution scale; default 1.0
}

pub struct SkyboxSettings {
    pub enabled:     bool,
    pub asset_id:    String,           // key into asset store for CubemapData
    pub intensity:   f32,              // sky brightness
    pub use_for_reflections: bool,     // bind to mesh pipeline for env reflections
}
```

### Cubemap GPU Upload

**`crates/rython-renderer/src/gpu.rs`**

Extend `GpuContext::process_uploads()` to handle `CubemapData`:
- Create `wgpu::Texture` with `TextureDimension::D2`, `array_layer_count = 6`, flag `CUBE_COMPATIBLE`.
- Upload each face as a separate `write_texture` call.
- Create a `TextureViewDescriptor` with `dimension = Cube`.

### Skybox Vertex Data

A unit cube with vertices at `±1` on all axes, wound so that each face is visible from inside. The skybox shader discards the translation component of the view matrix (top-left 3×3 only) so the skybox stays centered on the camera.

### Shader Changes

**`crates/rython-renderer/src/shaders.rs`** — New constant `SKYBOX_WGSL`:

```wgsl
struct SkyboxUniform {
    view_proj_no_translation: mat4x4<f32>,  // view rotation only, no translation
};
@group(0) @binding(0) var<uniform> sky: SkyboxUniform;
@group(0) @binding(1) var t_cubemap: texture_cube<f32>;
@group(0) @binding(2) var s_cubemap: sampler;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) local_pos: vec3<f32>,
};

@vertex
fn vs_skybox(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.local_pos    = in.position;
    var clip         = sky.view_proj_no_translation * vec4(in.position, 1.0);
    out.clip_position = clip.xyww;  // set z = w so depth = 1.0 after division
    return out;
}

@fragment
fn fs_skybox(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(t_cubemap, s_cubemap, normalize(in.local_pos));
}
```

**Modified `MESH_WGSL`** — Environment reflection (when `use_for_reflections = true`):

```wgsl
@group(9) @binding(0) var t_env_cube: texture_cube<f32>;
@group(9) @binding(1) var s_env_cube: sampler;

// In fs_main (added after specular):
if (model.has_env_reflection != 0u) {
    let R = reflect(-view_dir, N);
    let env = textureSample(t_env_cube, s_env_cube, R).rgb;
    total_color += env * model.env_reflection_strength;
}
```

New `ModelUniform` fields:

```wgsl
has_env_reflection:     u32,    // NEW
env_reflection_strength: f32,   // NEW
```

New `MeshComponent` fields:

```rust
pub env_reflection: bool,       // default false
pub env_reflection_strength: f32,  // default 0.3
```

### Frame Loop Changes

**`crates/rython-renderer/src/lib.rs`**

Skybox renders at the end of the 3D pass (after all meshes) with depth test enabled but no depth write. This ensures the skybox only fills pixels not covered by geometry.

### Key Files to Modify

| File | Change |
|------|--------|
| `crates/rython-resources/src/cubemap.rs` | New — `CubemapData`, face loading, equirect conversion |
| `crates/rython-renderer/src/skybox.rs` | New — `SkyboxResources`, `SkyboxSettings` |
| `crates/rython-renderer/src/shaders.rs` | `SKYBOX_WGSL`; env reflection in `MESH_WGSL` |
| `crates/rython-renderer/src/gpu.rs` | Cubemap upload, `SkyboxResources` field |
| `crates/rython-renderer/src/lib.rs` | Skybox at end of 3D pass |
| `crates/rython-ecs/src/component.rs` | `env_reflection`, `env_reflection_strength` on `MeshComponent` |
| `crates/rython-scripting/src/bridge/renderer.rs` | `set_skybox()`, `set_skybox_enabled()` |

---

## Python API

### Skybox Setup

```python
# Load from 6 face images
rython.renderer.set_skybox(
    faces=[
        "skybox/right.png",  "skybox/left.png",
        "skybox/top.png",    "skybox/bottom.png",
        "skybox/front.png",  "skybox/back.png",
    ],
    intensity=1.0,
    use_for_reflections=True,
)

# Load from equirectangular HDR
rython.renderer.set_skybox_hdr(
    path="hdri/studio.hdr",
    face_size=512,
    intensity=1.5,
)

rython.renderer.set_skybox_enabled(True)
```

### Mesh Reflection

```python
entity = rython.scene.spawn(
    transform=rython.Transform(0, 0, 0),
    mesh={
        "mesh_id":                "models/sphere.glb",
        "texture_id":             "textures/chrome.png",
        "env_reflection":         True,            # NEW
        "env_reflection_strength": 0.6,            # NEW
    },
)
```

---

## Test Cases

### Test 1: `CubemapData::from_faces` rejects mismatched face sizes

- **Setup:** Load 6 images where one is 512×512 and others are 256×256.
- **Expected:** `Err(ResourceError::CubemapFaceSizeMismatch)`.

### Test 2: Cubemap texture has 6 array layers

- **Setup:** Upload a valid `CubemapData` to GPU.
- **Expected:** `wgpu::Texture.depth_or_array_layers == 6`.

### Test 3: Texture view dimension is Cube

- **Setup:** Create view from cubemap texture.
- **Expected:** `TextureViewDescriptor.dimension == Some(Cube)`.

### Test 4: Skybox clip position has z == w

- **Setup:** Inspect vertex shader output for a skybox vertex.
- **Expected:** `clip.z == clip.w` (depth becomes 1.0 after perspective division).

### Test 5: View matrix translation is stripped for skybox

- **Setup:** Camera at `(100, 50, 200)`. Compute `view_proj_no_translation`.
- **Expected:** The translation component of `view_proj_no_translation` is zero; rotation is preserved.

### Test 6: `set_skybox_enabled(false)` skips skybox draw call

- **Setup:** Enable then disable skybox.
- **Expected:** No skybox draw encoded in command buffer when disabled.

### Test 7: Missing skybox asset logs warning, not panic

- **Setup:** `set_skybox(asset_id="nonexistent_sky")`.
- **Action:** Render frame.
- **Expected:** Warning logged; skybox draw skipped; no crash.

### Test 8: Environment reflection group not bound when not in use

- **Setup:** Spawn mesh with `env_reflection=False`.
- **Expected:** Bind group 9 is not set; `has_env_reflection == 0`.

### Test 9: Equirectangular conversion produces correct face count

- **Setup:** `CubemapData::from_equirectangular(equirect_image, 256)`.
- **Expected:** 6 faces, each `256×256`, all non-zero pixels.

### Test 10: `use_for_reflections=false` does not bind cubemap to mesh pipeline

- **Setup:** `set_skybox(use_for_reflections=False)`.
- **Expected:** Mesh pipeline bind group 9 is not set; no env reflection in any mesh.

---

## Gauntlet of Cubes Demo

**Where:** Each arena's load function (`arena_1.py`, `arena_2.py`, `arena_3.py`).

**Effect:** Arena 2 is the most compelling showcase. Currently there is no floor and no skybox — the player runs across orange platforms while surrounded by the engine's hardcoded grey clear color. A sky cubemap transforms the void into a dramatic cloudscape or starfield. The floating-platform aesthetic is intentional; giving it a real sky makes the height feel genuine.

**Example — cloud sky for Arena 2 in `game/scripts/levels/arena_2.py`:**

```python
def load():
    rython.renderer.set_skybox(
        faces=[
            "game/assets/skybox/void_right.png",  "game/assets/skybox/void_left.png",
            "game/assets/skybox/void_top.png",    "game/assets/skybox/void_bottom.png",
            "game/assets/skybox/void_front.png",  "game/assets/skybox/void_back.png",
        ],
        intensity=1.0,
        use_for_reflections=False,   # platforms are matte; no env reflection needed
    )
    rython.renderer.set_skybox_enabled(True)
    # ... existing platform spawning ...
```

**Example — hellish sky for Arena 3 with reflective lava in `game/scripts/levels/arena_3.py`:**

```python
def load():
    rython.renderer.set_skybox(
        faces=[
            "game/assets/skybox/hell_right.png",  "game/assets/skybox/hell_left.png",
            "game/assets/skybox/hell_top.png",    "game/assets/skybox/hell_bottom.png",
            "game/assets/skybox/hell_front.png",  "game/assets/skybox/hell_back.png",
        ],
        intensity=0.8,
        use_for_reflections=True,   # lava pit picks up env color
    )
    rython.renderer.set_skybox_enabled(True)

    # Lava pit gets env reflection — red sky reflected in it
    lava = rython.scene.spawn(
        transform=rython.Transform(0, 0.05, 0, scale_x=6, scale_y=0.1, scale_z=6),
        mesh={
            "mesh_id":                "cube",
            "texture_id":             "game/assets/textures/Red/red_box.png",
            "env_reflection":         True,
            "env_reflection_strength": 0.5,
        },
        tags={"tags": ["lava"]},
    )
    _registered.append(lava)
    # ... existing arena setup ...
```

**Cleanup — disable skybox on arena transition in `game/scripts/main.py` `_on_load_level()`:**

```python
def _on_load_level(data):
    rython.renderer.set_skybox_enabled(False)   # reset before new arena sets its own
    level_builder.clear_level()
    # ... load new level ...
```

**Assets to add:** Two 6-face cubemap sets: a pale blue/cloud set for Arena 2, and a dark orange/red ember set for Arena 3. 256×256 per face is sufficient for the demo.

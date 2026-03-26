# Tangent-Space Normal Compression

**Status:** Pending
**Priority:** Advanced, Higher Effort
**SPEC.md entry:** §20
**Depends on:** normal-mapping.spec.md (§1) — extends the tangent-space normal map pipeline

---

## Overview

Optimize normal map storage and GPU sampling by adopting `BC5 / DXT5nm` two-channel compression. Only the X and Y channels of the tangent-space normal are stored; Z is reconstructed in the fragment shader (`Z = sqrt(1 - X² - Y²)`). This reduces normal map memory by 25–50% compared to uncompressed Rgba8Unorm, while preserving accuracy for signed XY values.

Additionally, this spec defines the compression workflow and the on-import detection logic so that pre-compressed `.ktx2` normal maps are loaded directly without decompression.

---

## Rust Implementation

### Normal Compression Format Support

**`crates/rython-resources/src/lib.rs`** — Extend `ImageData`:

```rust
pub enum TextureEncoding {
    Rgba8Unorm,           // standard uncompressed
    Bc5Unorm,             // 2-channel BC5 (normal maps)
    Bc7Unorm,             // 4-channel BC7 (high-quality color)
    Etc2Rgba8,            // ETC2 for mobile
}

pub struct ImageData {
    pub width:    u32,
    pub height:   u32,
    pub pixels:   Vec<u8>,         // raw bytes (format-dependent)
    pub encoding: TextureEncoding, // NEW
}
```

When `encoding == Bc5Unorm`, `pixels` contains the raw BC5 block data (not RGBA).

### Compression Path

**`crates/rython-resources/src/compress.rs`** (new file)

```rust
/// Compress a normal map's XY channels to BC5 using the `texpresso` crate.
/// Input: Rgba8Unorm image (only RG channels used).
/// Output: Bc5Unorm ImageData.
pub fn compress_normal_bc5(image: &ImageData) -> Result<ImageData, ResourceError>;

/// Decompress BC5 → Rgba8Unorm (for platforms without BC5 hardware support).
pub fn decompress_bc5(image: &ImageData) -> ImageData;
```

Compression is performed at asset import time (not at runtime). The game project stores pre-compressed `.ktx2` or raw BC5 blobs.

**Cargo dependency** (add to `rython-resources/Cargo.toml`):

```toml
texpresso = { version = "2", features = ["bc5"] }
```

### GPU Upload Changes

**`crates/rython-renderer/src/gpu.rs` — `process_uploads()`**

Check `ImageData.encoding` when creating `wgpu::Texture`:

```rust
let format = match image.encoding {
    TextureEncoding::Rgba8Unorm => wgpu::TextureFormat::Rgba8Unorm,
    TextureEncoding::Bc5Unorm   => wgpu::TextureFormat::Bc5RgUnorm,
    TextureEncoding::Bc7Unorm   => wgpu::TextureFormat::Bc7RgbaUnorm,
    TextureEncoding::Etc2Rgba8  => wgpu::TextureFormat::Etc2Rgb8UnormSrgb,  // approx
};
```

For `Bc5RgUnorm`, the texture requires `wgpu::Features::TEXTURE_COMPRESSION_BC` on the adapter. If the feature is unavailable, decompress to Rgba8Unorm as a fallback.

### Shader Changes

**`crates/rython-renderer/src/shaders.rs` — `MESH_WGSL`**

Add Z reconstruction for BC5 normal maps:

```wgsl
struct ModelUniform {
    // ...
    normal_map_compressed: u32,   // NEW — 1 = BC5/DXT5nm, reconstruct Z
};

fn sample_normal_map(t: texture_2d<f32>, s: sampler, uv: vec2<f32>, compressed: u32) -> vec3<f32> {
    let sample = textureSample(t, s, uv);
    if (compressed != 0u) {
        // BC5/DXT5nm: only RG channels stored; reconstruct Z
        let xy = sample.rg * 2.0 - 1.0;
        let z  = sqrt(max(1.0 - dot(xy, xy), 0.0));
        return normalize(vec3(xy, z));
    } else {
        return normalize(sample.rgb * 2.0 - 1.0);
    }
}
```

**`MeshComponent`** gains one field:

```rust
pub normal_map_compressed: bool,   // NEW — set automatically on asset load
```

### Asset Importer Detection

**`crates/rython-resources/src/loaders/`** — When loading a normal map:

1. If the file extension is `.ktx2`: parse KTX2 header, read format, set `encoding` accordingly.
2. If the file is a raw `.png`/`.jpg` tagged as a normal map (e.g. by filename suffix `_n.png`): load as Rgba8Unorm. No automatic compression at runtime.
3. If `encoding == Bc5Unorm`: set `normal_map_compressed = true` on the `MeshComponent`.

### Key Files to Modify

| File | Change |
|------|--------|
| `crates/rython-resources/src/lib.rs` | `TextureEncoding` enum, `encoding` field on `ImageData` |
| `crates/rython-resources/src/compress.rs` | New — `compress_normal_bc5`, `decompress_bc5` |
| `crates/rython-renderer/src/gpu.rs` | `process_uploads` — format selection from encoding |
| `crates/rython-renderer/src/shaders.rs` | `sample_normal_map` with Z reconstruction |
| `crates/rython-ecs/src/component.rs` | `normal_map_compressed` on `MeshComponent` |

---

## Python API

No new runtime API. Compression is a build/import step. Python developers interact with the normal map exactly as before; the engine detects compression format automatically.

```python
# Works for both uncompressed and BC5 compressed normal maps
entity = rython.scene.spawn(
    transform=rython.Transform(0, 0, 0),
    mesh={
        "mesh_id":    "models/wall.glb",
        "texture_id": "textures/wall_diffuse.png",
        "normal_map": "textures/wall_normal_bc5.ktx2",   # BC5 compressed
    },
)
```

### CLI Tool (optional — post-MVP)

A `rython-cli compress-normals <input.png> <output.ktx2>` sub-command that wraps `compress_normal_bc5`.

---

## Test Cases

### Test 1: `compress_normal_bc5` output has `encoding == Bc5Unorm`

- **Setup:** Load a 64×64 Rgba8Unorm normal map. Compress.
- **Expected:** `output.encoding == TextureEncoding::Bc5Unorm`.

### Test 2: BC5 compressed size is approximately 50% of Rgba8

- **Setup:** 512×512 Rgba8Unorm normal map (1MB). Compress to BC5.
- **Expected:** `output.pixels.len() ≈ 512×512/2 = 131072` bytes (BC5 = 8 bits/texel = 2 channels × 4 bpp).

### Test 3: Z reconstruction gives unit-length normals

- **Setup:** BC5 pixel `(0.5, 0.5)` → decoded XY = `(0, 0)`.
- **Expected:** Reconstructed Z = `sqrt(1 - 0 - 0) = 1.0`; result = `(0, 0, 1)`.

### Test 4: Z reconstruction does not produce NaN

- **Setup:** BC5 pixel with `X=0.9, Y=0.9` → `xy = (0.8, 0.8)`, `dot(xy,xy) = 1.28 > 1.0`.
- **Expected:** `max(1.0 - dot(xy,xy), 0.0) == 0.0`; Z = `0.0`; no NaN.

### Test 5: BC5 normal map auto-sets `normal_map_compressed = true`

- **Setup:** Load `wall_normal.ktx2` (BC5 format). Check `MeshComponent`.
- **Expected:** `normal_map_compressed == true`.

### Test 6: Uncompressed PNG normal map sets `normal_map_compressed = false`

- **Setup:** Load `brick_n.png`.
- **Expected:** `normal_map_compressed == false`; full RGB channel used.

### Test 7: GPU texture format is `Bc5RgUnorm` for BC5 map

- **Setup:** Upload BC5 `ImageData` on a BC-capable adapter.
- **Expected:** `wgpu::Texture.format() == Bc5RgUnorm`.

### Test 8: Fallback to Rgba8Unorm on adapters without BC5 support

- **Setup:** Headless adapter with `BC_TEXTURE_COMPRESSION` feature disabled. Load BC5 normal map.
- **Expected:** `decompress_bc5()` called; texture uploaded as `Rgba8Unorm`; no crash.

### Test 9: Round-trip compression/decompression preserves direction

- **Setup:** Normal `(0, 0, 1)` → compress to BC5 → decompress → reconstruct Z.
- **Expected:** Result within 3 degrees of original normal (BC5 has ~0.5% quantization error).

### Test 10: `compress_normal_bc5` rejects non-power-of-two dimensions

- **Setup:** Input image 100×100 (non-POT).
- **Expected:** `Err(ResourceError::Bc5RequiresPowerOfTwo)` or auto-pad to 128×128 with warning.

---

## Gauntlet of Cubes Demo

**Where:** Asset pipeline for all texture sets used in the game; no runtime Python changes needed.

**Effect:** The Gauntlet of Cubes demo references five texture families (Light, Orange, Dark, Red, Purple). Once normal mapping (§1) is implemented, each family gets a `*_n.png` normal map. Compressing those normal maps to BC5 halves their VRAM footprint compared to uncompressed Rgba8Unorm, which is relevant when the engine scales to larger scenes.

The demo itself is a verification that the feature is transparent to the Python developer: the same spawn code works whether the normal map is uncompressed PNG or BC5 KTX2.

**Demonstration — before and after compression, same Python code:**

```python
# Both of these produce identical visual output;
# the engine detects the format automatically at load time.

# Uncompressed (development)
mesh={
    "mesh_id":    "cube",
    "texture_id": "game/assets/textures/Light/light_floor_grid.png",
    "normal_map": "game/assets/textures/Light/light_floor_grid_n.png",   # Rgba8Unorm PNG
}

# BC5 compressed (shipping build)
mesh={
    "mesh_id":    "cube",
    "texture_id": "game/assets/textures/Light/light_floor_grid.png",
    "normal_map": "game/assets/textures/Light/light_floor_grid_n.ktx2",  # BC5 KTX2
}
```

**Asset pipeline script (run at build time, not at runtime):**

```python
# tools/compress_normals.py — run with rython-cli or standalone
import subprocess, pathlib

NORMAL_MAPS = [
    "game/assets/textures/Light/light_floor_grid_n.png",
    "game/assets/textures/Light/light_wall_n.png",
    "game/assets/textures/Orange/orange_box_n.png",
    "game/assets/textures/Dark/dark_floor_grid_n.png",
    "game/assets/textures/Dark/dark_box_n.png",
    "game/assets/textures/Red/red_wall_n.png",
    "game/assets/textures/Purple/purple_box_n.png",
]

for src in NORMAL_MAPS:
    dst = src.replace("_n.png", "_n.ktx2")
    subprocess.run(["rython", "compress-normals", src, dst], check=True)
    print(f"Compressed: {src} → {dst}")
```

**Measured savings (estimate for 256×256 normal maps):**

| Format | Bytes per texture | 7 normal maps total |
|--------|-------------------|---------------------|
| Rgba8Unorm PNG | 262 144 | 1.75 MB |
| BC5 KTX2 | 65 536 | 0.43 MB |
| **Saving** | **75%** | **~1.3 MB** |

In the current demo the absolute saving is modest. The mechanism proves itself when the game grows to dozens of unique normal maps or when porting to memory-constrained platforms.

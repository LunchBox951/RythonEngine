# Resources

The resource system manages loading, caching, and lifetime of all game assets: images, meshes, sounds, fonts, and spritesheets. It provides an asynchronous, reference-counted asset pipeline with a memory budget and LRU eviction.


## Architecture

The ResourceManager is a Module that acts as a pure orchestrator. It does not decode assets itself — it dispatches decode work to background tasks on the rayon pool and routes decoded data to the appropriate consumer (renderer for GPU uploads, audio system for sound buffers).

```
Request asset -> Check cache -> Cache hit? -> Return existing handle
                                Cache miss? -> Submit background decode task
                                               -> On completion: upload to GPU/AL
                                               -> Mark handle as Ready
```


## Asset Handles

Every asset request returns an AssetHandle immediately, even before the asset is decoded. The handle is a lightweight reference-counted pointer that transitions through states:

```
PENDING  ->  READY   (decode succeeded, data available)
         ->  FAILED  (decode failed, error stored)
```

Game code can check handle state and access data:

```python
import rython

# Request an asset (returns immediately)
texture = rython.resources.load_image("textures/player.png")

# Check if ready
if texture.is_ready():
    # Use it
    rython.renderer.draw_image(asset=texture, x=0.5, y=0.5, w=0.1, h=0.1)
elif texture.is_failed():
    error = texture.error()
    print(f"Failed to load: {error}")
else:
    # Still loading... show placeholder or skip
    pass
```


## Deduplication

If the same asset is requested multiple times, the ResourceManager returns the same handle. No re-decode occurs. The internal reference count increments, and the asset stays alive as long as any handle exists.

```python
# These return the same handle (same path)
tex_a = rython.resources.load_image("textures/player.png")
tex_b = rython.resources.load_image("textures/player.png")
# tex_a and tex_b point to the same underlying data
```


## Decode Pipeline

Asset decoding runs on background tasks (rayon thread pool). Each asset type has a dedicated decoder:

### Image Decoder
Reads PNG, JPG, BMP, TGA files. Decodes to raw RGBA pixel bytes. On completion, the renderer creates a wgpu texture on the main thread.

### Mesh Decoder
Reads glTF files (replacing OBJ/FBX from the PythonEngine). Extracts vertex positions, normals, UVs, and indices. On completion, the renderer creates GPU vertex/index buffers.

### Sound Decoder
Reads WAV, OGG, FLAC, MP3 files. Decodes to PCM sample arrays. On completion, the audio system creates a kira sound data handle.

### Font Decoder
Reads TTF/OTF files. Rasterizes glyphs at a requested size into a texture atlas. On completion, the renderer creates a GPU texture for the glyph atlas.

### Spritesheet Decoder
Reads an image file and splits it into a grid of frames based on column/row counts. Each frame becomes a sub-region of a single GPU texture.

```python
# Load different asset types
texture = rython.resources.load_image("textures/player.png")
mesh = rython.resources.load_mesh("models/weapon.gltf")
sound = rython.resources.load_sound("audio/explosion.wav")
font = rython.resources.load_font("fonts/ui.ttf", size=24)
spritesheet = rython.resources.load_spritesheet("sprites/walk.png", cols=8, rows=1)
```


## GPU Upload Coordination

Asset decoding happens on background threads, but GPU resource creation (wgpu textures, buffers) must happen on the main thread (the thread that owns the wgpu device).

The decode pipeline solves this with a callback pattern:
1. Background task decodes asset to raw bytes
2. On completion, a sequential callback is submitted to the scheduler at IDLE priority
3. The callback runs on the main thread and creates the GPU resource
4. The AssetHandle transitions to READY

This ensures no GPU API calls happen off the main thread.


## Streaming Budget

The ResourceManager enforces a memory budget for loaded assets. The budget caps total memory used by decoded asset data (textures, meshes, sounds in RAM/VRAM).

When the budget is exceeded, the manager evicts the least-recently-used assets that have no active handles. An asset with at least one live handle is never evicted — it stays loaded regardless of budget.

```python
# Check current memory usage
used = rython.resources.memory_used_mb()
budget = rython.resources.memory_budget_mb()
print(f"Assets: {used:.1f} / {budget:.1f} MB")
```


## Preloading

For assets that must be ready before a scene starts (loading screens), use task groups to coordinate:

```python
group = rython.scheduler.create_group(
    callback=self.on_level_ready,
    owner=self,
)

group.add_background(fn=rython.resources.load_image, args=("textures/level02_ground.png",))
group.add_background(fn=rython.resources.load_mesh, args=("models/level02.gltf",))
group.add_background(fn=rython.resources.load_sound, args=("audio/level02_music.ogg",))

group.seal()

# on_level_ready fires when all assets are decoded and uploaded
```


## Configuration

```json
{
    "resources": {
        "streaming_budget_mb": 256
    }
}
```

- `streaming_budget_mb`: Maximum memory for loaded assets before LRU eviction kicks in (default 256 MB)


## Acceptance Tests

### T-RES-01: Load Returns Handle Immediately
Call `load_image("test.png")`. Measure the time to return.
- Expected: The call returns in under 1ms (does not block on decode)
- Expected: The returned handle's state is PENDING

### T-RES-02: Handle Transitions to READY
Load a valid 64x64 PNG image. Poll the handle each frame.
- Expected: Handle starts as PENDING
- Expected: Within 30 frames (0.5s), handle transitions to READY
- Expected: Once READY, `get_data()` returns non-null image data

### T-RES-03: Handle Transitions to FAILED
Load a nonexistent file "does_not_exist.png". Poll the handle.
- Expected: Handle starts as PENDING
- Expected: Within 30 frames, handle transitions to FAILED
- Expected: `error()` returns a message containing the file path

### T-RES-04: Deduplication — Same Path Same Handle
Call `load_image("test.png")` twice. Compare the returned handles.
- Expected: Both handles point to the same underlying data (same internal pointer)
- Expected: Only one background decode task is submitted (not two)

### T-RES-05: Deduplication — Different Paths Different Handles
Call `load_image("a.png")` and `load_image("b.png")`.
- Expected: The handles are distinct
- Expected: Two background decode tasks are submitted

### T-RES-06: Reference Counting — Handle Keeps Asset Alive
Load an image. Clone the handle (2 references). Drop one clone.
- Expected: Asset is still READY (one reference remains)
- Expected: Drop the second clone. Asset becomes eligible for eviction.

### T-RES-07: Image Decode Correctness
Load a known 2x2 PNG with pixels: red(255,0,0), green(0,255,0), blue(0,0,255), white(255,255,255). Read the decoded RGBA bytes.
- Expected: Byte 0-3: (255, 0, 0, 255) — red pixel
- Expected: Byte 4-7: (0, 255, 0, 255) — green pixel
- Expected: Byte 8-11: (0, 0, 255, 255) — blue pixel
- Expected: Byte 12-15: (255, 255, 255, 255) — white pixel

### T-RES-08: Mesh Decode — Vertex Count
Load a known glTF cube mesh (8 vertices, 12 triangles).
- Expected: Decoded mesh has 8 unique vertex positions (or 24 with split normals)
- Expected: Decoded mesh has 36 indices (12 triangles * 3)
- Expected: Vertex positions form a unit cube centered at origin

### T-RES-09: Sound Decode — PCM Output
Load a known 1-second 44100 Hz mono WAV file.
- Expected: Decoded PCM array has exactly 44100 samples
- Expected: Sample values are in the expected range (-1.0 to 1.0 for f32 PCM)

### T-RES-10: Font Decode — Glyph Atlas
Load a TTF font at size 32. Request glyphs for ASCII 32-126 (printable characters).
- Expected: Glyph atlas texture is generated with dimensions that are powers of 2
- Expected: Each printable character has a valid UV rectangle in the atlas
- Expected: Space character (32) has a width > 0 but no visible glyph pixels

### T-RES-11: Spritesheet Decode
Load a 128x32 PNG as a spritesheet with cols=4, rows=1.
- Expected: 4 frame regions are produced
- Expected: Each frame is 32x32 pixels
- Expected: Frame 0 starts at UV (0.0, 0.0), Frame 1 at (0.25, 0.0), etc.

### T-RES-12: Streaming Budget Enforcement
Set streaming_budget_mb=1. Load assets until total exceeds 1 MB. Load one more.
- Expected: After exceeding the budget, at least one handle-less asset is evicted
- Expected: Current memory usage stays at or below 1 MB (within tolerance of one asset size)
- Expected: Assets with active handles are NOT evicted

### T-RES-13: LRU Eviction Order
Load assets A, B, C in order (all handle-less after load). Access A again (touching its LRU timestamp). Trigger eviction.
- Expected: B is evicted first (least recently used)
- Expected: A is evicted last (most recently used)

### T-RES-14: GPU Upload Happens on Main Thread
Load an image. Set a thread-ID marker inside the GPU upload callback. Check the thread ID.
- Expected: The GPU upload callback runs on the main thread (same thread as the scheduler)
- Expected: The callback does NOT run on a rayon worker thread

### T-RES-15: Memory Usage Reporting
Load 3 images of known sizes (e.g., 256x256 RGBA = 256KB each). Query memory_used_mb().
- Expected: Reported usage is approximately 0.75 MB (3 * 0.25 MB)
- Expected: After evicting one, reported usage drops to approximately 0.5 MB

### T-RES-16: Concurrent Load Stress Test
Submit 100 load_image requests simultaneously for 100 different files. Run until all complete.
- Expected: All 100 handles transition to READY (for valid files) or FAILED (for missing files)
- Expected: No panics, no data races, no corrupted decodes
- Expected: Deduplication is correct (no double-decodes for any path)

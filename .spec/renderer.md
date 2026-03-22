# Renderer

The Renderer is a wgpu-based GPU pipeline that replaces the PythonEngine's ModernGL/OpenGL 3.3 renderer. It provides cross-platform rendering through wgpu's abstraction over Vulkan, Metal, DX12, and WebGPU backends.

The renderer follows the same command-driven pattern as the rest of the engine: systems submit DrawCommands to a queue, and the renderer sorts and executes them during the RENDER_EXECUTE phase.


## Rendering Phases

The renderer is built in two phases, matching the PythonEngine's roadmap:

### Phase 2: 2D Rendering
The initial implementation. Supports:
- Filled and bordered rectangles
- Circles and lines
- Textured quads (images/sprites)
- Text rendering via glyph atlases
- Alpha blending
- Z-sorted draw order

### Phase 3: 3D Rendering
Extends Phase 2 with:
- 3D mesh rendering with depth testing
- Directional, point, and spot lights
- Normal mapping and specular highlights
- Billboard sprites in 3D space
- Frustum culling
- FXAA anti-aliasing


## Draw Commands

All rendering goes through DrawCommand submissions. Systems and scripts never call GPU APIs directly — they describe what to draw, and the renderer handles how.

Draw command types:

- **DrawRect**: Filled or bordered rectangle at a screen position
- **DrawCircle**: Filled or bordered circle
- **DrawLine**: Line segment between two points
- **DrawImage**: Textured quad from a loaded image asset
- **DrawText**: Text string rendered with a loaded font
- **DrawMesh**: 3D mesh with material and world transform (Phase 3)
- **DrawBillboard**: Camera-facing sprite in 3D space (Phase 3)

```python
import rython

# 2D drawing (normalized screen space)
rython.renderer.draw_rect(
    x=0.1, y=0.1, w=0.3, h=0.05,
    color=(255, 100, 0),
    z=1.0,
)

rython.renderer.draw_image(
    asset_id="ui/health_bar.png",
    x=0.05, y=0.9, w=0.2, h=0.03,
    alpha=0.8,
    z=5.0,
)

rython.renderer.draw_text(
    text="Score: 1500",
    font_id="ui_font",
    x=0.5, y=0.02,
    color=(255, 255, 255),
    size=24,
    z=10.0,
)
```


## Coordinate System

2D draw commands use **normalized screen space**:
- Origin: top-left (0.0, 0.0)
- Bottom-right: (1.0, 1.0)
- Z-ordering: lower z values are drawn first (painter's algorithm)

This matches the PythonEngine's coordinate system. Game code never deals with pixel coordinates — the engine handles resolution scaling internally.

Colors are specified as (R, G, B) or (R, G, B, A) tuples with values from 0 to 255.


## Per-Frame Pipeline

Each frame, the renderer executes this pipeline:

1. **Clear**: Clear the framebuffer to the configured clear color
2. **Sort**: Sort all queued DrawCommands by z-value (ascending)
3. **Dispatch**: For each command, bind the appropriate pipeline/shader and issue draw calls
4. **Present**: Present the framebuffer to the screen (wgpu surface present)

The draw command queue is double-buffered: the RenderSystem writes to the "back" queue during RENDER_ENQUEUE, and the renderer reads from the "front" queue during RENDER_EXECUTE. The queues swap at the boundary between these phases. This avoids contention between command producers and the renderer.


## Shader System

The renderer compiles a small set of built-in shaders:

- **Primitive shader**: Renders rects, circles, and lines. Uses a unit quad vertex buffer with uniforms for position, size, rotation, color, and mode (fill/border/circle/line).
- **Image shader**: Renders textured quads with alpha blending. Samples from a bound texture.
- **Text shader**: Renders text by sampling a glyph atlas texture. Applies color modulation.
- **Mesh shader** (Phase 3): Standard 3D shader with model/view/projection matrices, lighting, and material properties.

Shaders are written in WGSL (WebGPU Shading Language), wgpu's native shader format.


## GPU Resource Management

GPU resources (textures, buffers, pipelines) are created on the main thread during the RENDER_EXECUTE phase. When the ResourceManager finishes decoding an image or mesh in the background, it queues a GPU upload callback. The renderer processes these callbacks during its tick, creating wgpu textures and buffers.

```
Background thread:  decode image -> raw pixel bytes
Main thread tick:   renderer picks up bytes -> creates wgpu::Texture -> stored in asset handle
```

This ensures all GPU API calls happen on the thread that owns the wgpu device and queue.


## Camera

The Camera provides view and projection matrices for 3D rendering (Phase 3).

Configuration:
- Field of view (default 90 degrees)
- Near clip plane (default 0.1)
- Far clip plane (default 1000.0)
- Billboard mode: cylindrical (rotate around Y axis only) or spherical (face camera fully)
- FXAA toggle

```python
import rython

camera = rython.camera
camera.set_position(0, 10, -20)
camera.set_look_at(0, 0, 0)
camera.set_fov(75)
```

The Camera is a single-owner module. In a game, the gameplay system owns the camera and drives its position/rotation. During cutscenes, ownership transfers to the cutscene system.


## Lighting (Phase 3)

The renderer supports multiple light types:

- **Ambient light**: Global fill light with color and intensity
- **Directional light**: Sun-like parallel rays with direction and color
- **Point light**: Omni-directional light with position, color, radius, and falloff
- **Spot light**: Cone-shaped light with position, direction, angle, and falloff

```python
rython.renderer.set_ambient_light(color=(40, 40, 60), intensity=0.3)

rython.renderer.add_point_light(
    position=(10, 5, 0),
    color=(255, 200, 150),
    radius=20.0,
    falloff=2.0,
)
```


## Configuration

```json
{
    "renderer": {
        "clear_color": [0, 0, 0, 255],
        "max_draw_commands": 65536,
        "msaa_samples": 4,
        "use_fxaa": true
    }
}
```

- `clear_color`: Framebuffer clear color (RGBA, 0-255)
- `max_draw_commands`: Maximum draw commands per frame (pre-allocated buffer)
- `msaa_samples`: Multisample anti-aliasing sample count
- `use_fxaa`: Enable FXAA post-processing (Phase 3)


## Acceptance Tests

### T-REND-01: Renderer Initialization
Create a window and initialize the renderer with default config.
- Expected: wgpu adapter is obtained (Vulkan, Metal, or DX12 — not fallback software)
- Expected: wgpu device and queue are created without error
- Expected: The surface is configured to the window's size
- Expected: The primitive, image, and text shader pipelines compile without error

### T-REND-02: Empty Frame Renders Without Error
Initialize the renderer. Run one frame with zero draw commands.
- Expected: The framebuffer is cleared to the configured clear color
- Expected: `surface.present()` succeeds
- Expected: No GPU validation errors (when validation layers are enabled)

### T-REND-03: Draw Command Z-Sorting
Submit 5 DrawRect commands with z values [5.0, 1.0, 3.0, 2.0, 4.0]. Inspect the sorted command list before dispatch.
- Expected: Commands are sorted as [1.0, 2.0, 3.0, 4.0, 5.0]
- Expected: The command with z=1.0 is rendered first (furthest back)

### T-REND-04: Normalized Coordinate Mapping
Submit a DrawRect at position (0.0, 0.0) with size (1.0, 1.0) on a 1920x1080 window. Inspect the generated vertex positions.
- Expected: The quad covers the entire framebuffer (maps to clip space -1..1)
- Expected: At (0.5, 0.5, 0.5, 0.5), the quad covers the center quarter of the screen

### T-REND-05: Color Value Mapping
Submit a DrawRect with color (255, 0, 128, 200). Inspect the uniform values sent to the shader.
- Expected: R = 1.0, G = 0.0, B ≈ 0.502, A ≈ 0.784 (0-255 mapped to 0.0-1.0)

### T-REND-06: Double-Buffered Command Queue
During RENDER_ENQUEUE, submit 100 draw commands to the back buffer. Simultaneously, the renderer reads from the front buffer (which should be last frame's commands).
- Expected: The renderer never reads a partially-written command from the current frame
- Expected: After the buffer swap, the renderer has exactly 100 commands to process

### T-REND-07: GPU Texture Upload from Background Decode
Decode a 256x256 RGBA image on a background thread (producing 256*256*4 = 262,144 bytes). Submit a GPU upload callback.
- Expected: The wgpu texture is created on the main thread (not the background thread)
- Expected: Texture dimensions are 256x256
- Expected: Texture format is RGBA8Unorm
- Expected: The AssetHandle transitions from PENDING to READY

### T-REND-08: DrawImage with Loaded Texture
Load a test image. Wait for it to become READY. Submit a DrawImage command referencing it. Render one frame.
- Expected: No GPU validation errors
- Expected: The image shader pipeline is bound during dispatch
- Expected: The texture is bound to the correct bind group

### T-REND-09: DrawText Glyph Atlas
Load a TTF font at size 24. Render the text "Hello". Inspect the generated glyph atlas.
- Expected: The glyph atlas texture contains rasterized glyphs for H, e, l, o
- Expected: Each DrawText command produces multiple textured quads (one per glyph)
- Expected: Glyphs are positioned left-to-right with correct kerning offsets

### T-REND-10: Camera View Matrix (Phase 3)
Set camera position to (0, 10, -20), look-at to (0, 0, 0).
- Expected: The view matrix transforms world origin (0,0,0) to a point in front of the camera
- Expected: Camera forward vector points approximately toward (0, -0.447, 0.894)

### T-REND-11: Camera Projection Matrix (Phase 3)
Set FOV to 90 degrees, near=0.1, far=1000.0, aspect ratio 16:9.
- Expected: The projection matrix has correct values for a 90-degree perspective projection
- Expected: Points at z=near map to NDC z=0 (or -1 depending on API convention)
- Expected: Points at z=far map to NDC z=1

### T-REND-12: Max Draw Commands Enforcement
Set `max_draw_commands=100`. Submit 150 draw commands.
- Expected: Only the first 100 commands are processed
- Expected: A warning is logged indicating 50 commands were dropped
- Expected: No buffer overflow or crash

### T-REND-13: Shader Hot-Reload Resilience
Intentionally provide a malformed WGSL shader string.
- Expected: Shader compilation returns an error (not a panic)
- Expected: The renderer logs the error with the shader source location
- Expected: Draw commands that require the failed shader are skipped, other rendering continues

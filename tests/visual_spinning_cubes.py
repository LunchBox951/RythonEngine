"""Visual test: Spinning Cubes

Spawns a ring of nine cubes that rotate around the Y axis, with a frame counter
overlaid as on-screen text.  Exercises the full scripting bridge API:

    - rython.scene.spawn(transform=..., mesh=..., tags=...)
    - rython.camera.set_position / set_look_at
    - rython.scheduler.register_recurring
    - rython.renderer.draw_text
    - rython.time.elapsed
    - rython.engine.request_quit

The engine's scripting module calls init() on load and flush_recurring_callbacks()
each frame.
"""
import math
import rython

# ── Configuration ─────────────────────────────────────────────────────────────

CUBE_COUNT = 9
RING_RADIUS = 3.0
RUN_DURATION = 10.0  # seconds before auto-quit

# ── State ─────────────────────────────────────────────────────────────────────

cubes = []
frame = 0


# ── Entry point ───────────────────────────────────────────────────────────────

def init():
    """Called once by the engine when the script module is loaded."""
    # Camera: pull back and look down at the ring of cubes
    rython.camera.set_position(0.0, 6.0, -14.0)
    rython.camera.set_look_at(0.0, 0.0, 0.0)

    # Spawn a ring of cubes
    for i in range(CUBE_COUNT):
        angle = (2.0 * math.pi * i) / CUBE_COUNT
        x = math.cos(angle) * RING_RADIUS
        z = math.sin(angle) * RING_RADIUS
        cube = rython.scene.spawn(
            transform=rython.Transform(x=x, y=0.0, z=z, scale=1.0),
            mesh="cube",
            tags=["cube", "spinning"],
        )
        cubes.append(cube)

    # Register the per-frame update
    rython.scheduler.register_recurring(on_tick)


# ── Per-frame update ──────────────────────────────────────────────────────────

def on_tick():
    """Called every frame by flush_recurring_callbacks()."""
    global frame
    frame += 1
    t = rython.time.elapsed

    # Spin each cube with a phase offset so they fan out over time
    for i, cube in enumerate(cubes):
        phase = (2.0 * math.pi * i) / CUBE_COUNT
        cube.transform.rot_y = t + phase

    # Overlay HUD text
    rython.renderer.draw_text(
        f"Spinning Cubes  frame={frame}  t={t:.2f}s",
        font_id="default",
        x=0.02,
        y=0.02,
        size=20,
        r=255,
        g=255,
        b=200,
    )

    # Auto-quit after the configured duration
    if t >= RUN_DURATION:
        rython.engine.request_quit()

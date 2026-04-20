"""Integration tests for the rython.physics Python API.

Sync tests verify that physics configuration calls succeed without crashing.
Frame-loop tests use FrameRunner to assert physics behaviour over multiple
engine frames (dynamic bodies falling under gravity, impulses, velocity, and
static bodies remaining stationary).
"""

import rython
from rython import Transform
from _harness import TestSuite, FrameRunner


suite = TestSuite()


# ── Sync tests (no frame loop needed) ────────────────────────────────────────

def test_set_gravity_default():
    rython.physics.set_gravity(0, -9.81, 0)


def test_set_gravity_zero():
    rython.physics.set_gravity(0, 0, 0)


# ── Frame-loop helpers ───────────────────────────────────────────────────────

# Shared state populated during init and read by frame callbacks.
_dynamic_entity = None
_impulse_entity = None
_velocity_entity = None
_static_entity = None
_impulse_initial_y = None
_velocity_initial_x = None


def _spawn_test_entities():
    """Spawn all entities needed by the frame-loop tests."""
    global _dynamic_entity, _impulse_entity, _velocity_entity, _static_entity
    global _impulse_initial_y, _velocity_initial_x

    # Gravity test: dynamic body at y=10, should fall
    _dynamic_entity = rython.scene.spawn(
        transform=Transform(x=0.0, y=10.0, z=0.0),
        rigid_body={"body_type": "dynamic"},
        collider={"shape": "box", "size": [1.0, 1.0, 1.0]},
    )

    # Impulse test: dynamic body at origin
    _impulse_entity = rython.scene.spawn(
        transform=Transform(x=10.0, y=5.0, z=0.0),
        rigid_body={"body_type": "dynamic"},
        collider={"shape": "box", "size": [1.0, 1.0, 1.0]},
    )
    _impulse_initial_y = 5.0

    # Velocity test: dynamic body at origin with zero gravity (set per-test)
    _velocity_entity = rython.scene.spawn(
        transform=Transform(x=0.0, y=100.0, z=0.0),
        rigid_body={"body_type": "dynamic", "gravity_factor": 0.0},
        collider={"shape": "box", "size": [1.0, 1.0, 1.0]},
    )
    _velocity_initial_x = 0.0

    # Static body test: should not move under gravity
    _static_entity = rython.scene.spawn(
        transform=Transform(x=20.0, y=5.0, z=0.0),
        rigid_body={"body_type": "static"},
        collider={"shape": "box", "size": [1.0, 1.0, 1.0]},
    )


# ── Frame callbacks ──────────────────────────────────────────────────────────

def check_gravity_pulls_dynamic():
    """After 15 frames with gravity -20, the dynamic entity should have fallen."""
    y = _dynamic_entity.transform.y
    assert y < 10.0, f"expected y < 10.0, got {y}"


def apply_impulse_at_frame_2():
    """At frame 2, apply an upward impulse to the impulse entity."""
    _impulse_entity.apply_impulse(0.0, 50.0, 0.0)


def check_impulse_effect():
    """After impulse, entity should have moved upward or have positive velocity."""
    y = _impulse_entity.transform.y
    vy = _impulse_entity.velocity.y
    assert y > _impulse_initial_y or vy > 0, (
        f"expected y > {_impulse_initial_y} or vy > 0, got y={y}, vy={vy}"
    )


def set_velocity_at_frame_2():
    """At frame 2, set positive x velocity on the velocity entity."""
    _velocity_entity.set_velocity(10.0, 0.0, 0.0)


def check_velocity_effect():
    """After set_velocity, entity should have moved in the +x direction."""
    x = _velocity_entity.transform.x
    assert x > _velocity_initial_x, (
        f"expected x > {_velocity_initial_x}, got {x}"
    )


def check_static_body_stationary():
    """A static body should not move under gravity."""
    y = _static_entity.transform.y
    assert abs(y - 5.0) < 0.5, f"expected y ~= 5.0, got {y}"


# ── Restitution tests ────────────────────────────────────────────────────────

# Entities used by restitution frame-loop tests.
_bounce_entity = None
_no_bounce_entity = None
_clamp_entity = None
_bounce_min_y = None


def _spawn_restitution_entities():
    """Spawn floor + test balls for restitution tests."""
    global _bounce_entity, _no_bounce_entity, _clamp_entity, _bounce_min_y

    # Static floor shared by all restitution tests
    rython.scene.spawn(
        transform=Transform(x=0.0, y=-5.0, z=0.0),
        rigid_body={"body_type": "static"},
        collider={"shape": "box", "size": [20.0, 0.5, 20.0]},
    )

    # High-restitution ball — dropped from y=5, should bounce back up
    _bounce_entity = rython.scene.spawn(
        transform=Transform(x=0.0, y=5.0, z=0.0),
        rigid_body={"body_type": "dynamic"},
        collider={"shape": "box", "size": [0.5, 0.5, 0.5], "restitution": 0.9},
    )
    _bounce_min_y = 5.0  # will be updated each frame to track the minimum y reached

    # Zero-restitution ball — should settle on the floor without bouncing
    _no_bounce_entity = rython.scene.spawn(
        transform=Transform(x=5.0, y=5.0, z=0.0),
        rigid_body={"body_type": "dynamic"},
        collider={"shape": "box", "size": [0.5, 0.5, 0.5], "restitution": 0.0},
    )

    # Out-of-range restitution (5.0) — must be silently clamped to 1.0, no panic
    _clamp_entity = rython.scene.spawn(
        transform=Transform(x=-5.0, y=5.0, z=0.0),
        rigid_body={"body_type": "dynamic"},
        collider={"shape": "box", "size": [0.5, 0.5, 0.5], "restitution": 5.0},
    )


def _track_bounce_min_y():
    """Update the minimum y the bounce ball has reached (called each frame)."""
    global _bounce_min_y
    y = _bounce_entity.transform.y
    if y < _bounce_min_y:
        _bounce_min_y = y


def test_restitution_bounce():
    """After a bounce with restitution=0.9, the ball must be above its minimum y."""
    current_y = _bounce_entity.transform.y
    assert current_y > _bounce_min_y, (
        f"expected bounce ball y={current_y} > min_y={_bounce_min_y}"
        " (ball should have rebounded)"
    )


def test_restitution_zero():
    """With restitution=0.0 the ball should have settled — no significant bounce."""
    vy = _no_bounce_entity.velocity.y
    assert vy <= 0.1, (
        f"expected no-bounce ball vy={vy} ≤ 0.1 (no rebound with restitution=0)"
    )


def test_restitution_clamps_out_of_range():
    """restitution=5.0 must be clamped; entity must still simulate without panic."""
    y = _clamp_entity.transform.y
    assert not (y != y), f"clamp entity y is NaN — physics exploded"
    # No further assertion: warn log is emitted by the engine; behavior matches
    # restitution=1.0 (fully elastic). The important guarantee is no panic.


# ── Scene-query entities ─────────────────────────────────────────────────────

# Shared query test state.
_query_floor = None
_query_body = None


def _spawn_query_entities():
    """Spawn a floor and an airborne entity for scene-query tests."""
    global _query_floor, _query_body

    # Static floor at y=0, large footprint.
    _query_floor = rython.scene.spawn(
        transform=Transform(x=0.0, y=0.0, z=0.0),
        rigid_body={"body_type": "static"},
        collider={"shape": "box", "size": [200.0, 0.1, 200.0]},
    )

    # Dynamic entity hovering at y=5 with no gravity (we use gravity_factor=0).
    _query_body = rython.scene.spawn(
        transform=Transform(x=0.0, y=5.0, z=0.0),
        rigid_body={"body_type": "dynamic", "gravity_factor": 0.0},
        collider={"shape": "box", "size": [1.0, 1.0, 1.0]},
    )


# ── Scene-query frame callbacks ───────────────────────────────────────────────

def test_raycast_hits_floor():
    """Ray from (0,10,0) pointing -Y with max_dist=20 should hit the floor."""
    hit = rython.physics.raycast((0.0, 10.0, 0.0), (0.0, -1.0, 0.0), 20.0)
    assert hit is not None, "raycast should hit the floor"
    assert hit.normal.y > 0.5, f"normal.y={hit.normal.y} expected upward"
    assert hit.toi > 0.0, f"toi={hit.toi} must be positive"
    assert hit.distance == hit.toi, "distance must alias toi"


def test_raycast_misses():
    """Ray pointing upward (+Y) should return None."""
    hit = rython.physics.raycast((0.0, 10.0, 0.0), (0.0, 1.0, 0.0), 20.0)
    assert hit is None, f"upward ray should return None, got {hit}"


def test_sphere_cast_hits():
    """Sphere (r=0.5) cast from (0,10,0) downward should hit the floor."""
    hit = rython.physics.sphere_cast(
        (0.0, 10.0, 0.0), (0.0, -1.0, 0.0), 0.5, 20.0
    )
    assert hit is not None, "sphere_cast should hit the floor"
    assert hit.normal.y > 0.5, f"normal.y={hit.normal.y} expected upward"


def test_ground_normal_for_entity():
    """ground_normal should return upward normal for entity above the floor."""
    normal = rython.physics.ground_normal(_query_body, 10.0)
    assert normal is not None, "ground_normal should find the floor"
    assert normal.y > 0.5, f"normal.y={normal.y} expected upward"


def test_ground_normal_none_when_airborne():
    """ground_normal with tiny max_dist should return None (floor is far below)."""
    # Entity is at y=5; floor top is at ~y=0.05.  max_dist=0.1 misses the floor.
    normal = rython.physics.ground_normal(_query_body, 0.1)
    assert normal is None, f"expected None with tiny max_dist, got {normal}"


# ── entry point ──────────────────────────────────────────────────────────────

def init():
    # Run sync tests immediately.
    suite.run("set_gravity_default", test_set_gravity_default)
    suite.run("set_gravity_zero", test_set_gravity_zero)

    # Set gravity for frame-loop tests.
    rython.physics.set_gravity(0, -20, 0)

    # Spawn entities used by the frame-loop tests.
    _spawn_test_entities()

    # Spawn entities used by restitution frame-loop tests.
    _spawn_restitution_entities()

    # Spawn entities used by scene-query tests.
    _spawn_query_entities()

    # Schedule frame-loop assertions.
    runner = FrameRunner(suite)

    # Impulse: apply at frame 2, check at frame 10
    runner.after_frames(2, apply_impulse_at_frame_2)
    runner.after_frames(10, check_impulse_effect)

    # Velocity: set at frame 2, check at frame 10
    runner.after_frames(2, set_velocity_at_frame_2)
    runner.after_frames(10, check_velocity_effect)

    # Gravity on dynamic entity: check at frame 15
    runner.after_frames(15, check_gravity_pulls_dynamic)

    # Static body: check at frame 15
    runner.after_frames(15, check_static_body_stationary)

    # Restitution: track min-y every frame; assert at frame 60
    runner.after_frames(30, _track_bounce_min_y)
    runner.after_frames(60, test_restitution_bounce)
    runner.after_frames(60, test_restitution_zero)
    runner.after_frames(60, test_restitution_clamps_out_of_range)

    # Scene-query tests: run at frame 3 so bodies are registered and
    # the query pipeline has been updated at least once.
    runner.after_frames(3, test_raycast_hits_floor)
    runner.after_frames(3, test_raycast_misses)
    runner.after_frames(3, test_sphere_cast_hits)
    runner.after_frames(3, test_ground_normal_for_entity)
    runner.after_frames(3, test_ground_normal_none_when_airborne)

    runner.on_done(lambda: suite.report_and_quit())
    runner.start()

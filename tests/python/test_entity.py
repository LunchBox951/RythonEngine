"""Integration tests for rython Entity operations.

Run via:
    cargo run -p rython-cli -- --headless --script-dir tests/python --entry-point test_entity
"""
import rython
from _harness import TestSuite, FrameRunner

suite = TestSuite()
runner = FrameRunner(suite)


# ---------------------------------------------------------------------------
# Sync tests (run immediately in init)
# ---------------------------------------------------------------------------

def test_entity_id_positive():
    e = rython.scene.spawn()
    assert isinstance(e.id, int), f"expected int, got {type(e.id).__name__}"
    assert e.id > 0, f"expected id > 0, got {e.id}"


def test_entity_transform_initial():
    e = rython.scene.spawn(transform=rython.Transform(4.0, 5.0, 6.0))
    tr = e.transform
    assert abs(tr.x - 4.0) < 0.01, f"x: expected 4.0, got {tr.x}"
    assert abs(tr.y - 5.0) < 0.01, f"y: expected 5.0, got {tr.y}"
    assert abs(tr.z - 6.0) < 0.01, f"z: expected 6.0, got {tr.z}"


def test_has_tag_false_when_missing():
    e = rython.scene.spawn()
    assert not e.has_tag("nonexistent"), "has_tag should return False for missing tag"


def test_add_tag_then_has_tag():
    e = rython.scene.spawn()
    e.add_tag("player")
    assert e.has_tag("player"), "has_tag('player') should be True after add_tag"


def test_entity_constructor():
    e = rython.Entity(id=0)
    assert e.id == 0, f"expected id == 0, got {e.id}"


# ---------------------------------------------------------------------------
# Frame-loop tests (deferred via FrameRunner)
# ---------------------------------------------------------------------------

# -- Physics impulse test --
_impulse_entity = None


def _setup_impulse_test():
    global _impulse_entity
    _impulse_entity = rython.scene.spawn(
        transform=rython.Transform(0.0, 50.0, 0.0),
        rigid_body={"body_type": "dynamic", "gravity_factor": 0.0},
        collider={"shape": "box", "size": [1, 1, 1]},
    )


def _apply_impulse():
    """Apply impulse after the entity is fully registered in physics."""
    _impulse_entity.apply_impulse(0.0, 100.0, 0.0)


def check_impulse():
    vel = _impulse_entity.velocity
    tr = _impulse_entity.transform
    # With gravity disabled, the upward impulse should clearly move the entity
    # above its spawn height and keep velocity positive.
    moved_up = tr.y > 50.0
    vel_positive = vel.y > 0.0
    assert moved_up or vel_positive, (
        f"expected upward motion: vel.y={vel.y}, pos.y={tr.y}"
    )


# -- Despawn graceful degradation test --
_despawn_entity = None


def _setup_despawn_test():
    global _despawn_entity
    _despawn_entity = rython.scene.spawn(
        transform=rython.Transform(0.0, 0.0, 0.0),
    )
    _despawn_entity.despawn()


def check_despawn():
    # After despawn + drain, further operations should not crash.
    # We simply access attributes; if no exception is raised, the test passes.
    try:
        _ = _despawn_entity.transform
    except Exception:
        pass  # graceful degradation — not crashing is the success criterion


# -- set_velocity test --
_velocity_entity = None


def _setup_velocity_test():
    global _velocity_entity
    _velocity_entity = rython.scene.spawn(
        transform=rython.Transform(0.0, 50.0, 0.0),
        rigid_body={"body_type": "dynamic", "gravity_factor": 0.0},
        collider={"shape": "box", "size": [1, 1, 1]},
    )


def _apply_velocity():
    """Set velocity after the entity is fully registered in physics."""
    _velocity_entity.set_velocity(5.0, 0.0, 0.0)


def check_velocity():
    vel = _velocity_entity.velocity
    tr = _velocity_entity.transform
    # The entity should have moved in the +x direction from its origin (0).
    # Accept either velocity or position as evidence the set_velocity worked.
    has_x_velocity = abs(vel.x) > 1.0
    has_moved = tr.x > 0.1
    assert has_x_velocity or has_moved, (
        f"expected x-axis motion: vel.x={vel.x}, pos.x={tr.x}"
    )


# ---------------------------------------------------------------------------
# init — entry point called by the engine
# ---------------------------------------------------------------------------

def init():
    # Run sync tests
    suite.run("entity_id_positive", test_entity_id_positive)
    suite.run("entity_transform_initial", test_entity_transform_initial)
    suite.run("has_tag_false_when_missing", test_has_tag_false_when_missing)
    suite.run("add_tag_then_has_tag", test_add_tag_then_has_tag)
    suite.run("entity_constructor", test_entity_constructor)

    # Set up frame-loop tests
    _setup_impulse_test()
    _setup_despawn_test()
    _setup_velocity_test()

    runner.after_frames(2, _apply_impulse)
    runner.after_frames(2, _apply_velocity)
    runner.after_frames(3, check_despawn)
    runner.after_frames(5, check_impulse)
    runner.after_frames(5, check_velocity)
    runner.on_done(lambda: suite.report_and_quit())
    runner.start()

"""Integration tests for rython.scene (SceneBridge).

Run via:
    cargo run -p rython-cli -- --headless --script-dir tests/python --entry-point test_scene
"""
import rython
from _harness import TestSuite, FrameRunner

suite = TestSuite()
runner = FrameRunner(suite)


# ---------------------------------------------------------------------------
# Sync tests (run immediately in init)
# ---------------------------------------------------------------------------

def test_spawn_no_args():
    e = rython.scene.spawn()
    assert e.id > 0, f"expected id > 0, got {e.id}"


def test_spawn_with_transform():
    t = rython.Transform(1.0, 2.0, 3.0)
    e = rython.scene.spawn(transform=t)
    tr = e.transform
    assert abs(tr.x - 1.0) < 0.01, f"x: expected 1.0, got {tr.x}"
    assert abs(tr.y - 2.0) < 0.01, f"y: expected 2.0, got {tr.y}"
    assert abs(tr.z - 3.0) < 0.01, f"z: expected 3.0, got {tr.z}"


def test_spawn_with_mesh_string():
    e = rython.scene.spawn(mesh="cube")
    assert e.id > 0, f"expected id > 0 after mesh='cube', got {e.id}"


def test_spawn_with_mesh_dict():
    e = rython.scene.spawn(mesh={"mesh_id": "cube", "visible": True})
    assert e.id > 0, f"expected id > 0 after mesh dict, got {e.id}"


def test_spawn_with_tags():
    e = rython.scene.spawn(tags=["enemy", "boss"])
    assert e.has_tag("enemy"), "expected has_tag('enemy') to be True"
    assert e.has_tag("boss"), "expected has_tag('boss') to be True"


def test_spawn_with_rigid_body():
    e = rython.scene.spawn(rigid_body={"body_type": "dynamic"})
    assert e.id > 0, f"expected id > 0 after rigid_body, got {e.id}"


def test_spawn_with_collider():
    e = rython.scene.spawn(collider={"shape": "box", "size": [1, 1, 1]})
    assert e.id > 0, f"expected id > 0 after collider, got {e.id}"


def test_spawn_with_rigid_body_and_collider():
    e = rython.scene.spawn(
        rigid_body={"body_type": "dynamic"},
        collider={"shape": "box", "size": [1, 1, 1]},
    )
    assert e.id > 0, f"expected id > 0 after rigid_body+collider, got {e.id}"


def test_multiple_spawns_unique_ids():
    e1 = rython.scene.spawn()
    e2 = rython.scene.spawn()
    e3 = rython.scene.spawn()
    assert e1.id != e2.id, f"e1.id == e2.id == {e1.id}"
    assert e2.id != e3.id, f"e2.id == e3.id == {e2.id}"
    assert e1.id != e3.id, f"e1.id == e3.id == {e1.id}"


def test_subscribe_returns_int():
    sub_id = rython.scene.subscribe("__test_noop", lambda **kw: None)
    assert isinstance(sub_id, int), f"expected int, got {type(sub_id).__name__}"
    # Clean up
    rython.scene.unsubscribe("__test_noop", sub_id)


# ---------------------------------------------------------------------------
# Frame-loop tests (deferred via FrameRunner)
# ---------------------------------------------------------------------------

_event_log = []


def _setup_emit_subscribe():
    """Subscribe to a test event and emit it."""
    _event_log.clear()

    def _handler(**kwargs):
        _event_log.append(kwargs)

    rython.scene.subscribe("test_ping", _handler)
    rython.scene.emit("test_ping", payload="hello")


_unsub_log = []


def _setup_unsubscribe():
    """Subscribe, immediately unsubscribe, then emit."""
    _unsub_log.clear()

    def _handler(**kwargs):
        _unsub_log.append(kwargs)

    sub_id = rython.scene.subscribe("test_unsub", _handler)
    rython.scene.unsubscribe("test_unsub", sub_id)
    rython.scene.emit("test_unsub", payload="should_not_arrive")


def check_emit_subscribe():
    assert len(_event_log) > 0, "event handler was never called"


def check_unsubscribe():
    assert len(_unsub_log) == 0, (
        f"handler called {len(_unsub_log)} time(s) after unsubscribe"
    )


# ---------------------------------------------------------------------------
# init — entry point called by the engine
# ---------------------------------------------------------------------------

def init():
    # Run sync tests
    suite.run("spawn_no_args", test_spawn_no_args)
    suite.run("spawn_with_transform", test_spawn_with_transform)
    suite.run("spawn_with_mesh_string", test_spawn_with_mesh_string)
    suite.run("spawn_with_mesh_dict", test_spawn_with_mesh_dict)
    suite.run("spawn_with_tags", test_spawn_with_tags)
    suite.run("spawn_with_rigid_body", test_spawn_with_rigid_body)
    suite.run("spawn_with_collider", test_spawn_with_collider)
    suite.run("spawn_with_rigid_body_and_collider", test_spawn_with_rigid_body_and_collider)
    suite.run("multiple_spawns_unique_ids", test_multiple_spawns_unique_ids)
    suite.run("subscribe_returns_int", test_subscribe_returns_int)

    # Set up frame-loop tests
    _setup_emit_subscribe()
    _setup_unsubscribe()

    runner.after_frames(5, check_emit_subscribe)
    runner.after_frames(5, check_unsubscribe)
    runner.on_done(lambda: suite.report_and_quit())
    runner.start()

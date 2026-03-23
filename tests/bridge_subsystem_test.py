"""
Bridge subsystem headless test.

Runs via:
    cargo run -p rython-cli -- --script-dir tests --entry-point bridge_subsystem_test --headless

The engine calls init() once; all assertions run there and the engine is told
to quit at the end.  Exit code is inferred from printed output — a CI wrapper
can grep for "FAILED" in stdout.

Bridges exercised:
  - rython.scene    (SceneBridge)
  - rython.camera   (CameraPy)
  - rython.input    (InputBridge)
  - rython.audio    (AudioBridge)
  - rython.resources (ResourcesBridge — skipped gracefully if Phase 6 pending)
"""
import math
import rython

# ── Result counters ───────────────────────────────────────────────────────────

_pass = 0
_fail = 0


def ok(label: str) -> None:
    global _pass
    _pass += 1
    print(f"  [PASS] {label}")


def fail(label: str, reason: str = "") -> None:
    global _fail
    _fail += 1
    msg = f"  [FAIL] {label}"
    if reason:
        msg += f": {reason}"
    print(msg)


def expect(cond: bool, label: str, reason: str = "") -> None:
    if cond:
        ok(label)
    else:
        fail(label, reason or f"condition was False")


def expect_eq(actual, expected, label: str) -> None:
    if actual == expected:
        ok(label)
    else:
        fail(label, f"expected {expected!r}, got {actual!r}")


def expect_close(actual: float, expected: float, label: str, tol: float = 1e-5) -> None:
    if abs(actual - expected) < tol:
        ok(label)
    else:
        fail(label, f"expected ~{expected}, got {actual}")


def no_raise(fn, label: str) -> bool:
    """Run fn(); ok if no exception, fail otherwise.  Returns True on success."""
    try:
        fn()
        ok(label)
        return True
    except Exception as exc:
        fail(label, str(exc))
        return False


# ── SceneBridge ───────────────────────────────────────────────────────────────

def test_scene() -> None:
    print("[ SceneBridge ]")

    # Bare spawn returns a valid entity.
    e0 = rython.scene.spawn()
    expect(e0.id > 0, "spawn() → entity with positive id")

    # Spawn with transform; read it back.
    e1 = rython.scene.spawn(
        transform=rython.Transform(x=1.0, y=2.0, z=3.0, scale=2.0)
    )
    t = e1.transform
    expect_close(t.x,     1.0, "spawn(transform) x=1")
    expect_close(t.y,     2.0, "spawn(transform) y=2")
    expect_close(t.z,     3.0, "spawn(transform) z=3")
    expect_close(t.scale, 2.0, "spawn(transform) scale=2")

    # Tags are queryable immediately after spawn.
    e2 = rython.scene.spawn(tags=["enemy", "active"])
    expect(    e2.has_tag("enemy"),  "has_tag('enemy') after spawn")
    expect(    e2.has_tag("active"), "has_tag('active') after spawn")
    expect(not e2.has_tag("player"), "not has_tag('player') after spawn")

    # add_tag at runtime.
    e2.add_tag("boss")
    expect(e2.has_tag("boss"), "has_tag('boss') after add_tag")

    # Spawn with mesh (string shorthand).
    e3 = rython.scene.spawn(mesh="cube")
    expect(e3.id > 0, "spawn(mesh='cube') → entity")

    # Spawn with mesh dict.
    e4 = rython.scene.spawn(
        mesh={"mesh_id": "sphere", "texture_id": "rock", "visible": True}
    )
    expect(e4.id > 0, "spawn(mesh=dict) → entity")

    # Combined: transform + mesh + tags in one call.
    e5 = rython.scene.spawn(
        transform=rython.Transform(x=0.0, y=0.0, z=0.0),
        mesh="cube",
        tags=["combined"],
    )
    expect(e5.has_tag("combined"), "combined spawn preserves tags")

    # All entity ids are unique.
    ids = {e0.id, e1.id, e2.id, e3.id, e4.id, e5.id}
    expect_eq(len(ids), 6, "all spawned entity ids are unique")

    # emit + subscribe round-trip.
    received: list = []

    def on_test_event(**kwargs):
        received.append(kwargs)

    rython.scene.subscribe("bridge_test_event", on_test_event)
    rython.scene.emit("bridge_test_event", score=99, tag="ok")
    expect(len(received) == 1, "subscribe handler called exactly once after emit")
    if received:
        expect_eq(received[0].get("score"), 99, "emit payload 'score'=99")

    # A second emit triggers the handler again.
    rython.scene.emit("bridge_test_event", score=0)
    expect(len(received) == 2, "second emit triggers handler again")

    # Emit to an unknown event is a no-op (no crash).
    no_raise(lambda: rython.scene.emit("no_subscriber_event"), "emit to unsubscribed event")

    # despawn is a no-op for an existing entity (no crash).
    no_raise(e4.despawn, "despawn() no exception")

    # attach_script registers a class and adds ScriptComponent.
    class DummyScript:
        pass

    no_raise(
        lambda: rython.scene.attach_script(e3, DummyScript),
        "attach_script() no exception",
    )

    # repr
    expect("rython.scene" in repr(rython.scene), "scene __repr__")


# ── CameraPy ──────────────────────────────────────────────────────────────────

def test_camera() -> None:
    print("[ CameraPy ]")

    cam = rython.camera

    # set_position stores exactly the given values.
    cam.set_position(5.0, 3.0, -8.0)
    expect_close(cam.pos_x, 5.0,  "set_position pos_x=5")
    expect_close(cam.pos_y, 3.0,  "set_position pos_y=3")
    expect_close(cam.pos_z, -8.0, "set_position pos_z=-8")

    # set_look_at stores exact target coords.
    cam.set_position(0.0, 5.0, -10.0)
    cam.set_look_at(0.0, 0.0, 0.0)
    expect_close(cam.target_x, 0.0, "set_look_at target_x=0")
    expect_close(cam.target_y, 0.0, "set_look_at target_y=0")
    expect_close(cam.target_z, 0.0, "set_look_at target_z=0")

    # Looking downward from (0,5,-10) toward (0,0,0): pitch should be negative.
    expect(cam.rot_pitch < 0.0, "looking down → rot_pitch < 0")

    # set_look_at at same height: pitch should be ~0.
    cam.set_position(0.0, 0.0, -10.0)
    cam.set_look_at(0.0, 0.0, 0.0)
    expect(abs(cam.rot_pitch) < 0.01, "horizontal look → |rot_pitch| < 0.01")

    # set_rotation stores pitch/yaw/roll and derives target.
    cam.set_position(0.0, 0.0, 0.0)
    cam.set_rotation(0.0, 0.0, 0.0)
    expect_close(cam.rot_pitch, 0.0, "set_rotation pitch=0")
    expect_close(cam.rot_yaw,   0.0, "set_rotation yaw=0")
    expect_close(cam.rot_roll,  0.0, "set_rotation roll=0")
    # yaw=0, pitch=0 → forward is +Z; target should be at z=1
    expect_close(cam.target_z, 1.0, "set_rotation(0,0,0) → target_z=1")

    # Non-zero pitch: looking straight up (pitch = π/2).
    cam.set_position(0.0, 0.0, 0.0)
    cam.set_rotation(math.pi / 2, 0.0, 0.0)
    # pitch.sin() = 1 → target_y = pos_y - 1 = -1
    expect_close(cam.target_y, -1.0, "pitch=π/2 → target_y=-1", tol=1e-4)

    # repr
    expect("Camera" in repr(cam), "camera __repr__ contains 'Camera'")


# ── InputBridge ───────────────────────────────────────────────────────────────

def test_input() -> None:
    print("[ InputBridge ]")

    # Headless mode: no PlayerController tick → snapshot is zeroed.
    expect_close(rython.input.axis("move_x"),  0.0, "headless axis('move_x') = 0.0")
    expect_close(rython.input.axis("move_y"),  0.0, "headless axis('move_y') = 0.0")
    expect_close(rython.input.axis("look_x"),  0.0, "headless axis('look_x') = 0.0")
    expect_close(rython.input.axis("unknown"), 0.0, "headless axis(unknown)  = 0.0")

    expect(not rython.input.pressed("fire"),    "headless pressed('fire')  = False")
    expect(not rython.input.held("move_x"),     "headless held('move_x')   = False")
    expect(not rython.input.released("jump"),   "headless released('jump') = False")
    expect(not rython.input.pressed("unknown"), "headless pressed(unknown) = False")

    # repr
    expect("rython.input" in repr(rython.input), "input __repr__")


# ── AudioBridge ───────────────────────────────────────────────────────────────

def test_audio() -> None:
    print("[ AudioBridge ]")

    # Volume operations are pure-logic: they update config even without hardware.
    no_raise(lambda: rython.audio.set_master_volume(0.5),       "set_master_volume(0.5)")
    no_raise(lambda: rython.audio.set_master_volume(0.0),       "set_master_volume(0.0) — mute")
    no_raise(lambda: rython.audio.set_master_volume(1.0),       "set_master_volume(1.0) — max")
    no_raise(lambda: rython.audio.set_volume("sfx",      0.8),  "set_volume('sfx', 0.8)")
    no_raise(lambda: rython.audio.set_volume("music",    0.6),  "set_volume('music', 0.6)")
    no_raise(lambda: rython.audio.set_volume("dialogue", 1.0),  "set_volume('dialogue', 1.0)")
    no_raise(lambda: rython.audio.set_volume("ambient",  0.4),  "set_volume('ambient', 0.4)")

    # stop_category with no active sounds is a no-op.
    no_raise(lambda: rython.audio.stop_category("sfx"),    "stop_category('sfx') — empty")
    no_raise(lambda: rython.audio.stop_category("music"),  "stop_category('music') — empty")

    # Unknown category must raise.
    try:
        rython.audio.set_volume("invalid_cat", 0.5)
        fail("set_volume('invalid_cat') should raise")
    except Exception:
        ok("set_volume('invalid_cat') raises on unknown category")

    # play() may fail in headless (no audio hardware); skip gracefully.
    try:
        handle = rython.audio.play("test.ogg", "sfx", False)
        expect(isinstance(handle, int), "play() returns int handle")
        no_raise(lambda: rython.audio.stop(handle), "stop(handle)")
    except Exception:
        ok("play() skipped gracefully (no audio hardware)")

    # repr
    expect("rython.audio" in repr(rython.audio), "audio __repr__")


# ── ResourcesBridge ───────────────────────────────────────────────────────────

def _check_handle(handle, label: str) -> None:
    """Verify an AssetHandlePy exposes the required boolean properties."""
    is_ready   = handle.is_ready
    is_pending = handle.is_pending
    is_failed  = handle.is_failed

    expect(isinstance(is_ready,   bool), f"{label}: is_ready is bool")
    expect(isinstance(is_pending, bool), f"{label}: is_pending is bool")
    expect(isinstance(is_failed,  bool), f"{label}: is_failed is bool")

    err = handle.error
    expect(err is None or isinstance(err, str), f"{label}: error is None or str")
    if is_failed:
        expect(err is not None, f"{label}: error is populated when is_failed=True")


def test_resources() -> None:
    print("[ ResourcesBridge ]")

    # Detect whether Phase 6 has been merged (stub raises ValueError on any attr).
    try:
        used = rython.resources.memory_used_mb()
    except (ValueError, AttributeError):
        print("  [SKIP] rython.resources is still a stub — Phase 6 not yet merged")
        return

    budget = rython.resources.memory_budget_mb()
    expect(isinstance(used,   float), f"memory_used_mb() is float (got {type(used).__name__})")
    expect(isinstance(budget, float), f"memory_budget_mb() is float (got {type(budget).__name__})")
    expect(used   >= 0.0, f"memory_used_mb() >= 0 (got {used})")
    expect(budget >  0.0, f"memory_budget_mb() > 0 (got {budget})")

    # load_* calls return AssetHandlePy instances.
    # Nonexistent paths produce a handle in pending or failed state.
    for fn_name, path in [
        ("load_image",       "nonexistent.png"),
        ("load_mesh",        "nonexistent.gltf"),
        ("load_sound",       "nonexistent.ogg"),
        ("load_font",        "nonexistent.ttf"),
        ("load_spritesheet", "nonexistent_sheet.png"),
    ]:
        try:
            handle = getattr(rython.resources, fn_name)(path)
            _check_handle(handle, fn_name)
        except Exception as exc:
            fail(f"{fn_name}('{path}') no exception", str(exc))

    # repr
    expect("resources" in repr(rython.resources).lower(), "resources __repr__")


# ── Entry point ───────────────────────────────────────────────────────────────

def init() -> None:
    """Called once by the engine after all modules have loaded."""
    print()
    print("=" * 60)
    print("  bridge_subsystem_test  (headless)")
    print("=" * 60)

    test_scene()
    print()
    test_camera()
    print()
    test_input()
    print()
    test_audio()
    print()
    test_resources()

    total = _pass + _fail
    print()
    print("=" * 60)
    print(f"  Results: {_pass}/{total} passed, {_fail} FAILED")
    print("=" * 60)
    if _fail > 0:
        print("BRIDGE TEST FAILED")
    else:
        print("BRIDGE TEST PASSED")
    print()

    rython.engine.request_quit()

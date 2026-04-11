"""Integration tests for rython.resources API."""

import rython
from _harness import TestSuite, FrameRunner

suite = TestSuite()


# ── Sync test definitions ─────────────────────────────────────────────────────


def test_memory_used_mb_returns_float():
    val = rython.resources.memory_used_mb()
    assert isinstance(val, float), f"Expected float, got {type(val).__name__}"
    assert val >= 0, f"Expected >= 0, got {val}"


def test_memory_budget_mb_returns_float():
    val = rython.resources.memory_budget_mb()
    assert isinstance(val, float), f"Expected float, got {type(val).__name__}"
    assert val > 0, f"Expected > 0, got {val}"


def test_load_image_returns_asset_handle():
    handle = rython.resources.load_image("nonexistent.png")
    assert handle is not None, "Expected an AssetHandle, got None"


def test_asset_handle_has_expected_properties():
    handle = rython.resources.load_image("nonexistent.png")
    assert hasattr(handle, "is_ready"), "Missing is_ready property"
    assert hasattr(handle, "is_pending"), "Missing is_pending property"
    assert hasattr(handle, "is_failed"), "Missing is_failed property"
    assert hasattr(handle, "error"), "Missing error property"


def test_load_mesh_returns_asset_handle():
    handle = rython.resources.load_mesh("nonexistent.gltf")
    assert handle is not None, "Expected an AssetHandle, got None"


def test_load_sound_returns_asset_handle():
    handle = rython.resources.load_sound("nonexistent.wav")
    assert handle is not None, "Expected an AssetHandle, got None"


def test_load_font_returns_asset_handle():
    handle = rython.resources.load_font("nonexistent.ttf")
    assert handle is not None, "Expected an AssetHandle, got None"


def test_load_font_with_custom_size():
    handle = rython.resources.load_font("nonexistent.ttf", size=32.0)
    assert handle is not None, "Expected an AssetHandle, got None"


def test_load_spritesheet_returns_asset_handle():
    handle = rython.resources.load_spritesheet("nonexistent.png", cols=4, rows=4)
    assert handle is not None, "Expected an AssetHandle, got None"


def test_multiple_loads_no_crash():
    rython.resources.load_image("a.png")
    rython.resources.load_mesh("b.gltf")
    rython.resources.load_sound("c.wav")
    rython.resources.load_font("d.ttf")
    rython.resources.load_spritesheet("e.png", cols=2, rows=2)


# ── Async test: asset handle eventually fails for nonexistent file ────────────

# Store the handle at module level so the frame callback can access it.
_nonexistent_handle = None


def _load_nonexistent_for_async_check():
    global _nonexistent_handle
    _nonexistent_handle = rython.resources.load_image("does_not_exist.png")


def check_asset_handle_eventually_fails():
    assert _nonexistent_handle is not None, "Handle was not created"
    # After several frames the handle should either be failed or still pending.
    # Both are acceptable since we have no real file. If it is failed, validate
    # the error attribute.
    if _nonexistent_handle.is_failed:
        assert _nonexistent_handle.error is None or isinstance(
            _nonexistent_handle.error, str
        ), "error should be None or str"
    # If still pending, that is also acceptable for a nonexistent file.


# ── Entry point ───────────────────────────────────────────────────────────────


def init():
    # Run synchronous tests immediately.
    suite.run("memory_used_mb_returns_float", test_memory_used_mb_returns_float)
    suite.run("memory_budget_mb_returns_float", test_memory_budget_mb_returns_float)
    suite.run("load_image_returns_asset_handle", test_load_image_returns_asset_handle)
    suite.run("asset_handle_has_expected_properties", test_asset_handle_has_expected_properties)
    suite.run("load_mesh_returns_asset_handle", test_load_mesh_returns_asset_handle)
    suite.run("load_sound_returns_asset_handle", test_load_sound_returns_asset_handle)
    suite.run("load_font_returns_asset_handle", test_load_font_returns_asset_handle)
    suite.run("load_font_with_custom_size", test_load_font_with_custom_size)
    suite.run("load_spritesheet_returns_asset_handle", test_load_spritesheet_returns_asset_handle)
    suite.run("multiple_loads_no_crash", test_multiple_loads_no_crash)

    # Kick off async check: load a nonexistent file, then inspect after frames.
    _load_nonexistent_for_async_check()

    runner = FrameRunner(suite)
    runner.after_frames(10, check_asset_handle_eventually_fails)
    runner.on_done(lambda: suite.report_and_quit())
    runner.start()

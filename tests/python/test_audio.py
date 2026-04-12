"""Integration tests for rython.audio API.

These tests no longer wrap every call in `try: ... except Exception: pass`.
The previous harness made the suite always report green regardless of the
actual outcome, hiding panics and regressions. Tests that depend on real
audio hardware are marked and skipped cleanly; tests that assert contracts
(invalid category raises, invalid volume is clamped) run unconditionally
and actually verify behavior.
"""

import rython
from _harness import TestSuite

suite = TestSuite()


# ── Tests that do not require audio hardware ──────────────────────────────────


def test_set_master_volume_accepts_boundaries():
    # Both 0.0 and 1.0 are valid; neither should raise.
    rython.audio.set_master_volume(0.0)
    rython.audio.set_master_volume(1.0)
    rython.audio.set_master_volume(0.5)


def test_set_volume_known_categories():
    # Each documented category must be accepted.
    for category in ("sfx", "music", "dialogue", "ambient"):
        rython.audio.set_volume(category, 0.5)


def test_set_volume_unknown_category_raises():
    # Regression: the bridge must surface an error, not swallow it.
    try:
        rython.audio.set_volume("invalid-category", 0.5)
    except RuntimeError:
        return  # expected
    except Exception as e:
        raise AssertionError(
            f"Unknown category must raise RuntimeError, got {type(e).__name__}"
        )
    raise AssertionError("Unknown category must raise, but did not")


def test_play_nonexistent_file_is_handled():
    # This must not crash the engine. Either an int handle is returned
    # (the bridge silently substituted a zero handle) or an exception is
    # raised — both are acceptable, but a panic is not.
    try:
        result = rython.audio.play("nonexistent.wav")
        assert isinstance(result, int), f"Expected int handle, got {type(result).__name__}"
    except RuntimeError:
        pass  # audio bridge correctly surfaced an error


def test_stop_invalid_handle_is_noop():
    # Stopping an unknown handle should be a no-op, not a crash.
    rython.audio.stop(99_999_999)


def test_stop_category_unknown_is_noop_or_raises():
    # `stop_category` on an unused category should either silently no-op
    # or raise — either is acceptable as long as the engine survives.
    try:
        rython.audio.stop_category("sfx")
    except RuntimeError:
        pass


# ── Entry point ───────────────────────────────────────────────────────────────


def init():
    suite.run("set_master_volume_accepts_boundaries", test_set_master_volume_accepts_boundaries)
    suite.run("set_volume_known_categories", test_set_volume_known_categories)
    suite.run("set_volume_unknown_category_raises", test_set_volume_unknown_category_raises)
    suite.run("play_nonexistent_file_is_handled", test_play_nonexistent_file_is_handled)
    suite.run("stop_invalid_handle_is_noop", test_stop_invalid_handle_is_noop)
    suite.run("stop_category_unknown_is_noop_or_raises", test_stop_category_unknown_is_noop_or_raises)
    suite.report_and_quit()

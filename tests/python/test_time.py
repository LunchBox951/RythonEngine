"""Integration tests for rython.time APIs.

Tests that rython.time.elapsed behaves as documented: returns a float,
is non-negative, and increases monotonically over frames.
"""

import rython
from _harness import TestSuite, FrameRunner

suite = TestSuite()
runner = FrameRunner(suite)

# ---------------------------------------------------------------------------
# Mutable state for tracking time values across frames
# ---------------------------------------------------------------------------
_elapsed_frame_1 = [None]
_elapsed_frame_20 = [None]
_prev_elapsed = [None]
_monotonic_ok = [True]


def init():
    # -- elapsed is a float (sync check in init) --
    suite.run(
        "test_elapsed_is_float",
        lambda: _assert_is_float(rython.time.elapsed),
    )

    # -- elapsed is non-negative (sync check in init) --
    suite.run(
        "test_elapsed_non_negative",
        lambda: _assert_non_negative(rython.time.elapsed),
    )

    # -- elapsed increases over frames --
    def record_frame_1():
        _elapsed_frame_1[0] = rython.time.elapsed

    def check_elapsed_increases():
        _elapsed_frame_20[0] = rython.time.elapsed
        assert _elapsed_frame_20[0] > _elapsed_frame_1[0], (
            f"elapsed did not increase: frame1={_elapsed_frame_1[0]}, "
            f"frame20={_elapsed_frame_20[0]}"
        )

    runner.after_frames(1, record_frame_1)
    runner.after_frames(20, check_elapsed_increases)

    # -- elapsed increases monotonically --
    def _track_monotonic():
        now = rython.time.elapsed
        if _prev_elapsed[0] is not None and now < _prev_elapsed[0]:
            _monotonic_ok[0] = False
        _prev_elapsed[0] = now

    rython.scheduler.register_recurring(_track_monotonic)

    def check_monotonic():
        assert _monotonic_ok[0], "elapsed decreased between frames"

    runner.after_frames(15, check_monotonic)

    # -- finish --
    runner.on_done(lambda: suite.report_and_quit())
    runner.start()


def _assert_is_float(value):
    assert isinstance(value, float), f"expected float, got {type(value).__name__}"


def _assert_non_negative(value):
    assert value >= 0, f"expected non-negative, got {value}"

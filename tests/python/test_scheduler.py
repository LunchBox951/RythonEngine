"""Integration tests for rython.scheduler core APIs.

Tests register_recurring, on_timer, and on_event through the engine's
Python bridge, using FrameRunner to assert across multiple frames.
"""

import rython
from _harness import TestSuite, FrameRunner

suite = TestSuite()
runner = FrameRunner(suite)

# ---------------------------------------------------------------------------
# Mutable state for tracking callback invocations
# ---------------------------------------------------------------------------
_recurring_count = [0]
_timer_fired = [False]
_timer_oneshot_count = [0]
_event_fired = [False]
_event_oneshot_count = [0]


def init():
    # -- register_recurring fires each frame --
    def _recurring_cb():
        _recurring_count[0] += 1

    rython.scheduler.register_recurring(_recurring_cb)

    def check_recurring():
        assert _recurring_count[0] >= 10, (
            f"expected recurring count >= 10, got {_recurring_count[0]}"
        )

    runner.after_frames(10, check_recurring)

    # -- on_timer fires after delay --
    def _timer_cb():
        _timer_fired[0] = True

    rython.scheduler.on_timer(0.1, _timer_cb)

    def check_timer_fired():
        assert _timer_fired[0], "on_timer callback did not fire within 20 frames"

    runner.after_frames(20, check_timer_fired)

    # -- on_timer is one-shot --
    def _timer_oneshot_cb():
        _timer_oneshot_count[0] += 1

    rython.scheduler.on_timer(0.05, _timer_oneshot_cb)

    def check_timer_oneshot():
        assert _timer_oneshot_count[0] == 1, (
            f"expected timer to fire exactly once, got {_timer_oneshot_count[0]}"
        )

    runner.after_frames(30, check_timer_oneshot)

    # -- on_event fires on emit --
    def _event_cb(**kwargs):
        _event_fired[0] = True

    rython.scheduler.on_event("test_evt", _event_cb)

    def emit_test_evt():
        rython.scene.emit("test_evt")

    runner.after_frames(3, emit_test_evt)

    def check_event_fired():
        assert _event_fired[0], "on_event callback did not fire after emit"

    runner.after_frames(10, check_event_fired)

    # -- on_event is one-shot --
    def _event_oneshot_cb(**kwargs):
        _event_oneshot_count[0] += 1

    rython.scheduler.on_event("test_evt2", _event_oneshot_cb)

    def emit_test_evt2_first():
        rython.scene.emit("test_evt2")

    def emit_test_evt2_second():
        rython.scene.emit("test_evt2")

    runner.after_frames(3, emit_test_evt2_first)
    runner.after_frames(5, emit_test_evt2_second)

    def check_event_oneshot():
        assert _event_oneshot_count[0] == 1, (
            f"expected event to fire exactly once, got {_event_oneshot_count[0]}"
        )

    runner.after_frames(10, check_event_oneshot)

    # -- finish --
    runner.on_done(lambda: suite.report_and_quit())
    runner.start()

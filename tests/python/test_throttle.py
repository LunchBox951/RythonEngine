"""Integration tests for the rython.throttle decorator.

Tests that @rython.throttle(hz=N) correctly limits call frequency and
rejects invalid hz values.
"""

import rython
from _harness import TestSuite, FrameRunner

suite = TestSuite()
runner = FrameRunner(suite)

# ---------------------------------------------------------------------------
# Mutable state for tracking throttled call counts
# ---------------------------------------------------------------------------
_throttle_call_count = [0]
_throttle_return_values = []


@rython.throttle(hz=10)
def _limited_fn():
    """Throttled to 10 calls/sec (every 0.1s)."""
    _throttle_call_count[0] += 1
    return "executed"


@rython.throttle(hz=10)
def _limited_fn_return():
    """Throttled function used to check return values."""
    return "value"


def init():
    # -- hz=0 raises ValueError (sync) --
    def test_hz_zero():
        try:
            rython.throttle(hz=0)
            assert False, "expected ValueError for hz=0"
        except ValueError:
            pass

    suite.run("test_hz_zero", test_hz_zero)

    # -- hz<0 raises ValueError (sync) --
    def test_hz_negative():
        try:
            rython.throttle(hz=-5)
            assert False, "expected ValueError for hz=-5"
        except ValueError:
            pass

    suite.run("test_hz_negative", test_hz_negative)

    # -- throttled function limits calls --
    def _call_limited_fn():
        _limited_fn()

    rython.scheduler.register_recurring(_call_limited_fn)

    def check_throttle_limits():
        count = _throttle_call_count[0]
        assert count < 30, (
            f"expected fewer than 30 calls at 10hz over ~480ms, got {count}"
        )
        assert count > 2, (
            f"expected more than 2 calls at 10hz over ~480ms, got {count}"
        )

    runner.after_frames(30, check_throttle_limits)

    # -- throttled function returns None when skipped --
    def _collect_returns():
        result = _limited_fn_return()
        _throttle_return_values.append(result)

    rython.scheduler.register_recurring(_collect_returns)

    def check_throttle_returns():
        none_count = sum(1 for v in _throttle_return_values if v is None)
        value_count = sum(1 for v in _throttle_return_values if v == "value")
        assert value_count >= 1, "throttled function never returned a value"
        assert none_count >= 1, (
            "throttled function never returned None (never rate-limited)"
        )

    runner.after_frames(30, check_throttle_returns)

    # -- finish --
    runner.on_done(lambda: suite.report_and_quit())
    runner.start()

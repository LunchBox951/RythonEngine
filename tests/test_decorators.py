"""
Tests for rython._decorators (throttle decorator).

Strategy
--------
``throttle`` is imported from ``rython._decorators`` at module level; this
works because ``_decorators.py`` does NOT import ``rython`` at the top of the
file — the ``import rython`` happens inside the wrapper closure, at call time.

Each test creates a ``FakeTime`` clock, patches ``sys.modules["rython"]``
only for the *duration of function calls*, and verifies call counts /
return values.
"""

from __future__ import annotations

from unittest.mock import MagicMock, patch

import pytest

from rython._decorators import throttle


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

class FakeTime:
    """Mutable clock that mimics ``rython.time``."""

    def __init__(self, initial: float = 0.0) -> None:
        self._t = initial

    @property
    def elapsed(self) -> float:
        return self._t

    def advance(self, dt: float) -> None:
        self._t += dt


def _mock_rython(fake_time: FakeTime) -> MagicMock:
    m = MagicMock()
    m.time = fake_time
    return m


# ---------------------------------------------------------------------------
# throttle — basic behaviour
# ---------------------------------------------------------------------------

class TestThrottleBasic:
    def test_first_call_executes(self):
        ft = FakeTime(0.0)
        calls: list[int] = []

        @throttle(hz=10)
        def fn() -> None:
            calls.append(1)

        with patch.dict("sys.modules", {"rython": _mock_rython(ft)}):
            fn()

        assert len(calls) == 1

    def test_second_call_too_soon_skipped(self):
        ft = FakeTime(0.0)
        calls: list[int] = []

        @throttle(hz=10)  # interval = 0.1 s
        def fn() -> None:
            calls.append(1)

        with patch.dict("sys.modules", {"rython": _mock_rython(ft)}):
            fn()           # t=0.0 — executes
            ft.advance(0.05)
            fn()           # t=0.05 — too soon, skipped

        assert len(calls) == 1

    def test_call_after_interval_executes(self):
        ft = FakeTime(0.0)
        calls: list[int] = []

        @throttle(hz=10)  # interval = 0.1 s
        def fn() -> None:
            calls.append(1)

        with patch.dict("sys.modules", {"rython": _mock_rython(ft)}):
            fn()           # t=0.0 executes
            ft.advance(0.1)
            fn()           # t=0.1 — boundary, executes

        assert len(calls) == 2

    def test_multiple_skips_then_execute(self):
        ft = FakeTime(0.0)
        calls: list[int] = []

        @throttle(hz=5)  # interval = 0.2 s
        def fn() -> None:
            calls.append(1)

        with patch.dict("sys.modules", {"rython": _mock_rython(ft)}):
            fn()           # t=0.0 execute
            ft.advance(0.05)
            fn()           # skip
            ft.advance(0.05)
            fn()           # skip
            ft.advance(0.1)
            fn()           # t=0.2 execute

        assert len(calls) == 2


# ---------------------------------------------------------------------------
# throttle — scene reload edge case
# ---------------------------------------------------------------------------

class TestThrottleSceneReload:
    def test_clock_reset_triggers_immediate_call(self):
        ft = FakeTime(100.0)
        calls: list[int] = []

        @throttle(hz=10)  # interval = 0.1 s
        def fn() -> None:
            calls.append(1)

        with patch.dict("sys.modules", {"rython": _mock_rython(ft)}):
            fn()           # t=100.0 executes, last=100.0
            ft._t = 0.0   # simulate scene reload (clock reset)
            fn()           # elapsed(0.0) < last(100.0) → reset → executes

        assert len(calls) == 2

    def test_after_reload_throttle_resumes_normally(self):
        ft = FakeTime(100.0)
        calls: list[int] = []

        @throttle(hz=10)  # interval = 0.1 s
        def fn() -> None:
            calls.append(1)

        with patch.dict("sys.modules", {"rython": _mock_rython(ft)}):
            fn()           # t=100.0 executes
            ft._t = 0.0   # reload
            fn()           # executes (reset)
            ft.advance(0.05)
            fn()           # skip — only 0.05 s since reload
            ft.advance(0.05)
            fn()           # t=0.1 executes

        assert len(calls) == 3


# ---------------------------------------------------------------------------
# throttle — validation & metadata
# ---------------------------------------------------------------------------

class TestThrottleValidation:
    def test_hz_zero_raises(self):
        with pytest.raises(ValueError, match="hz"):
            @throttle(hz=0)
            def fn() -> None:
                pass

    def test_hz_negative_raises(self):
        with pytest.raises(ValueError, match="hz"):
            @throttle(hz=-5)
            def fn() -> None:
                pass

    def test_functools_wraps_preserves_name(self):
        @throttle(hz=30)
        def my_special_function() -> None:
            """Docstring here."""

        assert my_special_function.__name__ == "my_special_function"
        assert my_special_function.__doc__ == "Docstring here."

    def test_return_value_passed_through(self):
        ft = FakeTime(0.0)

        @throttle(hz=10)
        def fn() -> int:
            return 42

        with patch.dict("sys.modules", {"rython": _mock_rython(ft)}):
            result = fn()

        assert result == 42

    def test_skipped_call_returns_none(self):
        ft = FakeTime(0.0)

        @throttle(hz=10)
        def fn() -> int:
            return 42

        with patch.dict("sys.modules", {"rython": _mock_rython(ft)}):
            fn()              # executes
            result = fn()     # too soon — skipped

        assert result is None

    def test_args_forwarded(self):
        ft = FakeTime(0.0)
        received: list[tuple] = []

        @throttle(hz=10)
        def fn(a: int, b: str) -> None:
            received.append((a, b))

        with patch.dict("sys.modules", {"rython": _mock_rython(ft)}):
            fn(1, "hello")

        assert received == [(1, "hello")]


# ---------------------------------------------------------------------------
# throttle — rython.__init__ export
# ---------------------------------------------------------------------------

class TestThrottleExport:
    def test_throttle_importable_from_rython(self):
        """throttle must be importable from the rython package directly."""
        import rython  # noqa: PLC0415
        assert hasattr(rython, "throttle"), (
            "rython.throttle not found — ensure it is exported in rython/__init__.py"
        )

    def test_throttle_is_callable(self):
        import rython  # noqa: PLC0415
        assert callable(rython.throttle)

    def test_throttle_in_all(self):
        import rython  # noqa: PLC0415
        assert "throttle" in rython.__all__

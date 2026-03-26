"""
Decorator utilities for rython game scripts.
"""

from __future__ import annotations

import functools
from typing import Any, Callable, TypeVar

F = TypeVar("F", bound=Callable[..., Any])


def throttle(hz: float) -> Callable[[F], F]:
    """
    Decorator factory that limits how often the wrapped function executes.

    ``@throttle(hz=30)`` means the function body runs at most 30 times per
    second.  Frames that arrive too soon are silently skipped (return ``None``).

    Uses ``rython.time.elapsed`` (seconds since engine start) as the clock.
    This keeps the decorator free of wall-clock imports and honours the
    engine's notion of time.

    Edge case — scene reload:  if ``rython.time.elapsed`` returns a value
    *less* than the last recorded call time, the engine clock has been reset
    (e.g. the scene was reloaded).  In that case the tracking state is cleared
    and the call proceeds immediately.

    Parameters
    ----------
    hz:
        Maximum invocations per second.  Must be greater than zero.

    Example
    -------
    ::

        @rython.throttle(hz=20)
        def fire_bullet():
            rython.scene.spawn(...)

    Prefer ``rython.scheduler.on_timer`` for time-based logic that does not
    need to be called from inside a per-frame callback.
    """
    if hz <= 0:
        raise ValueError(f"throttle hz must be > 0, got {hz!r}")

    interval = 1.0 / hz

    def decorator(fn: F) -> F:
        # Use a list as a mutable cell so the nested closure can rebind it.
        # Initialised so that the very first call always executes.
        last: list[float] = [-interval]

        @functools.wraps(fn)
        def wrapper(*args: Any, **kwargs: Any) -> Any:
            import rython  # local import avoids circular dependency at module load

            now: float = rython.time.elapsed
            if now < last[0]:
                # Clock went backwards — scene reload or similar reset.
                # Clear tracking so the next call goes through immediately.
                last[0] = now - interval
            if now - last[0] >= interval:
                last[0] = now
                return fn(*args, **kwargs)
            return None

        return wrapper  # type: ignore[return-value]

    return decorator

"""
Pure-Python type stub for the rython TimeBridge.
"""

from __future__ import annotations


class TimeBridge:
    """Time utilities exposed as ``rython.time``."""

    @property
    def elapsed(self) -> float:
        """Seconds elapsed since the engine started (monotonically increasing)."""
        raise NotImplementedError

    def __repr__(self) -> str:
        raise NotImplementedError

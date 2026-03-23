"""
Pure-Python type stub for the rython EngineBridge.
"""

from __future__ import annotations


class EngineBridge:
    """Engine lifecycle control exposed as ``rython.engine``."""

    def request_quit(self) -> None:
        """Signal the engine to exit cleanly after the current frame."""
        raise NotImplementedError

    def __repr__(self) -> str:
        raise NotImplementedError

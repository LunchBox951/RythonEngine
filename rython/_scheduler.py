"""
Pure-Python type stub for the rython SchedulerBridge.
"""

from __future__ import annotations

from typing import Callable


class SchedulerBridge:
    """
    Scheduler bridge exposed as ``rython.scheduler``.

    Wraps ``TaskScheduler::register_recurring_sequential`` at the Python level.
    """

    def register_recurring(self, callback: Callable[[], None]) -> None:
        """
        Register *callback* to be called once per frame for the lifetime of
        the engine run.  The callable receives no arguments.
        """
        raise NotImplementedError

    def __repr__(self) -> str:
        raise NotImplementedError

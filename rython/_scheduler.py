"""
Pure-Python type stub for the rython SchedulerBridge.
"""

from __future__ import annotations

from typing import Callable


class SchedulerBridge:
    """
    Scheduler bridge exposed as ``rython.scheduler``.

    Wraps ``TaskScheduler::register_recurring_sequential`` at the Python level.

    .. note::
        :meth:`on_timer` and :meth:`on_event` are **one-shot** helpers — each
        fires the callback exactly once, then becomes a no-op.  To repeat,
        schedule a new call from within the callback.  For persistent
        per-frame work use :meth:`register_recurring`.
    """

    def register_recurring(self, callback: Callable[[], None]) -> None:
        """
        Register *callback* to be called once per frame for the lifetime of
        the engine run.  The callable receives no arguments.

        .. note::
            Prefer :meth:`on_timer` or :meth:`on_event` for time- or
            event-based logic.  ``register_recurring`` has no built-in way to
            throttle or stop the callback.
        """
        raise NotImplementedError

    def on_timer(self, delay_secs: float, callback: Callable[[], None]) -> None:
        """
        Call *callback* **once** after *delay_secs* seconds have elapsed.

        This is a **one-shot** delay: the callback fires exactly once, then
        becomes a no-op.  To repeat the behaviour, schedule a new timer from
        within *callback*::

            def wave():
                spawn_enemies()
                rython.scheduler.on_timer(10.0, wave)   # re-arm

            rython.scheduler.on_timer(10.0, wave)

        :param delay_secs: Seconds to wait before invoking *callback*.
        :param callback: Zero-argument callable to invoke after the delay.
        """
        raise NotImplementedError

    def on_event(self, event_name: str, callback: Callable[..., None]) -> None:
        """
        Subscribe *callback* to the named scene event for **one firing**.

        The callback is invoked the next time ``rython.scene.emit(event_name,
        ...)`` is called, receiving the event payload as keyword arguments.
        After that single invocation the subscription becomes a no-op.

        For a persistent subscription use :meth:`~rython._scene.SceneBridge.subscribe`
        directly on ``rython.scene``.

        :param event_name: The event name to listen for.
        :param callback: Callable accepting keyword arguments from the payload.
        """
        raise NotImplementedError

    def __repr__(self) -> str:
        raise NotImplementedError

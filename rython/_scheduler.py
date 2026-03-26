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
        For most use cases, prefer the higher-level helpers
        :meth:`on_timer` and :meth:`on_event` over the low-level
        :meth:`register_recurring` callback. They are more expressive,
        return cancellable handles, and make intent clearer.
    """

    def register_recurring(self, callback: Callable[[], None]) -> None:
        """
        Register *callback* to be called once per frame for the lifetime of
        the engine run.  The callable receives no arguments.

        .. note::
            Prefer :meth:`on_timer` or :meth:`on_event` for new code.
            ``register_recurring`` has no built-in way to cancel or throttle
            the callback.
        """
        raise NotImplementedError

    def on_timer(self, interval_s: float, callback: Callable[[], None]) -> int:
        """
        Call *callback* repeatedly every *interval_s* seconds.

        Returns an integer handle that can be passed to :meth:`cancel` to
        stop future invocations.

        :param interval_s: Interval in seconds between each invocation.
        :param callback: Zero-argument callable to invoke on each tick.
        :returns: Cancellable subscription handle (int).
        """
        raise NotImplementedError

    def on_event(self, event_name: str, callback: Callable[..., None]) -> int:
        """
        Subscribe *callback* to the named scene event, firing every time
        ``rython.scene.emit(event_name, ...)`` is called.

        The callback receives the event payload as keyword arguments, matching
        the signature of :meth:`~rython._scene.SceneBridge.subscribe`.

        Returns an integer handle that can be passed to :meth:`cancel` to
        unsubscribe.

        :param event_name: The event name to listen for.
        :param callback: Callable accepting keyword arguments from the payload.
        :returns: Cancellable subscription handle (int).
        """
        raise NotImplementedError

    def cancel(self, handle: int) -> None:
        """
        Cancel a recurring timer or event subscription previously registered
        with :meth:`on_timer` or :meth:`on_event`.

        Calling ``cancel`` with an already-cancelled or unknown handle is a
        no-op.

        :param handle: The integer handle returned by :meth:`on_timer` or
            :meth:`on_event`.
        """
        raise NotImplementedError

    def __repr__(self) -> str:
        raise NotImplementedError

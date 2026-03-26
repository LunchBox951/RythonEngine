"""
Pure-Python type stub for the rython SchedulerBridge and JobHandle.
"""

from __future__ import annotations

from typing import Callable, Optional


class JobHandle:
    """
    A handle to a submitted async/parallel task.

    Returned by :meth:`SchedulerBridge.submit_background` and
    :meth:`SchedulerBridge.submit_parallel`.  Poll the status properties each
    frame or register a continuation with :meth:`on_complete`.

    .. note::
        Background tasks complete in a **future frame** (after
        ``flush_python_bg_completions`` runs).  Parallel tasks are done by the
        end of the tick in which they were submitted.
    """

    @property
    def is_done(self) -> bool:
        """``True`` once the task has finished (successfully or not)."""
        raise NotImplementedError

    @property
    def is_pending(self) -> bool:
        """``True`` while the task is still running or queued."""
        raise NotImplementedError

    @property
    def is_failed(self) -> bool:
        """``True`` if the task finished with an uncaught exception."""
        raise NotImplementedError

    @property
    def error(self) -> Optional[str]:
        """
        The error message if :attr:`is_failed`, otherwise ``None``.
        """
        raise NotImplementedError

    def on_complete(self, callback: Callable[[], None]) -> None:
        """
        Register *callback* (zero-argument callable) to fire when the task
        finishes.  If the task is already done, the callback fires immediately.

        :param callback: Zero-argument callable invoked upon task completion.
        """
        raise NotImplementedError

    def __repr__(self) -> str:
        raise NotImplementedError


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

    def submit_background(self, fn: Callable[[], None]) -> JobHandle:
        """
        Submit *fn* as a background task that runs on the thread pool.

        The callable is dispatched to the rayon thread pool and completes
        asynchronously.  The returned :class:`JobHandle` transitions to
        ``is_done`` in a future frame (when ``flush_python_bg_completions``
        runs).

        :param fn: Zero-argument callable to run in the background.
        :returns: A :class:`JobHandle` for monitoring the task.
        """
        raise NotImplementedError

    def submit_parallel(self, fn: Callable[[], None]) -> JobHandle:
        """
        Submit *fn* as a parallel task for the current tick's parallel phase.

        The callable is queued and executed within the current frame during
        ``flush_python_par_tasks``.  The returned :class:`JobHandle` is already
        done by the time the next Python callback runs.

        :param fn: Zero-argument callable to run in the parallel phase.
        :returns: A :class:`JobHandle` for monitoring the task.
        """
        raise NotImplementedError

    def run_sequential(self, fn: Callable[[], None]) -> None:
        """
        Queue *fn* to run on the main thread during the **next** tick's
        sequential phase (``flush_python_seq_tasks``).

        No :class:`JobHandle` is returned.  Use :meth:`on_timer` or events
        for continuation logic.

        :param fn: Zero-argument callable to run next tick.
        """
        raise NotImplementedError

    def __repr__(self) -> str:
        raise NotImplementedError

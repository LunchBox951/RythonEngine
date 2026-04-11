"""Integration tests for rython.scheduler task submission APIs.

Tests submit_background, submit_parallel, run_sequential, JobHandle
properties, and on_complete callbacks.
"""

import rython
from _harness import TestSuite, FrameRunner

suite = TestSuite()
runner = FrameRunner(suite)

# ---------------------------------------------------------------------------
# Mutable state for tracking task results
# ---------------------------------------------------------------------------
_seq_results = []
_on_complete_flag = [False]
_bg_handle = [None]
_par_handle = [None]
_fail_handle = [None]


def init():
    # -- submit_background returns JobHandle --
    _bg_handle[0] = rython.scheduler.submit_background(lambda: None)

    def check_bg_initial():
        # By frame 1 the handle may already be done, so just check it exists
        assert _bg_handle[0] is not None, "submit_background did not return a handle"

    runner.after_frames(1, check_bg_initial)

    def check_bg_done():
        assert _bg_handle[0].is_done, "background job not done after 5 frames"

    runner.after_frames(5, check_bg_done)

    # -- submit_parallel returns JobHandle --
    _par_handle[0] = rython.scheduler.submit_parallel(lambda: None)

    def check_par_done():
        assert _par_handle[0].is_done, "parallel job not done after 3 frames"

    runner.after_frames(3, check_par_done)

    # -- run_sequential queues for next tick --
    rython.scheduler.run_sequential(lambda: _seq_results.append(1))

    def check_seq():
        assert len(_seq_results) == 1, (
            f"expected sequential task to run once, got {len(_seq_results)}"
        )

    runner.after_frames(3, check_seq)

    # -- JobHandle.on_complete fires --
    h = rython.scheduler.submit_background(lambda: None)
    def _mark_complete():
        _on_complete_flag[0] = True

    h.on_complete(_mark_complete)

    def check_on_complete():
        assert _on_complete_flag[0], "on_complete callback did not fire"

    runner.after_frames(10, check_on_complete)

    # -- JobHandle.is_failed on exception --
    _fail_handle[0] = rython.scheduler.submit_background(lambda: 1 / 0)

    def check_failed():
        assert _fail_handle[0].is_failed, "expected job to be marked failed"
        assert _fail_handle[0].error is not None, "expected error message"

    runner.after_frames(5, check_failed)

    # -- finish --
    runner.on_done(lambda: suite.report_and_quit())
    runner.start()

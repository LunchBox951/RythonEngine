"""Test harness for RythonEngine Python integration tests.

Loaded inside the engine's Python interpreter alongside test scripts.
Provides assertion primitives and structured output for the external runner.
"""


class TestSuite:
    """Tracks test results and produces structured output for the runner."""

    def __init__(self):
        self._results = []

    def run(self, name, fn):
        """Run a test function, recording pass or fail."""
        try:
            fn()
            self._results.append((name, True, ""))
        except AssertionError as e:
            self._results.append((name, False, str(e) or "AssertionError"))
        except Exception as e:
            self._results.append((name, False, f"{type(e).__name__}: {e}"))

    def record_pass(self, name):
        """Manually record a passing test (for use in frame callbacks)."""
        self._results.append((name, True, ""))

    def record_fail(self, name, detail=""):
        """Manually record a failing test (for use in frame callbacks)."""
        self._results.append((name, False, detail))

    def report_and_quit(self):
        """Print structured results and request engine shutdown."""
        import rython
        print("RYTHON_TEST_BEGIN", flush=True)
        for name, passed, detail in self._results:
            if passed:
                print(f"PASS {name}", flush=True)
            else:
                print(f"FAIL {name}: {detail}", flush=True)
        print("RYTHON_TEST_END", flush=True)
        rython.engine.request_quit()

    @property
    def passed(self):
        return sum(1 for _, p, _ in self._results if p)

    @property
    def failed(self):
        return sum(1 for _, p, _ in self._results if not p)


class FrameRunner:
    """Schedule test assertions after N engine frames.

    Usage::

        runner = FrameRunner(suite)
        runner.after_frames(5, check_something)
        runner.after_frames(10, check_something_else)
        runner.on_done(lambda: suite.report_and_quit())
        runner.start()
    """

    def __init__(self, suite):
        self._suite = suite
        self._checkpoints = []
        self._on_done = None
        self._frame = 0

    def after_frames(self, n, fn):
        """Schedule fn to run after n frames. fn should use assert."""
        self._checkpoints.append((n, fn))

    def on_done(self, fn):
        """Set callback to run after all checkpoints complete."""
        self._on_done = fn

    def start(self):
        """Register the frame counter as a recurring callback."""
        import rython
        rython.scheduler.register_recurring(self._tick)

    def _tick(self):
        self._frame += 1
        for frame_num, fn in self._checkpoints:
            if self._frame == frame_num:
                try:
                    fn()
                except AssertionError as e:
                    self._suite.record_fail(
                        fn.__name__, str(e) or "AssertionError"
                    )
                except Exception as e:
                    self._suite.record_fail(
                        fn.__name__, f"{type(e).__name__}: {e}"
                    )
                else:
                    self._suite.record_pass(fn.__name__)
        max_frame = max((f for f, _ in self._checkpoints), default=0)
        if self._frame > max_frame and self._on_done:
            self._on_done()

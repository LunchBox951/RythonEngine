"""Tests for rython stub methods.

Verifies that each stub method exists and raises NotImplementedError
(they have no real implementation — the real ones live in the Rust extension).

Modules are loaded directly via importlib to avoid the pre-existing
NameError in rython/__init__.py (missing _PhysicsBridge import).
"""

import importlib.util
import sys
import pytest
from pathlib import Path

_ROOT = Path(__file__).parent.parent


def _load_stub(relpath: str, mod_name: str):
    """Load a rython stub module directly from its file path."""
    spec = importlib.util.spec_from_file_location(mod_name, _ROOT / relpath)
    assert spec is not None and spec.loader is not None
    mod = importlib.util.module_from_spec(spec)
    sys.modules[mod_name] = mod
    spec.loader.exec_module(mod)  # type: ignore[union-attr]
    return mod


# Load stubs without triggering rython/__init__.py
_scheduler = _load_stub("rython/_scheduler.py", "rython._scheduler")
_scene = _load_stub("rython/_scene.py", "rython._scene")

SchedulerBridge = _scheduler.SchedulerBridge
SceneBridge = _scene.SceneBridge


# ---------------------------------------------------------------------------
# SchedulerBridge stubs
# ---------------------------------------------------------------------------


class TestSchedulerBridgeStubs:
    def setup_method(self):
        self.s = SchedulerBridge()

    def test_register_recurring_raises(self):
        with pytest.raises(NotImplementedError):
            self.s.register_recurring(lambda: None)

    def test_on_timer_exists(self):
        assert callable(self.s.on_timer)

    def test_on_timer_raises(self):
        with pytest.raises(NotImplementedError):
            self.s.on_timer(1.0, lambda: None)

    def test_on_event_exists(self):
        assert callable(self.s.on_event)

    def test_on_event_raises(self):
        with pytest.raises(NotImplementedError):
            self.s.on_event("player_died", lambda **kw: None)

    def test_cancel_exists(self):
        assert callable(self.s.cancel)

    def test_cancel_raises(self):
        with pytest.raises(NotImplementedError):
            self.s.cancel(42)

    def test_repr_raises(self):
        with pytest.raises(NotImplementedError):
            repr(self.s)


# ---------------------------------------------------------------------------
# SceneBridge stubs
# ---------------------------------------------------------------------------


class TestSceneBridgeStubs:
    def setup_method(self):
        self.sc = SceneBridge()

    def test_spawn_raises(self):
        with pytest.raises(NotImplementedError):
            self.sc.spawn()

    def test_emit_raises(self):
        with pytest.raises(NotImplementedError):
            self.sc.emit("test_event")

    def test_subscribe_raises(self):
        with pytest.raises(NotImplementedError):
            self.sc.subscribe("test_event", lambda: None)

    def test_attach_script_raises(self):
        with pytest.raises(NotImplementedError):
            self.sc.attach_script(object(), type)

    def test_unsubscribe_exists(self):
        assert callable(self.sc.unsubscribe)

    def test_unsubscribe_raises(self):
        with pytest.raises(NotImplementedError):
            self.sc.unsubscribe(0)

    def test_repr_raises(self):
        with pytest.raises(NotImplementedError):
            repr(self.sc)

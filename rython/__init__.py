"""
rython — pure-Python stub package for IDE support (Pylance / pyright).

This package provides type annotations and docstrings that mirror the PyO3
bridge defined in ``crates/rython-scripting/src/bridge.rs``.  All method
bodies raise ``NotImplementedError``; the real implementations live in the
compiled Rust extension loaded at engine runtime.

Typical usage in a game script::

    import rython

    def init():
        rython.camera.set_position(0.0, 5.0, -10.0)
        cube = rython.scene.spawn(
            transform=rython.Transform(x=0.0, y=0.0, z=0.0),
            mesh="cube",
            tags=["spinning"],
        )
        rython.scheduler.register_recurring(tick)

    def tick():
        t = rython.time.elapsed
        rython.renderer.draw_text(f"t={t:.2f}")
"""

from __future__ import annotations

# ── Decorators ─────────────────────────────────────────────────────────────
from rython._decorators import throttle

# ── Core types ─────────────────────────────────────────────────────────────
from rython._types import Transform, Vec3

# ── Entity ─────────────────────────────────────────────────────────────────
from rython._entity import Entity

# ── Sub-system bridge instances (singletons exposed by the engine) ─────────
from rython._scene import SceneBridge as _SceneBridge
from rython._camera import Camera as _Camera
from rython._scheduler import SchedulerBridge as _SchedulerBridge
from rython._renderer import RendererBridge as _RendererBridge
from rython._time import TimeBridge as _TimeBridge
from rython._engine import EngineBridge as _EngineBridge
from rython._input import InputBridge as _InputBridge
from rython._audio import AudioBridge as _AudioBridge
from rython._physics import PhysicsBridge as _PhysicsBridge
from rython._ui import UIBridge as _UIBridge
from rython._resources import ResourcesBridge as _ResourcesBridge
from rython._stubs import SubModule as _SubModule

# Singleton instances that scripts import as attributes of the rython module.
# At runtime these are replaced by the PyO3 extension; during IDE analysis
# the type checker sees the stub class types below.

scene: _SceneBridge = _SceneBridge()  # type: ignore[assignment]
camera: _Camera = _Camera()  # type: ignore[assignment]
scheduler: _SchedulerBridge = _SchedulerBridge()  # type: ignore[assignment]
renderer: _RendererBridge = _RendererBridge()  # type: ignore[assignment]
time: _TimeBridge = _TimeBridge()  # type: ignore[assignment]
engine: _EngineBridge = _EngineBridge()  # type: ignore[assignment]

physics: _PhysicsBridge = _PhysicsBridge()  # type: ignore[assignment]
audio: _AudioBridge = _AudioBridge()  # type: ignore[assignment]
input: _InputBridge = _InputBridge()  # type: ignore[assignment]
ui: _UIBridge = _UIBridge()  # type: ignore[assignment]
resources: _ResourcesBridge = _ResourcesBridge()  # type: ignore[assignment]
modules: _SubModule = _SubModule("modules")

__all__ = [
    "throttle",
    "Vec3",
    "Transform",
    "Entity",
    "scene",
    "camera",
    "scheduler",
    "renderer",
    "time",
    "engine",
    "physics",
    "audio",
    "input",
    "ui",
    "resources",
    "modules",
]

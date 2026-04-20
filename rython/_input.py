"""Input stubs for IDE support.

The real pyclasses and enum types live in the compiled Rust extension
(`crates/rython-scripting/src/bridge/input_map.rs`). These stubs mirror
them for type-checker / autocomplete purposes only — method bodies raise
`NotImplementedError`.
"""
from __future__ import annotations

from enum import Enum
from typing import Any, Callable, Iterable, Optional, Tuple


# ── ActionValue ─────────────────────────────────────────────────────────────


class ActionValue:
    """Typed value delivered to input callbacks."""

    def as_bool(self) -> bool: ...
    def as_float(self) -> float: ...
    def as_vec2(self) -> Tuple[float, float]: ...
    def as_vec3(self) -> Tuple[float, float, float]: ...

    @property
    def kind(self) -> str:
        """One of ``button``, ``axis1d``, ``axis2d``, ``axis3d``."""
        ...


# ── Hardware enums ──────────────────────────────────────────────────────────


class KeyCode(Enum):
    A = 0; B = 1; C = 2; D = 3; E = 4; F = 5; G = 6; H = 7; I = 8; J = 9
    K = 10; L = 11; M = 12; N = 13; O = 14; P = 15; Q = 16; R = 17; S = 18
    T = 19; U = 20; V = 21; W = 22; X = 23; Y = 24; Z = 25
    Key0 = 26; Key1 = 27; Key2 = 28; Key3 = 29; Key4 = 30
    Key5 = 31; Key6 = 32; Key7 = 33; Key8 = 34; Key9 = 35
    Space = 36; Enter = 37; Escape = 38; Tab = 39; Backspace = 40
    LeftShift = 41; RightShift = 42
    LeftControl = 43; RightControl = 44
    LeftAlt = 45; RightAlt = 46
    Up = 47; Down = 48; Left = 49; Right = 50
    F1 = 51; F2 = 52; F3 = 53; F4 = 54; F5 = 55; F6 = 56
    F7 = 57; F8 = 58; F9 = 59; F10 = 60; F11 = 61; F12 = 62


class MouseButton(Enum):
    Left = 0
    Right = 1
    Middle = 2


class MouseAxis(Enum):
    X = 0
    Y = 1


class GamepadButton(Enum):
    South = 0; East = 1; West = 2; North = 3
    LeftBumper = 4; RightBumper = 5
    LeftTriggerButton = 6; RightTriggerButton = 7
    LeftStickPress = 8; RightStickPress = 9
    DPadUp = 10; DPadDown = 11; DPadLeft = 12; DPadRight = 13
    Start = 14; Select = 15


class GamepadAxis(Enum):
    LeftStickX = 0
    LeftStickY = 1
    RightStickX = 2
    RightStickY = 3
    LeftTrigger = 4
    RightTrigger = 5


class GamepadStick(Enum):
    LeftStick = 0
    RightStick = 1


# ── Spec types ──────────────────────────────────────────────────────────────


class ModifierSpec:
    """Opaque modifier specification returned by ``Modifiers.*`` factories."""


class TriggerSpec:
    """Opaque trigger specification returned by ``Triggers.*`` factories."""


# ── Modifiers / Triggers factories ──────────────────────────────────────────


class _Modifiers:
    """Factory namespace for per-binding modifiers (stateless transforms)."""

    @staticmethod
    def Negate(x: bool = False, y: bool = False, z: bool = False) -> ModifierSpec: ...

    @staticmethod
    def Scale(x: float = 1.0, y: float = 1.0, z: float = 1.0) -> ModifierSpec: ...

    @staticmethod
    def DeadZone(
        lower: float, upper: float = 1.0, radial: bool = False
    ) -> ModifierSpec:
        """Axial (default) or radial deadzone. Values below *lower* are zeroed;
        values between *lower* and *upper* are rescaled to ``[0, 1]``."""
        ...

    @staticmethod
    def Swizzle(order: str) -> ModifierSpec:
        """Axis-reorder. *order* is one of ``"XYZ"``, ``"YXZ"``, ``"ZXY"``,
        ``"YZX"``."""
        ...


class _Triggers:
    """Factory namespace for per-binding triggers (state machines)."""

    @staticmethod
    def Down() -> TriggerSpec:
        """Fires every frame the input is actuated. Default for bindings
        with no explicit trigger."""
        ...

    @staticmethod
    def Pressed() -> TriggerSpec:
        """Rising-edge: fires the frame the input becomes actuated."""
        ...

    @staticmethod
    def Released() -> TriggerSpec:
        """Falling-edge: fires the frame the input becomes inactive."""
        ...

    @staticmethod
    def Hold(threshold_seconds: float) -> TriggerSpec:
        """Fires after *threshold_seconds* of continuous actuation.
        Reports ``Ongoing`` while charging and ``Canceled`` if released early."""
        ...

    @staticmethod
    def Tap(max_seconds: float = 0.25) -> TriggerSpec:
        """Fires once on release if total hold duration stayed below
        *max_seconds*. ``Canceled`` if held longer."""
        ...

    @staticmethod
    def Pulse(interval_seconds: float) -> TriggerSpec:
        """Fires on initial press and every *interval_seconds* while held."""
        ...

    @staticmethod
    def Chorded(partner: str) -> TriggerSpec:
        """Fires only when this input is actuated AND the *partner* action
        is also actuated this frame. The partner must be declared earlier
        in the same map."""
        ...


Modifiers = _Modifiers
Triggers = _Triggers


# ── InputAction ─────────────────────────────────────────────────────────────


class InputAction:
    """One logical action inside an ``InputMap``."""

    @property
    def id(self) -> str: ...
    @property
    def kind(self) -> str:
        """One of ``button``, ``axis1d``, ``axis2d``, ``axis3d``."""
        ...

    def bind(
        self,
        key: Any,
        *,
        modifiers: Optional[Iterable[ModifierSpec]] = None,
        triggers: Optional[Iterable[TriggerSpec]] = None,
    ) -> None:
        """Bind a single hardware key/button/axis to this action."""
        ...

    def bind_composite_2d(
        self,
        *,
        up: KeyCode,
        down: KeyCode,
        left: KeyCode,
        right: KeyCode,
        modifiers: Optional[Iterable[ModifierSpec]] = None,
        triggers: Optional[Iterable[TriggerSpec]] = None,
    ) -> None: ...

    def bind_composite_3d(
        self,
        *,
        up: KeyCode,
        down: KeyCode,
        left: KeyCode,
        right: KeyCode,
        forward: KeyCode,
        back: KeyCode,
        modifiers: Optional[Iterable[ModifierSpec]] = None,
        triggers: Optional[Iterable[TriggerSpec]] = None,
    ) -> None: ...

    def on_started(self, callback: Callable[[ActionValue], Any]) -> None: ...
    def on_ongoing(self, callback: Callable[[ActionValue], Any]) -> None: ...
    def on_triggered(self, callback: Callable[[ActionValue], Any]) -> None: ...
    def on_completed(self, callback: Callable[[ActionValue], Any]) -> None: ...
    def on_canceled(self, callback: Callable[[ActionValue], Any]) -> None: ...


# ── InputMap ────────────────────────────────────────────────────────────────


class InputMap:
    """A prioritized bundle of actions + bindings that designers subclass.

    Construction arguments (``name``, ``priority``) are consumed by
    ``__new__``. Subclasses typically override ``__init__`` to declare
    actions inside the body::

        class MovementMap(rython.InputMap):
            def __init__(self, *args, **kwargs):
                self.move = self.action("move", kind="axis2d")
                self.move.bind_composite_2d(
                    up=KeyCode.W, down=KeyCode.S, left=KeyCode.A, right=KeyCode.D,
                )
                self.move.on_triggered(self._on_move)

            def _on_move(self, value):
                x, y = value.as_vec2()
                ...

        rython.input.push_map(MovementMap(name="gameplay", priority=10))
    """

    def __new__(cls, name: str = "default", priority: int = 0) -> "InputMap": ...

    @property
    def name(self) -> str: ...
    @property
    def priority(self) -> int: ...

    def action(self, id: str, kind: str) -> InputAction:
        """Declare a new action. *kind* must be ``button``, ``axis1d``,
        ``axis2d``, or ``axis3d``."""
        ...


# ── InputBridge ─────────────────────────────────────────────────────────────


class InputBridge:
    """Per-frame input state bridge (``rython.input``)."""

    # Polling
    def axis(self, action: str) -> float:
        """Axis magnitude for *action*. Returns ``0.0`` if unbound."""
        ...

    def axis2(self, action: str) -> Tuple[float, float]:
        """2D axis value (X, Y) for *action*. Returns ``(0, 0)`` if unbound."""
        ...

    def axis3(self, action: str) -> Tuple[float, float, float]:
        """3D axis value (X, Y, Z) for *action*. Returns ``(0, 0, 0)`` if unbound."""
        ...

    def value(self, action: str) -> Optional[ActionValue]:
        """Typed value for *action*, or ``None`` if unbound."""
        ...

    def pressed(self, action: str) -> bool: ...
    def held(self, action: str) -> bool: ...
    def released(self, action: str) -> bool: ...

    # Map lifecycle
    def push_map(self, m: InputMap) -> None: ...
    def pop_map(self, id: str) -> None: ...
    def clear_maps(self) -> None: ...
    def active_maps(self) -> list[str]: ...

    def rebind(
        self, map_id: str, action_id: str, binding_index: int, new_key: Any
    ) -> None:
        """Swap the hardware key at (*map_id*, *action_id*, *binding_index*)
        to *new_key* (a ``KeyCode`` / ``MouseButton`` / etc.)."""
        ...

    def __repr__(self) -> str:
        return "rython.input"

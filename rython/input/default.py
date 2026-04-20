"""Default input map — reinstates the old hardcoded bindings on the new
customizable-input API.

Before the customizable-input refactor, ``PlayerController`` booted with a
hardcoded `"default"` map that bound ``move_x / move_z / jump / pause`` to
WASD + arrows + Space + Escape. Games that relied on that polling-only
surface can restore the old behaviour with a single call:

.. code-block:: python

    import rython
    from rython.input.default import build_default_map

    def init():
        rython.input.push_map(build_default_map())

The returned map keeps the same action ids (``move_x``, ``move_z``,
``jump``, ``pause``) so existing polling calls — ``rython.input.axis(
"move_x")`` etc. — keep working.
"""
from __future__ import annotations

from rython import InputMap, KeyCode, Modifiers, Triggers


class _DefaultMap(InputMap):
    """Gameplay defaults: WASD/arrow movement, Space jump, Escape pause."""

    def __init__(self, *args: object, **kwargs: object) -> None:
        # Construction args are consumed by ``__new__`` (the Rust pyclass).
        del args, kwargs

        # X axis: A (–) / D (+), plus Left (–) / Right (+)
        move_x = self.action("move_x", kind="axis1d")
        move_x.bind_composite_2d(
            up=KeyCode.W, down=KeyCode.S, left=KeyCode.D, right=KeyCode.A,
        )
        move_x.bind_composite_2d(
            up=KeyCode.Up, down=KeyCode.Down, left=KeyCode.Right, right=KeyCode.Left,
        )

        # Z axis (forward/back): W (+) / S (–), plus Up / Down. Composite2D
        # natively produces Y-axis for up/down keys; swizzle YXZ moves that
        # Y value into the X component so the narrowed Axis1D picks it up.
        _fwd_swizzle = [Modifiers.Swizzle("YXZ")]
        move_z = self.action("move_z", kind="axis1d")
        move_z.bind_composite_2d(
            up=KeyCode.W, down=KeyCode.S, left=KeyCode.A, right=KeyCode.D,
            modifiers=_fwd_swizzle,
        )
        move_z.bind_composite_2d(
            up=KeyCode.Up, down=KeyCode.Down, left=KeyCode.Left, right=KeyCode.Right,
            modifiers=_fwd_swizzle,
        )

        jump = self.action("jump", kind="button")
        jump.bind(KeyCode.Space, triggers=[Triggers.Pressed()])

        pause = self.action("pause", kind="button")
        pause.bind(KeyCode.Escape, triggers=[Triggers.Pressed()])


def build_default_map(name: str = "default", priority: int = 0) -> InputMap:
    """Construct a fresh default map. Call once during game boot:

        rython.input.push_map(build_default_map())

    Returns a new ``InputMap`` instance every call (contexts are owned by
    the caller).
    """
    return _DefaultMap(name=name, priority=priority)


__all__ = ["build_default_map"]

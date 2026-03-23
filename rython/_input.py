"""InputBridge stub for IDE support."""
from __future__ import annotations


class InputBridge:
    """Per-frame input state bridge (``rython.input``)."""

    def axis(self, action: str) -> float:
        """Axis value for *action* (-1.0 to 1.0). Returns ``0.0`` if unbound."""
        raise NotImplementedError

    def pressed(self, action: str) -> bool:
        """``True`` on the first frame *action* is pressed."""
        raise NotImplementedError

    def held(self, action: str) -> bool:
        """``True`` every frame *action* is held."""
        raise NotImplementedError

    def released(self, action: str) -> bool:
        """``True`` on the first frame *action* is released."""
        raise NotImplementedError

    def __repr__(self) -> str:
        return "rython.input"

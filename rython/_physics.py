"""PhysicsBridge stub for IDE support."""
from __future__ import annotations


class PhysicsBridge:
    """Physics world bridge (``rython.physics``)."""

    def set_gravity(self, x: float, y: float, z: float) -> None:
        """Set the gravity vector for the physics world."""
        raise NotImplementedError

    def __repr__(self) -> str:
        return "rython.physics"

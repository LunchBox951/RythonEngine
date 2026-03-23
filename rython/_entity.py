"""
Pure-Python type stub for the rython Entity wrapper.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from rython._types import Transform, Vec3


class Entity:
    """A handle to an ECS entity returned by ``rython.scene.spawn()``."""

    id: int

    def __init__(self, id: int = 0) -> None:
        """Construct an entity handle for *id* (usually created by the engine)."""
        raise NotImplementedError

    @property
    def transform(self) -> "Transform":
        """Return the entity's current TransformComponent values."""
        raise NotImplementedError

    def has_tag(self, tag: str) -> bool:
        """Return True if the entity has the given string tag attached."""
        raise NotImplementedError

    def add_tag(self, tag: str) -> None:
        """Attach a string tag to this entity."""
        raise NotImplementedError

    def despawn(self) -> None:
        """Queue this entity for removal at the end of the current frame."""
        raise NotImplementedError

    def apply_force(self, force: "Vec3") -> None:
        """Apply a continuous force to this entity's rigid body."""
        raise NotImplementedError

    def apply_impulse(self, impulse: "Vec3") -> None:
        """Apply an instant impulse to this entity's rigid body."""
        raise NotImplementedError

    def set_velocity(self, vel: "Vec3") -> None:
        """Set the linear velocity of this entity's rigid body."""
        raise NotImplementedError

    @property
    def velocity(self) -> "Vec3":
        """Return the current linear velocity of this entity's rigid body."""
        raise NotImplementedError

    def __repr__(self) -> str:
        raise NotImplementedError

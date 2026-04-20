"""PhysicsBridge stub for IDE support."""
from __future__ import annotations

from typing import Optional

from rython._entity import Entity
from rython._types import Vec3


class RayHit:
    """Result of a successful scene query (raycast or sphere-cast).

    ``distance`` is an alias for ``toi`` (time-of-impact equals distance
    along the unit-normalised direction vector).
    """

    entity: Entity
    """The entity whose collider was hit."""

    point: Vec3
    """World-space point of first contact."""

    normal: Vec3
    """World-space outward surface normal at the hit point."""

    toi: float
    """Time-of-impact: distance along the (unit) ray direction to the hit."""

    @property
    def distance(self) -> float:
        """Alias for ``toi``."""
        return self.toi


class PhysicsBridge:
    """Physics world bridge (``rython.physics``)."""

    def set_gravity(self, x: float, y: float, z: float) -> None:
        """Set the gravity vector for the physics world."""
        raise NotImplementedError

    def raycast(
        self,
        origin: tuple[float, float, float],
        direction: tuple[float, float, float],
        max_dist: float,
    ) -> Optional[RayHit]:
        """Cast a ray from *origin* in *direction* up to *max_dist* world units.

        *direction* is normalised internally.  Returns a :class:`RayHit` on
        the first collider hit, or ``None`` if nothing is within range.
        """
        raise NotImplementedError

    def sphere_cast(
        self,
        origin: tuple[float, float, float],
        direction: tuple[float, float, float],
        radius: float,
        max_dist: float,
    ) -> Optional[RayHit]:
        """Cast a sphere of *radius* from *origin* in *direction* up to *max_dist*.

        *direction* is normalised internally.  Returns a :class:`RayHit` on
        the first collider hit, or ``None`` if nothing is within range.
        """
        raise NotImplementedError

    def ground_normal(
        self,
        entity: Entity,
        max_dist: float = 2.0,
    ) -> Optional[Vec3]:
        """Return the ground surface normal directly below *entity*.

        Casts a ray straight down from the entity's rigid-body position and
        returns the hit normal as a :class:`Vec3`, or ``None`` if no collider
        is within *max_dist* world units.
        """
        raise NotImplementedError

    def __repr__(self) -> str:
        return "rython.physics"

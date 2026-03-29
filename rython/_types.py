"""
Pure-Python type stubs for rython Vec3 and Transform.

At runtime these classes raise NotImplementedError — they exist solely to give
Pylance and other type checkers accurate type information for IDE support.
"""

from __future__ import annotations

from typing import Optional


class Vec3:
    """A 3-component float vector exposed by the rython engine (PyO3 bridge)."""

    x: float
    y: float
    z: float

    def __init__(self, x: float, y: float, z: float) -> None:
        """Create a Vec3 with the given components."""
        raise NotImplementedError

    def length(self) -> float:
        """Return the Euclidean length of this vector."""
        raise NotImplementedError

    def normalized(self) -> "Vec3":
        """Return a unit-length copy of this vector (zero-safe)."""
        raise NotImplementedError

    def dot(self, other: "Vec3") -> float:
        """Return the dot product with *other*."""
        raise NotImplementedError

    def __add__(self, other: "Vec3") -> "Vec3":
        raise NotImplementedError

    def __sub__(self, other: "Vec3") -> "Vec3":
        raise NotImplementedError

    def __mul__(self, scalar: float) -> "Vec3":
        raise NotImplementedError

    def __rmul__(self, scalar: float) -> "Vec3":
        raise NotImplementedError

    def __neg__(self) -> "Vec3":
        raise NotImplementedError

    def __repr__(self) -> str:
        raise NotImplementedError


class Transform:
    """World-space transform component bound to an ECS entity."""

    x: float
    y: float
    z: float
    rot_x: float
    rot_y: float
    rot_z: float
    scale: float
    scale_x: float
    scale_y: float
    scale_z: float

    def __init__(
        self,
        x: float = 0.0,
        y: float = 0.0,
        z: float = 0.0,
        rot_x: float = 0.0,
        rot_y: float = 0.0,
        rot_z: float = 0.0,
        scale: float = 1.0,
        scale_x: Optional[float] = None,
        scale_y: Optional[float] = None,
        scale_z: Optional[float] = None,
    ) -> None:
        """Create a standalone Transform (not yet bound to an entity).

        *scale* sets all axes uniformly. Use *scale_x*, *scale_y*, *scale_z*
        to override individual axes (defaults to *scale* when ``None``).
        """
        raise NotImplementedError

    def __repr__(self) -> str:
        raise NotImplementedError

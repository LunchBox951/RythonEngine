"""
Pure-Python type stub for the rython Camera.
"""

from __future__ import annotations


class Camera:
    """
    Camera control bridge exposed as ``rython.camera``.

    The camera starts at ``(0, 0, -10)`` facing the origin.
    All rotation angles are in radians.
    """

    pos_x: float
    pos_y: float
    pos_z: float
    rot_pitch: float
    rot_yaw: float
    rot_roll: float

    def set_position(self, x: float, y: float, z: float) -> None:
        """Move the camera to world-space position *(x, y, z)*."""
        raise NotImplementedError

    def set_rotation(self, pitch: float, yaw: float, roll: float) -> None:
        """Set camera orientation as Euler angles (radians)."""
        raise NotImplementedError

    def set_look_at(self, target_x: float, target_y: float, target_z: float) -> None:
        """Orient the camera toward the given world-space point."""
        raise NotImplementedError

    def __repr__(self) -> str:
        raise NotImplementedError

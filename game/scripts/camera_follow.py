"""Third-person smooth camera follow for Gauntlet of Cubes."""
import rython
from game.scripts import player

OFFSET = (0.0, 8.0, -12.0)
SMOOTH: float = 5.0

_cam_x: float = 0.0
_cam_y: float = 8.0
_cam_z: float = -12.0


def init() -> None:
    """Snap camera to player immediately (call on level load)."""
    global _cam_x, _cam_y, _cam_z
    print('[DEBUG camera_follow.init] entry')
    px, py, pz = player.get_position()
    _cam_x = px + OFFSET[0]
    _cam_y = py + OFFSET[1]
    _cam_z = pz + OFFSET[2]
    rython.camera.set_position(_cam_x, _cam_y, _cam_z)
    rython.camera.set_look_at(px, py + 1.0, pz)
    print(f'[DEBUG camera_follow.init] exit: cam=({_cam_x:.2f},{_cam_y:.2f},{_cam_z:.2f}), look_at=({px:.2f},{py+1.0:.2f},{pz:.2f})')


def update(dt: float) -> None:
    """Smoothly follow the player each frame."""
    global _cam_x, _cam_y, _cam_z
    px, py, pz = player.get_position()
    target_x = px + OFFSET[0]
    target_y = py + OFFSET[1]
    target_z = pz + OFFSET[2]

    t = min(SMOOTH * dt, 1.0)
    _cam_x += (target_x - _cam_x) * t
    _cam_y += (target_y - _cam_y) * t
    _cam_z += (target_z - _cam_z) * t

    rython.camera.set_position(_cam_x, _cam_y, _cam_z)
    rython.camera.set_look_at(px, py + 1.0, pz)

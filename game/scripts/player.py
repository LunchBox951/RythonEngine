"""Player controller for Gauntlet of Cubes."""
import rython
from rython import Transform, Entity
from typing import Optional, Tuple

MOVE_SPEED: float = 6.0
JUMP_IMPULSE: float = 12.0

_entity: Optional[Entity] = None
_spawn_pos: Tuple[float, float, float] = (0.0, 2.0, 0.0)
_jump_sub_id: Optional[int] = None
_collision_sub_id: Optional[int] = None
_collision_end_sub_id: Optional[int] = None
_floor_contacts: set = set()


def _on_jump(**kwargs) -> None:
    """Handle input:jump event — apply jump impulse when grounded."""
    if kwargs.get("value") != 1.0:
        return
    if _entity is None:
        return
    if is_grounded():
        _entity.apply_impulse(0.0, JUMP_IMPULSE, 0.0)


def _on_collision(**kwargs) -> None:
    """Track ground contact via upward-facing collision normal."""
    global _floor_contacts
    normal = kwargs.get("normal", [0.0, 0.0, 0.0])
    if normal[1] > 0.7:
        entity_a = kwargs.get("entity_a")
        entity_b = kwargs.get("entity_b")
        other_id = entity_b if (_entity is not None and entity_a == _entity.id) else entity_a
        if other_id is not None:
            _floor_contacts.add(other_id)


def _on_collision_end(**kwargs) -> None:
    """Remove entity from floor contacts when solid contact ends."""
    global _floor_contacts
    entity_a = kwargs.get("entity_a")
    entity_b = kwargs.get("entity_b")
    other_id = entity_b if (_entity is not None and entity_a == _entity.id) else entity_a
    if other_id is not None:
        _floor_contacts.discard(other_id)


def spawn(x: float = 0.0, y: float = 2.0, z: float = 0.0) -> Entity:
    """Spawn the player entity at (x, y, z) and store its handle."""
    global _entity, _spawn_pos, _jump_sub_id, _collision_sub_id, _collision_end_sub_id, _floor_contacts
    _spawn_pos = (x, y, z)
    _floor_contacts = set()
    _entity = rython.scene.spawn(
        transform=Transform(x=x, y=y, z=z, scale_x=0.8, scale_y=1.8, scale_z=0.8),
        mesh={"mesh_id": "cube", "texture_id": "game/assets/textures/Light/light_box_alt1.png"},
        tags=["player"],
        rigid_body={"body_type": "dynamic", "mass": 1.0},
        collider={"shape": "box", "size": [0.8, 1.8, 0.8]},
    )
    if _jump_sub_id is None:
        _jump_sub_id = rython.scene.subscribe("input:jump", _on_jump)
    _collision_sub_id = rython.scene.subscribe(
        f"collision:{_entity.id}", _on_collision
    )
    _collision_end_sub_id = rython.scene.subscribe(
        f"collision_end:{_entity.id}", _on_collision_end
    )
    return _entity


def update(dt: float) -> None:
    """Called every frame while the game is PLAYING."""
    if _entity is None:
        return

    move_x = rython.input.axis("move_x")
    move_z = rython.input.axis("move_z")
    vel_y = _entity.velocity.y
    if move_x != 0.0 or move_z != 0.0:
        _entity.set_velocity(move_x * MOVE_SPEED, vel_y, move_z * MOVE_SPEED)
    elif is_grounded():
        _entity.set_velocity(0.0, vel_y, 0.0)

    if _entity.transform.y < -20.0:
        respawn()


def is_grounded() -> bool:
    """Return True when the player has an active upward-facing solid contact."""
    return len(_floor_contacts) > 0


def get_entity() -> Optional[Entity]:
    return _entity


def get_position() -> Tuple[float, float, float]:
    """Return (x, y, z) of the player, or spawn pos if not spawned."""
    if _entity is None:
        return _spawn_pos
    t = _entity.transform
    return (t.x, t.y, t.z)


def respawn() -> None:
    """Teleport the player back to the spawn position."""
    global _entity, _collision_sub_id, _collision_end_sub_id, _floor_contacts
    if _entity is not None:
        if _collision_sub_id is not None:
            rython.scene.unsubscribe(f"collision:{_entity.id}", _collision_sub_id)
            _collision_sub_id = None
        if _collision_end_sub_id is not None:
            rython.scene.unsubscribe(f"collision_end:{_entity.id}", _collision_end_sub_id)
            _collision_end_sub_id = None
        _entity.despawn()
    _floor_contacts = set()
    x, y, z = _spawn_pos
    _entity = rython.scene.spawn(
        transform=Transform(x=x, y=y, z=z, scale_x=0.8, scale_y=1.8, scale_z=0.8),
        mesh={"mesh_id": "cube", "texture_id": "game/assets/textures/Light/light_box_alt1.png"},
        tags=["player"],
        rigid_body={"body_type": "dynamic", "mass": 1.0},
        collider={"shape": "box", "size": [0.8, 1.8, 0.8]},
    )
    _collision_sub_id = rython.scene.subscribe(
        f"collision:{_entity.id}", _on_collision
    )
    _collision_end_sub_id = rython.scene.subscribe(
        f"collision_end:{_entity.id}", _on_collision_end
    )


def despawn() -> None:
    """Remove the player entity."""
    global _entity, _collision_sub_id, _collision_end_sub_id, _floor_contacts
    if _entity is not None:
        if _collision_sub_id is not None:
            rython.scene.unsubscribe(f"collision:{_entity.id}", _collision_sub_id)
            _collision_sub_id = None
        if _collision_end_sub_id is not None:
            rython.scene.unsubscribe(f"collision_end:{_entity.id}", _collision_end_sub_id)
            _collision_end_sub_id = None
        _entity.despawn()
        _entity = None
    _floor_contacts = set()

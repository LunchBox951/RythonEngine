"""Level construction helpers shared by all arena modules."""
import rython
from rython import Transform, Entity
from typing import Optional, List, Dict, Any

_entity_registry: List[Entity] = []


def spawn_static_block(
    x: float,
    y: float,
    z: float,
    sx: float = 1.0,
    sy: float = 1.0,
    sz: float = 1.0,
    tags: Optional[List[str]] = None,
    mesh_opts: Optional[Dict[str, Any]] = None,
) -> Entity:
    """Spawn an immovable cube block and register it for cleanup."""
    mesh: Any = mesh_opts if isinstance(mesh_opts, dict) else "cube"
    entity = rython.scene.spawn(
        transform=Transform(x=x, y=y, z=z),
        mesh=mesh,
        tags=(tags or []) + ["static_block"],
        rigid_body={"body_type": "static"},
        collider={"shape": "box", "size": [sx, sy, sz]},
    )
    _entity_registry.append(entity)
    return entity


def spawn_pickup(
    x: float,
    y: float,
    z: float,
    pickup_type: str = "health",
    value: int = 25,
    tags: Optional[List[str]] = None,
) -> Entity:
    """Spawn a pickup item (health, score, etc.) and register it."""
    entity = rython.scene.spawn(
        transform=Transform(x=x, y=y, z=z, scale=0.5),
        mesh="cube",
        tags=["pickup", pickup_type] + (tags or []),
        rigid_body={"body_type": "kinematic"},
        collider={"shape": "box", "size": [0.5, 0.5, 0.5], "is_trigger": True},
    )
    _entity_registry.append(entity)
    return entity


def spawn_enemy(
    x: float,
    y: float,
    z: float,
    enemy_type: str = "skeleton",
    is_boss: bool = False,
    tags: Optional[List[str]] = None,
) -> Entity:
    """Spawn an enemy entity and register it for cleanup."""
    entity_tags = ["enemy", enemy_type] + (["boss"] if is_boss else []) + (tags or [])
    mass = 5.0 if is_boss else 2.0
    collider_size = [1.5, 2.5, 1.5] if is_boss else [1.0, 2.0, 1.0]
    entity_scale = 1.5 if is_boss else 1.0
    entity = rython.scene.spawn(
        transform=Transform(x=x, y=y, z=z, scale=entity_scale),
        mesh="cube",
        tags=entity_tags,
        rigid_body={"body_type": "dynamic", "mass": mass},
        collider={"shape": "box", "size": collider_size},
    )
    _entity_registry.append(entity)
    return entity


def register_entity(entity: Entity) -> None:
    """Register an externally-spawned entity for cleanup on level transition."""
    _entity_registry.append(entity)


def clear_level() -> None:
    """Despawn all registered entities and clear the registry."""
    for entity in _entity_registry:
        try:
            entity.despawn()
        except Exception:
            pass
    _entity_registry.clear()

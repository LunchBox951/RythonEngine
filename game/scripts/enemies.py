"""Enemy management wrapper — coordinates all active NPC AI states."""
from typing import List, Any
from game.scripts import player

_enemies: List[Any] = []


def register(entity: Any, enemy_type: str = "skeleton", is_boss: bool = False) -> None:
    """Register a spawned entity as an enemy with skeleton AI."""
    from game.scripts.npc.skeleton import create_state
    state = create_state(entity, is_boss)
    _enemies.append(state)


def update(dt: float) -> None:
    """Tick all active enemy AI states."""
    from game.scripts.npc import skeleton
    px, py, pz = player.get_position()
    for state in _enemies:
        skeleton.update(state, dt, px, py, pz)


def clear() -> None:
    """Remove all tracked enemies (call on level transition)."""
    _enemies.clear()


def get_enemies() -> List[Any]:
    return _enemies

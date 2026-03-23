"""
levels/arena_1.py — Tutorial Arena.

20x20 ground, 4 raised platforms, 1 slow skeleton, 3 pickups.
Theme: Green / Light.

Level completion: collect all 3 pickups.
"""
from __future__ import annotations

import rython
from game.scripts import game_state, player, enemies
from game.scripts import level_builder as lb

_pickup_entities: list = []
_pickups_total: int = 3
_collected: int = 0
_tick_registered: bool = False


def build() -> None:
    global _pickup_entities, _collected, _tick_registered
    _pickup_entities = []
    _collected = 0

    # Ground — 20x20
    lb.spawn_static_block(0.0, -0.5, 0.0, 20.0, 1.0, 20.0)

    # Border walls
    lb.spawn_static_block(-10.0, 1.0, 0.0, 1.0, 2.0, 20.0)  # west
    lb.spawn_static_block(10.0, 1.0, 0.0, 1.0, 2.0, 20.0)   # east
    lb.spawn_static_block(0.0, 1.0, -10.0, 20.0, 2.0, 1.0)  # north
    lb.spawn_static_block(0.0, 1.0, 10.0, 20.0, 2.0, 1.0)   # south

    # 4 raised platforms
    lb.spawn_static_block(-5.0, 2.0, -5.0, 4.0, 0.5, 4.0)
    lb.spawn_static_block(5.0, 3.0, -5.0, 4.0, 0.5, 4.0)
    lb.spawn_static_block(-5.0, 4.0, 5.0, 4.0, 0.5, 4.0)
    lb.spawn_static_block(5.0, 2.5, 5.0, 4.0, 0.5, 4.0)

    # 3 score pickups
    p1 = lb.spawn_pickup(-5.0, 2.5, -5.0, pickup_type="score", value=100)
    p2 = lb.spawn_pickup(5.0, 3.5, -5.0, pickup_type="score", value=100)
    p3 = lb.spawn_pickup(0.0, 0.5, 0.0, pickup_type="score", value=100)
    _pickup_entities.extend([p1, p2, p3])

    # 1 slow skeleton (patrol speed overridden via waypoints at a slower pace)
    entity = lb.spawn_enemy(3.0, 1.0, 3.0, enemy_type="skeleton", is_boss=False)
    enemies.register(entity, enemy_type="skeleton", is_boss=False)

    # Spawn player at centre
    player.spawn(0.0, 2.0, 0.0)

    # Music
    rython.audio.play("game/assets/music/arena1.mp3", "music", True)
    rython.audio.set_volume("music", 0.7)

    if not _tick_registered:
        _tick_registered = True
        rython.scheduler.register_recurring(_tick)


def _tick() -> None:
    global _collected, _pickup_entities

    if game_state.get_level() != 1:
        return
    if game_state.get_state() != game_state.PLAYING:
        return

    px, py, pz = player.get_position()
    remaining = []
    for entity in _pickup_entities:
        try:
            etf = entity.transform
            dx = px - etf.x
            dy = py - etf.y
            dz = pz - etf.z
            if dx * dx + dy * dy + dz * dz < 2.25:
                entity.despawn()
                game_state.add_score(100)
                rython.audio.play(
                    "game/assets/sounds/sfx/coin_pickup_01.ogg", "sfx", False
                )
                _collected += 1
                if _collected >= _pickups_total:
                    rython.audio.play(
                        "game/assets/music/jingle_levelup.ogg", "sfx", False
                    )
                    rython.scene.emit("level_complete")
            else:
                remaining.append(entity)
        except Exception:
            pass
    _pickup_entities[:] = remaining

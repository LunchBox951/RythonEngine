"""
levels/arena_2.py — Gauntlet Run.

Floating platforms over void, 3 skeletons, 3 pickups.
Theme: Orange / Dark.

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

    # No ground — void below. Floating platforms form a path.
    # Start platform
    lb.spawn_static_block(0.0, 0.0, 0.0, 6.0, 0.5, 6.0)

    # Platform chain across the void
    platform_data = [
        (8.0,  0.0,  0.0,  4.0, 0.5, 4.0),
        (16.0, 1.0,  0.0,  4.0, 0.5, 4.0),
        (16.0, 2.5, -8.0,  4.0, 0.5, 4.0),
        (8.0,  3.5, -14.0, 4.0, 0.5, 4.0),
        (0.0,  4.0, -18.0, 4.0, 0.5, 4.0),
        (-8.0, 4.0, -14.0, 6.0, 0.5, 6.0),  # end platform
    ]
    for x, y, z, w, h, d in platform_data:
        lb.spawn_static_block(x, y, z, w, h, d)

    # Decorative pillars
    lb.spawn_static_block(16.0, 2.25, -6.5)
    lb.spawn_static_block(16.0, 2.25, -9.5)

    # 3 score pickups (one per main platform)
    p1 = lb.spawn_pickup(8.0,  1.5,  0.0,  pickup_type="score", value=100)
    p2 = lb.spawn_pickup(16.0, 3.0, -8.0,  pickup_type="score", value=100)
    p3 = lb.spawn_pickup(-8.0, 5.5, -14.0, pickup_type="score", value=100)
    _pickup_entities.extend([p1, p2, p3])

    # 3 skeleton enemies on platforms
    skel_data = [
        (8.0,  1.0,  0.0),
        (16.0, 2.0,  0.0),
        (-8.0, 5.0, -14.0),
    ]
    for sx, sy, sz in skel_data:
        entity = lb.spawn_enemy(sx, sy, sz, enemy_type="skeleton", is_boss=False)
        enemies.register(entity, enemy_type="skeleton", is_boss=False)

    # Spawn player at start
    player.spawn(0.0, 2.0, 0.0)

    # Music
    rython.audio.play("game/assets/music/arena2.mp3", "music", True)
    rython.audio.set_volume("music", 0.7)

    if not _tick_registered:
        _tick_registered = True
        rython.scheduler.register_recurring(_tick)


def _tick() -> None:
    global _collected, _pickup_entities

    if game_state.get_level() != 2:
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

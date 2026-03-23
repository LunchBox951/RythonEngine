"""
levels/arena_3.py — Boss Arena.

Circular walled arena, lava pit (proximity damage), 5 + boss skeletons in waves.
Theme: Red / Purple.

Wave progression:
  Wave 1: 5 skeletons — active from level load.
  Wave 2: 5 skeletons + 1 boss — spawns 25 s after wave 1.

Level completion: collect all 3 pickups (scattered around the arena).
"""
from __future__ import annotations

import math
import rython
from game.scripts import game_state, player, enemies
from game.scripts import level_builder as lb

_pickup_entities: list = []
_pickups_total: int = 3
_collected: int = 0
_tick_registered: bool = False

_wave: int = 1
_wave_timer: float = 0.0
_wave2_spawned: bool = False
_WAVE2_DELAY: float = 25.0

_lava_damage_timer: float = 0.0
_prev_time: float = 0.0


def build() -> None:
    global _pickup_entities, _collected
    global _wave, _wave_timer, _wave2_spawned, _lava_damage_timer, _prev_time
    global _tick_registered

    _pickup_entities = []
    _collected = 0
    _wave = 1
    _wave_timer = 0.0
    _wave2_spawned = False
    _lava_damage_timer = 0.0
    _prev_time = rython.time.elapsed

    # Circular ground (approximated with a large square)
    lb.spawn_static_block(0.0, -0.5, 0.0, 22.0, 1.0, 22.0)

    # Circular perimeter walls (18 pillar segments at radius 11)
    _build_circular_wall(radius=11.0, y=0.0, segments=18, height=4.0)

    # Lava pit — static, visual only (damage handled by proximity in _tick)
    lb.spawn_static_block(0.0, 0.05, 0.0, 6.0, 0.1, 6.0, tags=["lava"])

    # 3 score pickups around the arena
    p1 = lb.spawn_pickup(7.0,  0.5,  0.0,  pickup_type="score", value=100)
    p2 = lb.spawn_pickup(-7.0, 0.5,  0.0,  pickup_type="score", value=100)
    p3 = lb.spawn_pickup(0.0,  0.5, -7.0,  pickup_type="score", value=100)
    _pickup_entities.extend([p1, p2, p3])

    # Wave 1: 5 skeletons around the arena perimeter
    _spawn_wave1()

    # Spawn player away from lava
    player.spawn(0.0, 2.0, 8.0)

    # Music (reuse arena2 track)
    rython.audio.play("game/assets/music/arena2.mp3", "music", True)
    rython.audio.set_volume("music", 0.7)

    if not _tick_registered:
        _tick_registered = True
        rython.scheduler.register_recurring(_tick)


def _build_circular_wall(radius: float, y: float, segments: int,
                         height: float) -> None:
    for i in range(segments):
        angle = (2.0 * math.pi * i) / segments
        wx = math.cos(angle) * radius
        wz = math.sin(angle) * radius
        lb.spawn_static_block(wx, y + height / 2.0, wz, 2.0, height, 2.0)


def _spawn_wave1() -> None:
    positions = [
        (7.0, 1.0, 7.0), (-7.0, 1.0, 7.0),
        (7.0, 1.0, -7.0), (-7.0, 1.0, -7.0),
        (0.0, 1.0, 8.0),
    ]
    for sx, sy, sz in positions:
        entity = lb.spawn_enemy(sx, sy, sz, enemy_type="skeleton", is_boss=False)
        enemies.register(entity, enemy_type="skeleton", is_boss=False)


def _spawn_wave2() -> None:
    positions = [
        (6.0, 1.0, 0.0), (-6.0, 1.0, 0.0),
        (0.0, 1.0, 6.0), (0.0, 1.0, -6.0),
        (4.0, 1.0, -4.0),
    ]
    for sx, sy, sz in positions:
        entity = lb.spawn_enemy(sx, sy, sz, enemy_type="skeleton", is_boss=False)
        enemies.register(entity, enemy_type="skeleton", is_boss=False)

    # Boss
    boss = lb.spawn_enemy(0.0, 1.0, -8.0, enemy_type="skeleton", is_boss=True)
    enemies.register(boss, enemy_type="skeleton", is_boss=True)
    rython.audio.play("game/assets/music/jingle_levelup.ogg", "sfx", False)


def _tick() -> None:
    global _collected, _pickup_entities
    global _wave, _wave_timer, _wave2_spawned
    global _lava_damage_timer, _prev_time

    if game_state.get_level() != 3:
        return
    if game_state.get_state() != game_state.PLAYING:
        return

    now = rython.time.elapsed
    dt = now - _prev_time if _prev_time > 0.0 else 0.0
    _prev_time = now
    if dt <= 0.0 or dt > 0.1:
        return

    # Wave 2: spawn after delay
    if _wave == 1 and not _wave2_spawned:
        _wave_timer += dt
        if _wave_timer >= _WAVE2_DELAY:
            _wave = 2
            _wave2_spawned = True
            _spawn_wave2()

    # Lava proximity damage (radius 3 from centre, y near ground)
    px, py, pz = player.get_position()
    if px * px + pz * pz < 9.0 and abs(py) < 1.5:
        _lava_damage_timer += dt
        if _lava_damage_timer >= 1.0:
            _lava_damage_timer = 0.0
            game_state.take_damage(5)
            rython.audio.play(
                "game/assets/sounds/sfx/impact_light_01.ogg", "sfx", False
            )
    else:
        _lava_damage_timer = 0.0

    # Pickup collection
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
                        "game/assets/music/jingle_win.ogg", "sfx", False
                    )
                    rython.scene.emit("level_complete")
            else:
                remaining.append(entity)
        except Exception:
            pass
    _pickup_entities[:] = remaining

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

_pickups_total: int = 3
_collected: int = 0

_TEX_PLATFORM = "game/assets/textures/Orange/orange_box.png"
_TEX_PILLAR = "game/assets/textures/Dark/dark_box.png"


def _on_collect(entity, **kwargs) -> None:
    global _collected
    if game_state.get_state() != game_state.PLAYING:
        return
    player_entity = player.get_entity()
    if player_entity is None:
        return
    entity_a = kwargs.get("entity_a")
    entity_b = kwargs.get("entity_b")
    entrant_id = entity_b if entity_a == entity.id else entity_a
    if entrant_id != player_entity.id:
        return
    entity.despawn()
    game_state.add_score(100)
    rython.audio.play("game/assets/sounds/sfx/coin_pickup_01.ogg", "sfx", False)
    _collected += 1
    if _collected >= _pickups_total:
        rython.audio.play("game/assets/music/jingle_levelup.ogg", "sfx", False)
        rython.scene.emit("level_complete")


def build() -> None:
    global _collected
    _collected = 0

    # Arena visual settings — deep midnight blue void, cold side-light from the left
    rython.renderer.set_clear_color(0.05, 0.06, 0.14, 1.0)
    rython.renderer.set_light_direction(-0.8, -0.5, 0.2)
    rython.renderer.set_light_color(0.85, 0.90, 1.0)
    rython.renderer.set_light_intensity(0.95)

    # No ground — void below. Floating platforms form a path.
    # Start platform
    lb.spawn_static_block(0.0, 0.0, 0.0, 6.0, 0.5, 6.0, texture=_TEX_PLATFORM)

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
        lb.spawn_static_block(x, y, z, w, h, d, texture=_TEX_PLATFORM)

    # Decorative pillars
    lb.spawn_static_block(16.0, 2.25, -6.5, texture=_TEX_PILLAR)
    lb.spawn_static_block(16.0, 2.25, -9.5, texture=_TEX_PILLAR)

    # 3 score pickups — subscribe trigger_enter for each
    p1 = lb.spawn_pickup(8.0,  1.5,  0.0,  pickup_type="score", value=100)
    p2 = lb.spawn_pickup(16.0, 3.0, -8.0,  pickup_type="score", value=100)
    p3 = lb.spawn_pickup(-8.0, 5.5, -14.0, pickup_type="score", value=100)
    for p in (p1, p2, p3):
        rython.scene.subscribe(f"trigger_enter:{p.id}", lambda entity=p, **kw: _on_collect(entity, **kw))

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

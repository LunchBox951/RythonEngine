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

_pickups_total: int = 3
_collected: int = 0

_TEX_FLOOR = "game/assets/textures/Light/light_floor_grid.png"
_TEX_WALL = "game/assets/textures/Light/light_wall.png"
_TEX_BOX = "game/assets/textures/Light/light_box.png"


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

    # Arena visual settings — pale blue-grey sky, warm overhead sun
    rython.renderer.set_clear_color(0.62, 0.65, 0.70, 1.0)
    rython.renderer.set_light_direction(0.3, -1.0, 0.4)
    rython.renderer.set_light_color(1.0, 0.96, 0.88)
    rython.renderer.set_light_intensity(1.1)

    # Ground — 20x20
    lb.spawn_static_block(0.0, -0.5, 0.0, 20.0, 1.0, 20.0, texture=_TEX_FLOOR)

    # Border walls
    lb.spawn_static_block(-10.0, 1.0, 0.0, 1.0, 2.0, 20.0, texture=_TEX_WALL)  # west
    lb.spawn_static_block(10.0, 1.0, 0.0, 1.0, 2.0, 20.0, texture=_TEX_WALL)   # east
    lb.spawn_static_block(0.0, 1.0, -10.0, 20.0, 2.0, 1.0, texture=_TEX_WALL)  # north
    lb.spawn_static_block(0.0, 1.0, 10.0, 20.0, 2.0, 1.0, texture=_TEX_WALL)   # south

    # 4 raised platforms
    lb.spawn_static_block(-5.0, 2.0, -5.0, 4.0, 0.5, 4.0, texture=_TEX_BOX)
    lb.spawn_static_block(5.0, 3.0, -5.0, 4.0, 0.5, 4.0, texture=_TEX_BOX)
    lb.spawn_static_block(-5.0, 4.0, 5.0, 4.0, 0.5, 4.0, texture=_TEX_BOX)
    lb.spawn_static_block(5.0, 2.5, 5.0, 4.0, 0.5, 4.0, texture=_TEX_BOX)

    # 3 score pickups — subscribe trigger_enter for each
    p1 = lb.spawn_pickup(-5.0, 2.5, -5.0, pickup_type="score", value=100)
    p2 = lb.spawn_pickup(5.0, 3.5, -5.0, pickup_type="score", value=100)
    p3 = lb.spawn_pickup(0.0, 0.5, 0.0, pickup_type="score", value=100)
    for p in (p1, p2, p3):
        rython.scene.subscribe(f"trigger_enter:{p.id}", lambda entity=p, **kw: _on_collect(entity, **kw))

    # 1 slow skeleton (patrol speed overridden via waypoints at a slower pace)
    entity = lb.spawn_enemy(3.0, 1.0, 3.0, enemy_type="skeleton", is_boss=False)
    enemies.register(entity, enemy_type="skeleton", is_boss=False)

    # Spawn player at centre
    player.spawn(0.0, 2.0, 0.0)

    # Music
    rython.audio.play("game/assets/music/arena1.mp3", "music", True)
    rython.audio.set_volume("music", 0.7)

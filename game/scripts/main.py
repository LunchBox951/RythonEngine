"""Entry point for Gauntlet of Cubes.

Load via: make run SCRIPT_DIR=. SCRIPT=game.scripts.main
"""
import rython
from game.scripts import game_state
from game.scripts import player, camera_follow, enemies, level_builder
from game.scripts.menus import main_menu, pause_menu, hud, settings_menu

_prev_time: float = 0.0


def init() -> None:
    """Called once by the engine at startup."""
    global _prev_time
    rython.physics.set_gravity(0, -20, 0)
    _prev_time = rython.time.elapsed

    rython.scene.subscribe("start_game", _on_start_game)
    rython.scene.subscribe("load_level", _on_load_level)
    rython.scene.subscribe("player_died", _on_player_died)
    rython.scene.subscribe("level_complete", _on_level_complete)
    rython.scene.subscribe("enemy_attack", _on_enemy_attack)
    rython.scene.subscribe("input:pressed", _on_input_pressed)

    main_menu.create()
    pause_menu.create()
    settings_menu.create()
    hud.create()

    rython.scheduler.register_recurring(_game_tick)


def _game_tick() -> None:
    """Main game loop — called every frame."""
    global _prev_time
    now = rython.time.elapsed
    dt = now - _prev_time
    _prev_time = now

    if dt <= 0 or dt > 0.5:
        return

    state = game_state.get_state()

    if state == game_state.PLAYING:
        player.update(dt)
        camera_follow.update(dt)
        enemies.update(dt)
        hud.update()


def _on_input_pressed(**kwargs) -> None:
    action = kwargs.get("action", "")
    if action != "pause":
        return
    state = game_state.get_state()
    if state == game_state.PLAYING:
        game_state.set_state(game_state.PAUSED)
        pause_menu.show()
    elif state == game_state.PAUSED:
        game_state.set_state(game_state.PLAYING)
        pause_menu.hide()


def _on_start_game(**kwargs) -> None:
    game_state.reset_all()
    game_state.set_state(game_state.PLAYING)
    hud.show()
    rython.scene.emit("load_level", level=1)


def _on_load_level(**kwargs) -> None:
    level_num = kwargs.get("level", 1)
    game_state.set_level(level_num)
    game_state.reset_for_level()

    rython.audio.stop_category("music")
    level_builder.clear_level()
    enemies.clear()

    if level_num == 1:
        from game.scripts.levels import arena_1 as arena
    elif level_num == 2:
        from game.scripts.levels import arena_2 as arena
    else:
        from game.scripts.levels import arena_3 as arena

    arena.build()
    camera_follow.init()


def _on_player_died(**kwargs) -> None:
    player.respawn()
    game_state.set_health(game_state.get_max_health())


def _on_level_complete(**kwargs) -> None:
    level = game_state.get_level()
    if level < 3:
        rython.scene.emit("load_level", level=level + 1)
    else:
        game_state.set_state(game_state.MENU)
        hud.hide()
        rython.renderer.draw_text(
            "YOU WIN!", x=0.35, y=0.45, size=48, r=255, g=255, b=100
        )
        main_menu.show()


def _on_enemy_attack(**kwargs) -> None:
    damage = kwargs.get("damage", 10)
    game_state.take_damage(damage)
    rython.audio.play("game/assets/sounds/sfx/impact_light_01.ogg", "sfx")

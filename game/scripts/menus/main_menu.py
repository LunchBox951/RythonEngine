"""Main menu for Gauntlet of Cubes — includes CC-BY attribution for Eric Taylor music."""
import rython
from game.scripts import game_state
from typing import Optional

_panel_id: Optional[int] = None
_music_handle: int = -1


def create() -> None:
    """Build main menu widgets and start menu music."""
    global _panel_id

    panel = rython.ui.create_panel(0.0, 0.0, 1.0, 1.0)

    title = rython.ui.create_label("GAUNTLET OF CUBES", 0.3, 0.1, 0.4, 0.1)
    # CC-BY 3.0 required attribution for Eric Taylor music
    credit = rython.ui.create_label("Music by Eric Taylor", 0.35, 0.22, 0.3, 0.05)
    play_btn = rython.ui.create_button("Play Game", 0.35, 0.35, 0.3, 0.08)
    settings_btn = rython.ui.create_button("Settings", 0.35, 0.45, 0.3, 0.08)
    quit_btn = rython.ui.create_button("Quit", 0.35, 0.55, 0.3, 0.08)

    rython.ui.add_child(panel, title)
    rython.ui.add_child(panel, credit)
    rython.ui.add_child(panel, play_btn)
    rython.ui.add_child(panel, settings_btn)
    rython.ui.add_child(panel, quit_btn)

    rython.ui.on_click(play_btn, _on_play)
    rython.ui.on_click(settings_btn, _on_settings)
    rython.ui.on_click(quit_btn, _on_quit)

    _panel_id = panel
    show()


def _on_play() -> None:
    print('[DEBUG main_menu._on_play] entry')
    rython.audio.play("game/assets/sounds/ui/confirm_01.ogg", "sfx")
    hide()
    print('[DEBUG main_menu._on_play] before scene.emit(start_game)')
    rython.scene.emit("start_game")
    print('[DEBUG main_menu._on_play] after scene.emit(start_game) — returned')


def _on_settings() -> None:
    print(f'[DEBUG main_menu._on_settings] entry, _panel_id={_panel_id}')
    from game.scripts.menus import settings_menu
    rython.audio.play("game/assets/sounds/ui/click_01.ogg", "sfx")
    hide()
    print('[DEBUG main_menu._on_settings] before settings_menu.show()')
    settings_menu.show()
    print('[DEBUG main_menu._on_settings] after settings_menu.show()')
    game_state.set_state(game_state.SETTINGS)


def _on_quit() -> None:
    rython.audio.play("game/assets/sounds/ui/click_01.ogg", "sfx")
    rython.engine.request_quit()


def show() -> None:
    print(f'[DEBUG main_menu.show] entry, _panel_id={_panel_id}')
    global _music_handle
    if _panel_id is not None:
        rython.ui.show(_panel_id)
    if _music_handle == -1:
        _music_handle = rython.audio.play(
            "game/assets/music/menu.mp3", "music", looping=True
        )


def hide() -> None:
    print(f'[DEBUG main_menu.hide] entry, _panel_id={_panel_id}')
    global _music_handle
    if _panel_id is not None:
        rython.ui.hide(_panel_id)
    if _music_handle != -1:
        rython.audio.stop(_music_handle)
        _music_handle = -1

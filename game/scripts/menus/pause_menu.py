"""Pause menu — shown when Escape is pressed during gameplay."""
import rython
from game.scripts import game_state
from game.scripts.ui_loader import load_layout
from typing import Optional

_panel_id: Optional[int] = None


def create() -> None:
    """Build the pause menu widgets from UI layout (hidden initially)."""
    global _panel_id

    widgets = load_layout("game/ui/pause_menu.json")

    _panel_id = widgets["PausePanel"]
    resume_btn = widgets["ResumeButton"]
    menu_btn = widgets["MainMenuButton"]

    rython.ui.on_click(resume_btn, _on_resume)
    rython.ui.on_click(menu_btn, _on_main_menu)

    rython.ui.hide(_panel_id)


def _on_resume() -> None:
    hide()
    game_state.set_state(game_state.PLAYING)
    rython.audio.play("game/assets/sounds/ui/click_01.ogg", "sfx")


def _on_main_menu() -> None:
    from game.scripts import level_builder, enemies
    from game.scripts.menus import main_menu
    from game.scripts.menus import hud
    rython.audio.play("game/assets/sounds/ui/click_01.ogg", "sfx")
    hide()
    hud.hide()
    level_builder.clear_level()
    enemies.clear()
    game_state.set_state(game_state.MENU)
    main_menu.show()


def show() -> None:
    if _panel_id is not None:
        rython.ui.show(_panel_id)


def hide() -> None:
    if _panel_id is not None:
        rython.ui.hide(_panel_id)

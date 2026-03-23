"""Pause menu — shown when Escape is pressed during gameplay."""
import rython
from game.scripts import game_state
from typing import Optional

_panel_id: Optional[int] = None


def create() -> None:
    """Build the pause menu widgets (hidden initially)."""
    global _panel_id
    panel = rython.ui.create_panel(0.3, 0.25, 0.4, 0.5)

    title = rython.ui.create_label("PAUSED", 0.0, 0.0, 1.0, 0.12)
    resume_btn = rython.ui.create_button("Resume", 0.0, 0.0, 1.0, 0.12)
    menu_btn = rython.ui.create_button("Main Menu", 0.0, 0.0, 1.0, 0.12)

    rython.ui.add_child(panel, title)
    rython.ui.add_child(panel, resume_btn)
    rython.ui.add_child(panel, menu_btn)
    rython.ui.set_layout(panel, "vertical", 0.02, 0.02)

    rython.ui.on_click(resume_btn, _on_resume)
    rython.ui.on_click(menu_btn, _on_main_menu)

    rython.ui.hide(panel)
    _panel_id = panel


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

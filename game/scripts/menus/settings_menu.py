"""Settings menu — volume controls, accessible from main menu."""
import rython
from game.scripts import game_state
from typing import Optional

_panel_id: Optional[int] = None
_music_vol_label_id: Optional[int] = None
_sfx_vol_label_id: Optional[int] = None

_music_vol: int = 80
_sfx_vol: int = 100


def create() -> None:
    """Build settings menu widgets (hidden initially)."""
    global _panel_id, _music_vol_label_id, _sfx_vol_label_id

    panel = rython.ui.create_panel(0.25, 0.15, 0.5, 0.7)

    title = rython.ui.create_label("SETTINGS", 0.0, 0.0, 1.0, 0.1)
    rython.ui.add_child(panel, title)

    # Music volume row
    music_section = rython.ui.create_panel(0.0, 0.0, 1.0, 0.12)
    music_label = rython.ui.create_label("Music Vol:", 0.0, 0.0, 0.35, 1.0)
    music_minus = rython.ui.create_button("-", 0.0, 0.0, 0.12, 1.0)
    music_vol_lbl = rython.ui.create_label(f"{_music_vol}%", 0.0, 0.0, 0.22, 1.0)
    music_plus = rython.ui.create_button("+", 0.0, 0.0, 0.12, 1.0)
    rython.ui.add_child(music_section, music_label)
    rython.ui.add_child(music_section, music_minus)
    rython.ui.add_child(music_section, music_vol_lbl)
    rython.ui.add_child(music_section, music_plus)
    rython.ui.add_child(panel, music_section)
    _music_vol_label_id = music_vol_lbl

    # SFX volume row
    sfx_section = rython.ui.create_panel(0.0, 0.0, 1.0, 0.12)
    sfx_label = rython.ui.create_label("SFX Vol:", 0.0, 0.0, 0.35, 1.0)
    sfx_minus = rython.ui.create_button("-", 0.0, 0.0, 0.12, 1.0)
    sfx_vol_lbl = rython.ui.create_label(f"{_sfx_vol}%", 0.0, 0.0, 0.22, 1.0)
    sfx_plus = rython.ui.create_button("+", 0.0, 0.0, 0.12, 1.0)
    rython.ui.add_child(sfx_section, sfx_label)
    rython.ui.add_child(sfx_section, sfx_minus)
    rython.ui.add_child(sfx_section, sfx_vol_lbl)
    rython.ui.add_child(sfx_section, sfx_plus)
    rython.ui.add_child(panel, sfx_section)
    _sfx_vol_label_id = sfx_vol_lbl

    # Back button
    back_btn = rython.ui.create_button("Back", 0.0, 0.0, 1.0, 0.1)
    rython.ui.add_child(panel, back_btn)

    rython.ui.on_click(music_minus, _music_down)
    rython.ui.on_click(music_plus, _music_up)
    rython.ui.on_click(sfx_minus, _sfx_down)
    rython.ui.on_click(sfx_plus, _sfx_up)
    rython.ui.on_click(back_btn, _on_back)

    rython.ui.hide(panel)
    _panel_id = panel

    # Apply initial volumes
    rython.audio.set_volume("music", _music_vol / 100.0)
    rython.audio.set_volume("sfx", _sfx_vol / 100.0)


def _music_down() -> None:
    global _music_vol
    _music_vol = max(0, _music_vol - 10)
    rython.audio.set_volume("music", _music_vol / 100.0)
    if _music_vol_label_id is not None:
        rython.ui.set_text(_music_vol_label_id, f"{_music_vol}%")


def _music_up() -> None:
    global _music_vol
    _music_vol = min(100, _music_vol + 10)
    rython.audio.set_volume("music", _music_vol / 100.0)
    if _music_vol_label_id is not None:
        rython.ui.set_text(_music_vol_label_id, f"{_music_vol}%")


def _sfx_down() -> None:
    global _sfx_vol
    _sfx_vol = max(0, _sfx_vol - 10)
    rython.audio.set_volume("sfx", _sfx_vol / 100.0)
    if _sfx_vol_label_id is not None:
        rython.ui.set_text(_sfx_vol_label_id, f"{_sfx_vol}%")


def _sfx_up() -> None:
    global _sfx_vol
    _sfx_vol = min(100, _sfx_vol + 10)
    rython.audio.set_volume("sfx", _sfx_vol / 100.0)
    if _sfx_vol_label_id is not None:
        rython.ui.set_text(_sfx_vol_label_id, f"{_sfx_vol}%")


def _on_back() -> None:
    from game.scripts.menus import main_menu
    rython.audio.play("game/assets/sounds/ui/click_01.ogg", "sfx")
    hide()
    game_state.set_state(game_state.MENU)
    main_menu.show()


def show() -> None:
    if _panel_id is not None:
        rython.ui.show(_panel_id)


def hide() -> None:
    if _panel_id is not None:
        rython.ui.hide(_panel_id)

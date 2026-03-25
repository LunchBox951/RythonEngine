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
    """Build settings menu widgets from UI layout (hidden initially)."""
    global _panel_id, _music_vol_label_id, _sfx_vol_label_id

    widgets = rython.ui.load_layout("game/ui/settings_menu.json")

    _panel_id = widgets["SettingsPanel"]
    _music_vol_label_id = widgets["MusicVolLabel"]
    _sfx_vol_label_id = widgets["SfxVolLabel"]

    rython.ui.on_click(widgets["MusicMinusButton"], _music_down)
    rython.ui.on_click(widgets["MusicPlusButton"], _music_up)
    rython.ui.on_click(widgets["SfxMinusButton"], _sfx_down)
    rython.ui.on_click(widgets["SfxPlusButton"], _sfx_up)
    rython.ui.on_click(widgets["BackButton"], _on_back)

    rython.ui.hide(_panel_id)

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

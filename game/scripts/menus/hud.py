"""Heads-up display: health, score, level indicator."""
import rython
from game.scripts import game_state

_visible: bool = False


def create() -> None:
    """Initialize the HUD (starts hidden until gameplay begins)."""
    global _visible
    _visible = False


@rython.throttle(hz=10)
def update() -> None:
    """Draw HUD elements; called every frame while PLAYING."""
    if not _visible:
        return
    health = game_state.get_health()
    max_hp = game_state.get_max_health()
    score = game_state.get_score()
    level = game_state.get_level()
    rython.renderer.draw_text(
        f"HP: {health}/{max_hp}", x=0.02, y=0.02, size=20, r=255, g=50, b=50
    )
    rython.renderer.draw_text(
        f"Score: {score}", x=0.02, y=0.07, size=18, r=255, g=255, b=100
    )
    rython.renderer.draw_text(
        f"Level {level}", x=0.85, y=0.02, size=18, r=200, g=200, b=200
    )


def show() -> None:
    global _visible
    _visible = True


def hide() -> None:
    global _visible
    _visible = False

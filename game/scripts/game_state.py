"""Game state machine for Gauntlet of Cubes."""
import rython

MENU = "MENU"
PLAYING = "PLAYING"
PAUSED = "PAUSED"
SETTINGS = "SETTINGS"

_state: str = MENU
_current_level: int = 1
_score: int = 0
_health: int = 100
_max_health: int = 100


def get_state() -> str:
    return _state


def set_state(s: str) -> None:
    global _state
    _state = s


def get_level() -> int:
    return _current_level


def set_level(n: int) -> None:
    global _current_level
    _current_level = n


def get_score() -> int:
    return _score


def add_score(n: int) -> None:
    global _score
    _score += n


def get_health() -> int:
    return _health


def set_health(n: int) -> None:
    global _health
    _health = max(0, min(_max_health, n))


def get_max_health() -> int:
    return _max_health


def take_damage(n: int) -> None:
    global _health
    _health = max(0, _health - n)
    if _health == 0:
        rython.scene.emit("player_died")


def reset_for_level() -> None:
    """Reset health to max, keep score."""
    global _health
    _health = _max_health


def reset_all() -> None:
    """Full reset: health, score, level back to 1."""
    global _state, _current_level, _score, _health
    _current_level = 1
    _score = 0
    _health = _max_health

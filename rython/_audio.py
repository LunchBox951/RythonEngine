"""AudioBridge stub for IDE support."""
from __future__ import annotations


class AudioBridge:
    """Audio playback bridge (``rython.audio``)."""

    def play(self, path: str, category: str = "sfx", looping: bool = False) -> int:
        """Play a sound. Returns an integer handle for :meth:`stop`."""
        raise NotImplementedError

    def stop(self, handle: int) -> None:
        """Stop a playing sound by handle. Idempotent."""
        raise NotImplementedError

    def stop_category(self, category: str) -> None:
        """Stop all sounds in a category."""
        raise NotImplementedError

    def set_volume(self, category: str, volume: float) -> None:
        """Set volume for a category (0.0 -- 1.0)."""
        raise NotImplementedError

    def set_master_volume(self, volume: float) -> None:
        """Set master volume (0.0 -- 1.0)."""
        raise NotImplementedError

    def __repr__(self) -> str:
        return "rython.audio"

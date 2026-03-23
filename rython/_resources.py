"""ResourcesBridge stub for IDE support."""
from __future__ import annotations

from typing import Optional


class AssetHandle:
    """Reference-counted handle to an in-flight or completed asset load."""

    @property
    def is_ready(self) -> bool:
        """True when the asset has finished loading successfully."""
        raise NotImplementedError

    @property
    def is_pending(self) -> bool:
        """True when the asset is still being decoded in the background."""
        raise NotImplementedError

    @property
    def is_failed(self) -> bool:
        """True when the asset failed to load."""
        raise NotImplementedError

    @property
    def error(self) -> Optional[str]:
        """Error message if the handle is failed, otherwise None."""
        raise NotImplementedError

    def __repr__(self) -> str:
        return "AssetHandle(...)"


class ResourcesBridge:
    """Asset loading bridge (``rython.resources``)."""

    def load_image(self, path: str) -> AssetHandle:
        """Begin loading an image file. Returns an :class:`AssetHandle`."""
        raise NotImplementedError

    def load_mesh(self, path: str) -> AssetHandle:
        """Begin loading a glTF mesh file. Returns an :class:`AssetHandle`."""
        raise NotImplementedError

    def load_sound(self, path: str) -> AssetHandle:
        """Begin loading a WAV sound file. Returns an :class:`AssetHandle`."""
        raise NotImplementedError

    def load_font(self, path: str, size: float = 16.0) -> AssetHandle:
        """Begin loading a font file at *size* px. Returns an :class:`AssetHandle`."""
        raise NotImplementedError

    def load_spritesheet(self, path: str, cols: int = 1, rows: int = 1) -> AssetHandle:
        """Begin loading a spritesheet image split into *cols* × *rows* frames.
        Returns an :class:`AssetHandle`."""
        raise NotImplementedError

    def memory_used_mb(self) -> float:
        """Bytes currently occupied by decoded assets, in megabytes."""
        raise NotImplementedError

    def memory_budget_mb(self) -> float:
        """Configured LRU eviction budget, in megabytes."""
        raise NotImplementedError

    def __repr__(self) -> str:
        return "rython.resources"

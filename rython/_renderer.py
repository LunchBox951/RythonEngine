"""
Pure-Python type stub for the rython RendererBridge.
"""

from __future__ import annotations


class RendererBridge:
    """
    Renderer bridge exposed as ``rython.renderer``.

    Draw calls queued here are flushed by the engine each frame.
    """

    def draw_text(
        self,
        text: str,
        font_id: str = "default",
        x: float = 0.5,
        y: float = 0.1,
        size: int = 16,
        r: int = 255,
        g: int = 255,
        b: int = 255,
        z: float = 0.0,
    ) -> None:
        """
        Queue a text draw command for the current frame.

        *x* and *y* are normalised screen coordinates [0, 1].
        *r*, *g*, *b* are 0-255 colour channels.
        *z* controls render ordering (higher = drawn on top).
        """
        raise NotImplementedError

    def __repr__(self) -> str:
        raise NotImplementedError

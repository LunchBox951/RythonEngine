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

    def set_clear_color(
        self, r: float, g: float, b: float, a: float = 1.0
    ) -> None:
        """Set the framebuffer clear color (linear RGBA, each component [0, 1])."""
        raise NotImplementedError

    def set_light_direction(self, x: float, y: float, z: float) -> None:
        """Set the directional light world-space direction (auto-normalized)."""
        raise NotImplementedError

    def set_light_color(self, r: float, g: float, b: float) -> None:
        """Set the directional light RGB color (linear [0, 1])."""
        raise NotImplementedError

    def set_light_intensity(self, intensity: float) -> None:
        """Set the directional light intensity multiplier."""
        raise NotImplementedError

    def set_ambient_light(
        self,
        r: float = 0.1,
        g: float = 0.1,
        b: float = 0.1,
        intensity: float = 1.0,
    ) -> None:
        """Set the scene-wide ambient light color and intensity (linear RGB)."""
        raise NotImplementedError

    def set_shadow_enabled(self, enabled: bool) -> None:
        """Enable or disable shadow casting from the primary directional light."""
        raise NotImplementedError

    def set_shadow_map_size(self, size: int) -> None:
        """Set the shadow map resolution in pixels (512, 1024, 2048, or 4096)."""
        raise NotImplementedError

    def set_shadow_bias(self, bias: float) -> None:
        """Set the shadow depth bias (prevents shadow acne). Default: 0.005."""
        raise NotImplementedError

    def set_shadow_pcf(self, samples: int) -> None:
        """Set PCF sample count: 1 = no filtering, >= 4 = 3x3 kernel. Default: 4."""
        raise NotImplementedError

    def __repr__(self) -> str:
        raise NotImplementedError

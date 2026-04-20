"""
Pure-Python type stub for the rython RendererBridge.
"""

from __future__ import annotations

from rython._resources import AssetHandle


class RendererBridge:
    """
    Renderer bridge exposed as ``rython.renderer``.

    Draw calls queued here are flushed by the engine each frame.

    Built-in mesh ids pre-uploaded by the engine: ``"cube"``, ``"sphere"``.
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

    def register_mesh(self, mesh_id: str, handle: AssetHandle) -> None:
        """Register a loaded mesh handle into the renderer's mesh cache.

        The upload is **lazy**: the handle is pushed into a pending queue and the
        actual GPU upload happens once the handle transitions to ``Ready``.  If
        the handle is still ``Pending`` at drain time it is automatically requeued
        for the next frame.  If it has ``Failed`` it is dropped with a warning.

        After a successful upload the mesh can be referenced by ``mesh_id`` in
        draw calls and spawned entities.

        Built-in ids ``"cube"`` and ``"sphere"`` are reserved and cannot be
        overwritten — passing either raises ``ValueError``.

        In **headless mode** (``--headless``) there is no GPU renderer, so
        registrations are drained and dropped each frame; Ready registrations
        are logged at ``warn!`` level so the divergence is visible in CI logs.

        Args:
            mesh_id: Unique identifier string for the mesh.  Must be non-empty
                and must not be ``"cube"`` or ``"sphere"``.
            handle: An ``AssetHandle`` returned by ``rython.resources.load_mesh()``.

        Raises:
            ValueError: If ``mesh_id`` is empty or is a reserved built-in id.
        """
        raise NotImplementedError

    def __repr__(self) -> str:
        raise NotImplementedError

"""UIBridge stub for IDE support."""
from __future__ import annotations

from typing import Callable, Dict, Optional, Tuple


class UIBridge:
    """UI system bridge (``rython.ui``)."""

    def create_label(self, text: str, x: float, y: float, w: float, h: float) -> int:
        """Create a label widget at normalized screen position. Returns the widget ID."""
        raise NotImplementedError

    def create_button(self, text: str, x: float, y: float, w: float, h: float) -> int:
        """Create a button widget at normalized screen position. Returns the widget ID."""
        raise NotImplementedError

    def create_panel(self, x: float, y: float, w: float, h: float) -> int:
        """Create a panel container widget. Returns the widget ID."""
        raise NotImplementedError

    def create_text_input(
        self, placeholder: str, x: float, y: float, w: float, h: float
    ) -> int:
        """Create a text input widget. Returns the widget ID."""
        raise NotImplementedError

    def add_child(self, parent: int, child: int) -> None:
        """Attach *child* widget as a child of *parent*."""
        raise NotImplementedError

    def set_layout(
        self, id: int, direction: str, spacing: float, padding: float
    ) -> None:
        """Set layout direction for a container widget.

        *direction* must be ``"none"``, ``"vertical"``, or ``"horizontal"``.
        *spacing* is the gap between children; *padding* is inner padding.
        """
        raise NotImplementedError

    def show(self, id: int) -> None:
        """Make the widget visible."""
        raise NotImplementedError

    def hide(self, id: int) -> None:
        """Hide the widget."""
        raise NotImplementedError

    def is_visible(self, id: int) -> bool:
        """``True`` if the widget and all its ancestors are visible."""
        raise NotImplementedError

    def set_text(self, id: int, text: str) -> None:
        """Set the display text of a widget (label, button label, or text input value)."""
        raise NotImplementedError

    def on_click(self, id: int, callback: Callable[[], None]) -> None:
        """Register *callback* as the click handler for a button widget.

        The callback is called with no arguments when the button is clicked.
        """
        raise NotImplementedError

    def set_theme(
        self,
        *,
        button_color: Optional[Tuple[int, int, int]] = None,
        text_color: Optional[Tuple[int, int, int]] = None,
        panel_color: Optional[Tuple[int, int, int]] = None,
        border_color: Optional[Tuple[int, int, int]] = None,
        font_size: Optional[int] = None,
    ) -> None:
        """Apply a partial theme override. Unspecified fields keep their current value.

        Colors are ``(r, g, b)`` tuples with values 0–255.
        """
        raise NotImplementedError

    def load_layout(self, path: str) -> Dict[str, int]:
        """Load a UI layout from an editor JSON file (additive).

        Applies the file's theme, creates all widgets with fresh runtime IDs,
        sets all visual properties, and wires parent-child relationships.

        Returns a dict mapping widget name to runtime widget ID.
        """
        raise NotImplementedError

    def __repr__(self) -> str:
        return "rython.ui"

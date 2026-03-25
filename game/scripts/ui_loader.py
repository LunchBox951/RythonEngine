"""Load UI layouts from editor JSON files using the Rust bridge."""
import rython


def load_layout(path: str) -> dict[str, int]:
    """Load a UI layout JSON file and create all widgets via the Rust bridge.

    Returns a dict mapping widget name -> runtime widget ID,
    so callers can wire up callbacks and references by name.
    """
    return rython.ui.load_layout(path)

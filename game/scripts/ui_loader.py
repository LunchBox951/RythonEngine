"""Load UI layouts from editor JSON files and create widgets via rython.ui."""
import json
import rython


def load_layout(path: str) -> dict[str, int]:
    """Load a UI layout JSON file and create all widgets.

    Returns a dict mapping widget name -> runtime widget ID,
    so callers can wire up callbacks and references by name.
    """
    with open(path, "r") as f:
        doc = json.load(f)

    widgets = doc.get("widgets", [])
    name_to_id: dict[str, int] = {}
    json_id_to_runtime_id: dict[int, int] = {}

    # First pass: create all widgets (no parenting yet)
    for w in widgets:
        kind = w["kind"]
        x, y = w["x"], w["y"]
        width, height = w["w"], w["h"]
        text = w.get("text", "")

        if kind == "Panel":
            rid = rython.ui.create_panel(x, y, width, height)
        elif kind == "Label":
            rid = rython.ui.create_label(text, x, y, width, height)
        elif kind == "Button":
            rid = rython.ui.create_button(text, x, y, width, height)
        elif kind == "TextInput":
            placeholder = w.get("placeholder", "")
            rid = rython.ui.create_text_input(placeholder, x, y, width, height)
        else:
            continue

        json_id_to_runtime_id[w["id"]] = rid
        name_to_id[w["name"]] = rid

    # Second pass: parent-child relationships
    for w in widgets:
        rid = json_id_to_runtime_id.get(w["id"])
        if rid is None:
            continue
        for child_json_id in w.get("children", []):
            child_rid = json_id_to_runtime_id.get(child_json_id)
            if child_rid is not None:
                rython.ui.add_child(rid, child_rid)

    # Third pass: layout settings
    for w in widgets:
        rid = json_id_to_runtime_id.get(w["id"])
        if rid is None:
            continue
        layout = w.get("layout", "None")
        if layout in ("Vertical", "Horizontal"):
            theme = doc.get("theme", {})
            spacing = theme.get("spacing", 0.0)
            padding = theme.get("padding", 0.0)
            direction = layout.lower()
            rython.ui.set_layout(rid, direction, spacing, padding)

    return name_to_id

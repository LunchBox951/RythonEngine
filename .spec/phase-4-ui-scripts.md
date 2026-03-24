# Phase 4: UI Editor + Script Scaffolding

**Goal:** Visual UI layout editing and Python script generation. This phase completes
the MVP.

**Result:** Users can design game UIs visually (widget trees, themes, layout), generate
Python script boilerplate, and associate scripts with entities.

**Depends on:** Phase 3 (asset browser, gizmos, full scene editing)

---

## 1. Engine Change: `UIManager` Serialization

**File:** `crates/rython-ui/src/manager.rs`

The current `UIManager` has no save/load capability — widget trees are built
programmatically in Python scripts. The UI editor needs JSON persistence.

### `UIManager::save_json() -> serde_json::Value`

Serialize the widget tree:

```json
{
  "theme": {
    "font_id": "default",
    "font_size": 16,
    "text_color": [255, 255, 255, 255],
    "button_color": [80, 80, 80, 255],
    ...
  },
  "widgets": [
    {
      "id": 1,
      "kind": "Panel",
      "x": 0.0, "y": 0.0, "w": 0.3, "h": 1.0,
      "color": [40, 40, 40, 255],
      "visible": true,
      "layout": "Vertical",
      "parent": null,
      "children": [2, 3]
    },
    {
      "id": 2,
      "kind": "Label",
      "text": "Score: 0",
      "font_id": "default",
      "font_size": 24,
      "parent": 1,
      "children": []
    }
  ]
}
```

Each widget record includes all properties relevant to its `WidgetKind`. Animations are
not serialized (they are runtime behavior defined in scripts).

### `UIManager::load_json(data: &Value)`

Clear the widget tree, then reconstruct all widgets from JSON. Restore parent-child
relationships. Advance the internal widget ID counter past the max loaded ID (similar to
`EntityId::ensure_counter_past()`).

### Widget Properties to Serialize

Per `WidgetKind`:

| Kind | Properties |
|---|---|
| All | id, kind, x, y, w, h, color, visible, parent, children, layout, z_order |
| Label | text, font_id, font_size, text_color |
| Button | text, font_id, font_size, text_color, hover_color, active_color |
| TextInput | text, font_id, font_size, placeholder, cursor_pos |
| Panel | border_color, border_width |
| ScrollView | scroll_offset, content_height |

---

## 2. UI Editor Panel

### `src/panels/ui_editor.rs`

The UI editor is a dedicated panel (or a tab alongside the scene editor) with three
sub-areas:

```
┌─ UI Editor ──────────────────────────────────────────────┐
│ ┌─ Widget Tree ───┐ ┌─ Preview ──────┐ ┌─ Properties ─┐ │
│ │ Panel "HUD"     │ │                │ │ Kind: Label  │ │
│ │  ├ Label "Score"│ │  ┌──────────┐  │ │ Text: Score  │ │
│ │  ├ Button "Menu"│ │  │ Score: 0 │  │ │ Font: default│ │
│ │  └ Panel "Mini" │ │  │ [Menu]   │  │ │ Size: 24     │ │
│ │    └ Label "Map"│ │  │ ┌──────┐ │  │ │ X: 0.02      │ │
│ │                 │ │  │ │ Map  │ │  │ │ Y: 0.02      │ │
│ │ [+ Add Widget]  │ │  │ └──────┘ │  │ │ Color: ...   │ │
│ │                 │ │  └──────────┘  │ │              │ │
│ └─────────────────┘ └────────────────┘ └──────────────┘ │
└──────────────────────────────────────────────────────────┘
```

### Widget Tree (left)

- Displays the widget hierarchy as an egui `TreeNode` tree
- Click to select a widget (updates `SelectionState::Widget(id)`)
- Right-click context menu:
  - Add child widget (Label, Button, TextInput, Panel, ScrollView)
  - Delete widget (and all children)
  - Duplicate widget
  - Move up / Move down (reorder among siblings)
- Drag-and-drop to reparent widgets

### Preview (center)

Renders the UI layout into a small offscreen texture:

1. Create a separate `UIManager` instance for the editor (not shared with any runtime)
2. Call `ui_manager.compute_layout()` to position all widgets
3. Call `ui_manager.build_draw_commands()` to generate `DrawRect`, `DrawText`, etc.
4. Render these draw commands into an offscreen texture using the existing renderer's
   `render_rects()` and `render_text()` methods
5. Display the texture as an `egui::Image`

The preview updates whenever a widget property changes.

### Properties (right)

When a widget is selected, show editable fields based on its `WidgetKind`:

| Widget Kind | Editable Properties |
|---|---|
| **All** | x, y, w, h (drag values), color (color picker), visible (checkbox), layout direction (dropdown) |
| **Label** | text (text input), font_id, font_size, text_color |
| **Button** | text, font_id, font_size, text_color, hover_color, active_color |
| **TextInput** | placeholder text, font_id, font_size |
| **Panel** | border_color, border_width |
| **ScrollView** | content_height |

Property changes push UI-specific undo commands (same command pattern as scene editing but
targeting `UIManager` instead of `Scene`).

### Theme Editor

A collapsible section at the top of the properties panel for editing the `Theme`:

- Font ID (text input)
- Font size (drag value)
- Text color (color picker)
- Button colors: normal, hover, active (color pickers)
- Panel color (color picker)
- Border color + width
- Padding, spacing (drag values)

Theme changes update the preview immediately.

### File I/O

| Action | Behavior |
|---|---|
| Save UI | Call `ui_manager.save_json()`, write to `ui/<name>.json` |
| Load UI | Read `ui/<name>.json`, call `ui_manager.load_json()` |
| New UI | Reset `UIManager` to empty, prompt for filename |

The project's `ui/` directory can contain multiple UI definitions (e.g., `hud.json`,
`main_menu.json`, `pause.json`).

---

## 3. Script Scaffolding

### `src/project/scaffold.rs`

Generates Python script files from templates. The editor creates the files; the user
edits them in their preferred IDE.

### Templates

#### Basic module (`main.py`):

```python
"""{{project_name}} — entry point.

Called by the engine on startup.
"""
import rython


def init():
    """Called once when the scripting module is loaded."""
    rython.camera.set_position(0.0, 5.0, -10.0)
    rython.camera.set_look_at(0.0, 0.0, 0.0)

    # Load scene entities here, or let the engine load from scene JSON.
    pass
```

#### Script class:

```python
"""{{class_name}} — entity script.

Attach to an entity with:
    rython.scene.attach_script(entity, {{class_name}})
"""
import rython


class {{class_name}}:
    def __init__(self, entity):
        self.entity = entity

    def on_spawn(self):
        pass

    def on_despawn(self):
        pass

    def on_collision(self, other_entity, normal_vec):
        pass

    def on_trigger_enter(self, other_entity):
        pass

    def on_trigger_exit(self, other_entity):
        pass

    def on_input_action(self, action_name, value):
        pass
```

#### Per-frame update:

```python
"""{{module_name}} — per-frame update logic."""
import rython


def init():
    rython.scheduler.register_recurring(on_tick)


def on_tick():
    t = rython.time.elapsed
    # Update logic here
    pass
```

### `src/panels/script_panel.rs`

**Script Panel:**

```
┌─ Scripts ────────────────────────────────────┐
│ [+ New Script]  [+ New Script Class]         │
│                                              │
│  main.py              (entry point)          │
│  player.py            (attached: Player#1)   │
│  enemy.py             (attached: Enemy#3)    │
│  utils.py                                    │
│                                              │
│ [Open in IDE]                                │
└──────────────────────────────────────────────┘
```

| Feature | Description |
|---|---|
| **List scripts** | Scan `scripts/` directory, show all `.py` files |
| **New Script** | Prompt for name + template type (basic, script class, per-frame). Generate from template and write to `scripts/<name>.py` |
| **New Script Class** | Prompt for class name. Generate the script class template |
| **Open in IDE** | Launch `$EDITOR`, `code`, or `xdg-open` with the file path |
| **Entity association** | Select an entity, then from the script panel click "Attach" to record the association in `project.json` metadata |

### Entity-Script Association

Stored in `project.json` under a `scripts` key:

```json
{
  "script_associations": [
    { "entity_tag": "player", "script": "player.py", "class": "Player" },
    { "entity_tag": "enemy", "script": "enemy.py", "class": "Enemy" }
  ]
}
```

This is metadata only — the editor does not run Python. It serves as documentation and
could be used to auto-generate `attach_script()` calls in `main.py`.

---

## 4. Verification

1. UI Editor: can create a new UI file with panels, labels, and buttons
2. Widget tree shows correct hierarchy; selecting a widget shows its properties
3. Preview updates in real-time as properties are changed
4. Saving a UI writes valid JSON to `ui/<name>.json`; loading restores the widget tree
5. Theme changes propagate to the preview
6. Script panel lists all `.py` files in `scripts/`
7. "New Script" creates a file from the correct template
8. "Open in IDE" launches the system editor
9. Entity-script associations are saved to and loaded from `project.json`

---

## Files Created / Modified

| Action | File |
|---|---|
| **Modify** | `crates/rython-ui/src/manager.rs` (add `save_json()` / `load_json()`) |
| **Create** | `crates/rython-editor/src/panels/ui_editor.rs` |
| **Create** | `crates/rython-editor/src/panels/script_panel.rs` |
| **Create** | `crates/rython-editor/src/project/scaffold.rs` |
| **Modify** | `crates/rython-editor/src/project/format.rs` (add `script_associations`) |
| **Modify** | `crates/rython-editor/src/app.rs` (integrate UI editor + script panel) |

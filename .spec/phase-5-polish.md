# Phase 5: Polish

**Goal:** Quality-of-life improvements that make the editor feel like a real production
tool.

**Result:** Dockable panel layout, full keyboard shortcuts, multi-entity operations,
recent project history, and a "Play" button to launch the game.

**Depends on:** Phase 4 (MVP complete)

---

## 1. Dockable Panels (egui-dock)

### Dependency

Add `egui_dock` to the editor's dependencies. egui-dock provides a docking system similar
to Visual Studio or Unity where panels can be:

- Dragged and dropped to reposition
- Split horizontally or vertically
- Tabbed alongside other panels
- Floated into separate windows
- Collapsed/hidden

### Default Layout

```
┌──────────────────────────────────────────────────┐
│ Menu Bar                                         │
├────────────┬──────────────────┬──────────────────┤
│            │                  │                  │
│ Hierarchy  │    Viewport      │   Inspector      │
│ (tab)      │    (tab)         │   (tab)          │
│ UI Editor  │    UI Preview    │   Script Panel   │
│ (tab)      │    (tab)         │   (tab)          │
│            │                  │                  │
├────────────┴──────────────────┼──────────────────┤
│                               │                  │
│    Asset Browser              │   Console/Log    │
│                               │                  │
└───────────────────────────────┴──────────────────┘
```

### Layout Persistence

Save the dock layout to a JSON file in the project directory (`.editor/layout.json`) so
it is restored when the project is reopened. Use `egui_dock`'s serialization support.

### View Menu

The View menu lists all available panels with checkboxes to show/hide them:
- Hierarchy
- Viewport
- Inspector
- Asset Browser
- UI Editor
- Script Panel
- Console (log output)

"Reset Layout" restores the default arrangement.

---

## 2. Keyboard Shortcuts

### Global Shortcuts

| Shortcut | Action |
|---|---|
| Ctrl+N | New Project |
| Ctrl+O | Open Project |
| Ctrl+S | Save Scene |
| Ctrl+Shift+S | Save Scene As |
| Ctrl+Z | Undo |
| Ctrl+Shift+Z | Redo |
| Delete | Delete selected entity/widget |
| Ctrl+D | Duplicate selected entity/widget |
| Ctrl+C | Copy selection |
| Ctrl+V | Paste |

### Viewport Shortcuts

| Shortcut | Action |
|---|---|
| W | Translate gizmo mode |
| E | Rotate gizmo mode |
| R | Scale gizmo mode |
| X | Toggle World/Local gizmo space |
| F | Focus camera on selected entity |
| Numpad 0 | Reset camera to default position |
| Numpad 1 | Front view (look along -Z) |
| Numpad 3 | Right view (look along -X) |
| Numpad 7 | Top view (look along -Y) |
| G | Toggle grid visibility |

### Implementation

Register shortcuts via `egui::Context::input_mut()` checking for key combinations.
Viewport shortcuts only fire when the viewport panel has focus (determined by hover
state or egui focus tracking).

---

## 3. Multi-Select

### Selection State Extension

```rust
pub struct SelectionState {
    pub primary: Selection,
    pub multi: Vec<EntityId>,     // Additional selected entities
}
```

### Interactions

| Input | Behavior |
|---|---|
| Click | Select single entity, clear multi-select |
| Ctrl+Click | Toggle entity in multi-select |
| Shift+Click in hierarchy | Range select (all entities between last selected and clicked) |

### Multi-Select Operations

When multiple entities are selected:
- Delete: despawn all selected (as a single undoable batch command)
- Gizmo transform: applies the delta to all selected entities
- Inspector: shows shared component types with values that match; divergent values show "--"
- Copy/Paste: copies all selected entities

### Batch Commands

A `BatchCommand` wraps multiple `EditorCommand`s into a single undo step:

```rust
pub struct BatchCommand {
    commands: Vec<Box<dyn EditorCommand>>,
    description: String,
}
```

`execute()` runs all commands in order; `undo()` runs all in reverse.

---

## 4. Copy / Paste

### Copy

When the user presses Ctrl+C with entities selected:
1. For each selected entity, serialize its full state: components (via `snapshot_entity()`),
   parent relationship, children (recursively)
2. Store the serialized data in an editor clipboard (in-memory, not system clipboard)

### Paste

When the user presses Ctrl+V:
1. Deserialize each copied entity, creating new entities with fresh `EntityId`s
2. Offset positions slightly (e.g., +1 on X) to visually distinguish from originals
3. Restore parent-child relationships among the pasted group (remapped to new IDs)
4. Push a batch `SpawnEntity` command for undo
5. Select the newly pasted entities

### Duplicate (Ctrl+D)

Shortcut for Copy + Paste in one step.

---

## 5. Recent Projects

### Storage

Save recent project paths in the editor's config file:
`~/.config/rython-editor/recent.json`

```json
{
  "recent_projects": [
    "/home/user/games/my_game",
    "/home/user/games/platformer",
    "/home/user/games/space_shooter"
  ]
}
```

Maximum 10 entries, most recent first. Remove entries whose `project.json` no longer exists
on disk.

### UI

- **Welcome screen:** When no project is open, show a landing page with:
  - "New Project" button
  - "Open Project" button
  - List of recent projects (click to open)
- **File menu:** "Recent Projects" submenu listing the last 10 projects

---

## 6. "Play" Button

### Behavior

A prominent "Play" button (or `F5` shortcut) in the toolbar:

1. Auto-save the current scene if dirty (prompt if unsaved)
2. Spawn the `rython` CLI binary as a child process:
   ```
   rython --script-dir <project>/scripts
          --entry-point <project.entry_point>
          --config <project>/project.json
   ```
3. Show a "Playing..." indicator in the editor
4. Capture stdout/stderr and display in the Console panel
5. "Stop" button (or `Shift+F5`) sends SIGTERM to the child process

### Scene Loading at Runtime

For the game to load editor-created scenes, the entry point script needs to call a scene
loader. The editor can auto-generate this in `main.py` during scaffolding:

```python
def init():
    rython.scene.load_json("scenes/level_1.json")
```

This requires `rython.scene.load_json()` to exist in the scripting bridge. If it doesn't,
this is a future engine enhancement noted here but not required for the editor itself.

### Process Management

```rust
pub struct PlaySession {
    child: std::process::Child,
    stdout_reader: BufReader<ChildStdout>,
    stderr_reader: BufReader<ChildStderr>,
}
```

Poll stdout/stderr each frame and append lines to the Console panel's log buffer.

---

## 7. Console / Log Panel

A scrollable text panel that displays:
- Editor log messages (info, warnings, errors from the editor itself)
- Game output when running via the Play button (stdout + stderr)
- Asset loading status messages

Features:
- Auto-scroll to bottom on new messages
- Clear button
- Filter by log level (info, warn, error)
- Copy selected text

---

## 8. Editor Preferences

A preferences dialog (Edit > Preferences or Ctrl+,):

| Setting | Options |
|---|---|
| Theme | Dark (default), Light |
| Font size | Slider (10-20 pt) |
| Viewport background color | Color picker |
| Grid spacing | Drag value (default 1.0) |
| Auto-save interval | Off, 1min, 5min, 10min |
| Default gizmo mode | Translate, Rotate, Scale |
| External editor command | Text input (default: auto-detect) |

Stored in `~/.config/rython-editor/preferences.json`.

---

## 9. Verification

1. Panels can be dragged, docked, tabbed, and floated
2. Layout persists across editor restarts
3. All keyboard shortcuts work as documented
4. Ctrl+Click in hierarchy adds to multi-select; Delete removes all selected
5. Gizmo transforms apply to all selected entities simultaneously
6. Ctrl+C / Ctrl+V copies and pastes entities with offset positions
7. Recent projects appear on the welcome screen and in File menu
8. Play button launches the game; output appears in Console panel
9. Stop button terminates the game process cleanly
10. Preferences dialog saves and restores settings

---

## Files Created / Modified

| Action | File |
|---|---|
| **Modify** | `crates/rython-editor/Cargo.toml` (add `egui_dock`) |
| **Modify** | `crates/rython-editor/src/app.rs` (dock layout, shortcuts, play session) |
| **Modify** | `crates/rython-editor/src/state/selection.rs` (multi-select) |
| **Create** | `crates/rython-editor/src/state/clipboard.rs` |
| **Create** | `crates/rython-editor/src/state/preferences.rs` |
| **Create** | `crates/rython-editor/src/panels/console.rs` |
| **Create** | `crates/rython-editor/src/panels/welcome.rs` |
| **Modify** | `crates/rython-editor/src/state/undo.rs` (add `BatchCommand`) |
| **Modify** | `crates/rython-editor/src/viewport/gizmo.rs` (multi-entity transform) |

# UI

The UI system provides an immediate-mode-inspired widget tree for 2D user interfaces: menus, HUDs, dialogue boxes, inventories. It is built on top of the renderer and input systems, handling layout, styling, animation, and event routing.


## Architecture

The UIManager is a Module that orchestrates:
- **Widget tree**: A hierarchy of UI elements (buttons, labels, panels, etc.)
- **Layout system**: Computes widget positions and sizes based on layout rules
- **Theme**: Centralized styling (colors, fonts, spacing)
- **UIAnimator**: Property tweening for smooth transitions
- **InputRouter**: Dispatches input events to focused/hovered widgets

The UIManager runs a recurring task at GAME_LATE priority. Each frame, it:
1. Drains UI commands (show, hide, focus, cursor visibility)
2. Applies layout calculations
3. Ticks active animations
4. Processes input events (returns unconsumed events to the game)
5. Emits draw commands for all visible widgets to the renderer


## Widgets

All widgets share a common set of properties: position, size, color, visibility, parent/children, and interaction state (normal, hover, active, disabled).

### Built-in Widget Types

**Label**: Static text display.
```python
import rython

label = rython.ui.create_label(
    text="Game Over",
    font="title_font",
    size=48,
    color=(255, 255, 255),
    position=(0.5, 0.3),  # normalized screen space, centered
)
```

**Button**: Clickable element with hover/active states and a callback.
```python
play_btn = rython.ui.create_button(
    text="Play",
    position=(0.5, 0.5),
    size=(0.2, 0.06),
    on_click=self.start_game,
)
```

**TextInput**: Editable text field with cursor and selection.
```python
name_input = rython.ui.create_text_input(
    placeholder="Enter name...",
    position=(0.5, 0.4),
    size=(0.3, 0.05),
    on_submit=self.on_name_entered,
)
```

**Panel**: Container for other widgets. Provides background, border, and padding.
```python
menu_panel = rython.ui.create_panel(
    position=(0.3, 0.2),
    size=(0.4, 0.6),
    color=(20, 20, 30, 200),
    border_width=2,
    border_color=(100, 100, 120),
)
```

**ScrollView**: Scrollable container for content larger than its bounds.


## Widget Tree

Widgets are organized in a parent-child tree. A widget's position is relative to its parent. Visibility cascades: hiding a parent hides all its children.

```python
# Build a menu
menu = rython.ui.create_panel(position=(0.3, 0.2), size=(0.4, 0.6))

title = rython.ui.create_label(text="Main Menu", parent=menu)
play_btn = rython.ui.create_button(text="Play", parent=menu, on_click=self.play)
quit_btn = rython.ui.create_button(text="Quit", parent=menu, on_click=self.quit)

# Show/hide the entire menu
rython.ui.show(menu)
rython.ui.hide(menu)
```


## Layout System

Widgets can be positioned manually (absolute coordinates) or use layout rules for automatic arrangement:

- **Anchoring**: Attach a widget to a parent edge or center
- **Margins**: Spacing from parent edges
- **Auto-sizing**: Widget size adapts to content (text length, children count)
- **Stacking**: Children arranged vertically or horizontally with spacing

```python
# Vertical stack layout
menu = rython.ui.create_panel(
    position=(0.3, 0.2),
    size=(0.4, 0.6),
    layout="vertical",
    spacing=0.02,
    padding=0.03,
)

# Children are automatically stacked vertically
rython.ui.create_button(text="Play", parent=menu, on_click=self.play)
rython.ui.create_button(text="Options", parent=menu, on_click=self.options)
rython.ui.create_button(text="Quit", parent=menu, on_click=self.quit)
```


## Theme

The Theme system provides centralized styling. Widgets pull their default colors, fonts, and spacing from the active theme. Themes can be switched at runtime for features like dark mode.

```python
rython.ui.set_theme({
    "font": "default_font",
    "font_size": 18,
    "text_color": (220, 220, 220),
    "button_color": (50, 50, 70),
    "button_hover_color": (70, 70, 100),
    "button_active_color": (40, 40, 55),
    "panel_color": (20, 20, 30, 200),
    "border_color": (100, 100, 120),
    "border_width": 1,
    "padding": 0.01,
    "spacing": 0.01,
})
```


## Animation

The UIAnimator provides property tweening for smooth UI transitions. Any numeric widget property (position, size, color, alpha, rotation) can be animated.

```python
# Fade in a panel
rython.ui.animate(menu,
    property="alpha",
    from_value=0.0,
    to_value=1.0,
    duration=0.3,
    easing="ease_out",
)

# Slide a panel into view
rython.ui.animate(menu,
    property="position_x",
    from_value=-0.5,
    to_value=0.3,
    duration=0.4,
    easing="ease_in_out",
)
```

Available easing functions: `linear`, `ease_in`, `ease_out`, `ease_in_out`, `bounce`, `elastic`.

Animations can be chained (sequential) or grouped (parallel):

```python
# Sequential: fade in, then slide
rython.ui.animate_sequence(menu, [
    {"property": "alpha", "to": 1.0, "duration": 0.2},
    {"property": "position_y", "to": 0.2, "duration": 0.3, "easing": "bounce"},
])
```


## Input Routing

The InputRouter dispatches mouse and keyboard events to the appropriate widget:

- **Mouse move**: Updates hover state. The topmost widget under the cursor receives hover.
- **Mouse click**: The hovered widget receives the click. Fires `on_click` callback.
- **Keyboard**: The focused widget receives key events. Focus is set by clicking or by `rython.ui.focus(widget)`.
- **Tab navigation**: Tab moves focus to the next focusable widget in tree order.

Events consumed by the UI are not forwarded to the game's input system. This prevents clicking a menu button from also firing a game "attack" action.

```python
# Control cursor visibility
rython.ui.set_cursor_visible(True)   # Show OS cursor (menus)
rython.ui.set_cursor_visible(False)  # Hide cursor (gameplay)
```


## Commands

The UIManager accepts commands via a queue, matching the engine's command-driven pattern:

- **ShowUICmd**: Make a widget tree visible
- **HideUICmd**: Hide a widget tree
- **FocusWidgetCmd**: Set keyboard focus to a widget
- **SetCursorVisibleCmd**: Show or hide the OS cursor

Commands are drained during the UIManager's per-frame task, ensuring consistent state within a frame.


## Acceptance Tests

### T-UI-01: Widget Creation
Create a Label widget with text="Hello", position=(0.5, 0.3), color=(255, 255, 255).
- Expected: Widget is assigned a unique widget ID
- Expected: Widget properties match the creation parameters
- Expected: Widget is visible by default

### T-UI-02: Widget Tree — Parent-Child
Create a Panel. Create a Button with parent=Panel.
- Expected: Button's parent is the Panel
- Expected: Panel's children list contains the Button
- Expected: Button's absolute position is relative to the Panel's position

### T-UI-03: Widget Tree — Visibility Cascade
Create a Panel with two child Buttons. Hide the Panel.
- Expected: Panel is not visible
- Expected: Both child Buttons are not visible (inherited)
- Expected: Show the Panel. Both Buttons become visible again

### T-UI-04: Vertical Stack Layout
Create a Panel with layout="vertical", spacing=0.02, padding=0.01. Add 3 Buttons of height 0.05 each.
- Expected: Button 1 position.y = panel.y + padding (0.01)
- Expected: Button 2 position.y = Button 1.y + Button 1.height + spacing (0.05 + 0.02 = 0.08 from Button 1)
- Expected: Button 3 position.y = Button 2.y + 0.07

### T-UI-05: Horizontal Stack Layout
Create a Panel with layout="horizontal", spacing=0.02. Add 3 Buttons of width 0.1 each.
- Expected: Buttons are arranged left-to-right
- Expected: Gaps between buttons are exactly 0.02

### T-UI-06: Theme Application
Set theme with button_color=(50, 50, 70). Create a Button without specifying color.
- Expected: Button's color is (50, 50, 70) (inherited from theme)
- Expected: Creating a Button with explicit color overrides the theme

### T-UI-07: Theme Switch at Runtime
Set theme A with text_color=(255, 255, 255). Create a Label. Switch to theme B with text_color=(0, 0, 0).
- Expected: Label's text color changes to (0, 0, 0) after theme switch
- Expected: Widgets with explicit colors are unaffected by theme switch

### T-UI-08: Animation — Linear Tween
Animate a widget's alpha from 0.0 to 1.0 over 1.0 seconds with linear easing. Sample at t=0.0, 0.25, 0.5, 0.75, 1.0.
- Expected: Values are 0.0, 0.25, 0.5, 0.75, 1.0 (within ± 0.01)

### T-UI-09: Animation — Ease In
Animate position_x from 0.0 to 1.0 over 1.0 seconds with ease_in. Sample at t=0.5.
- Expected: Value is less than 0.5 (ease_in starts slow)
- Expected: Value is approximately 0.25 (quadratic ease-in: t^2)

### T-UI-10: Animation — Ease Out
Same as T-UI-09 but with ease_out. Sample at t=0.5.
- Expected: Value is greater than 0.5 (ease_out starts fast)
- Expected: Value is approximately 0.75 (quadratic ease-out)

### T-UI-11: Animation Completion
Animate alpha from 0.0 to 1.0 over 0.5 seconds. Wait 1.0 seconds.
- Expected: Alpha is exactly 1.0 (clamped to target, not overshooting)
- Expected: The animation is no longer ticking (completed)

### T-UI-12: Sequential Animation Chain
Chain two animations: alpha 0->1 over 0.3s, then position_y 0.5->0.2 over 0.3s.
- Expected: At t=0.15, alpha is ~0.5 and position_y is still 0.5 (second hasn't started)
- Expected: At t=0.45, alpha is 1.0 and position_y is ~0.35 (second is running)
- Expected: At t=0.6+, both are at final values

### T-UI-13: Input Routing — Click on Button
Create a Button at position (0.4, 0.4) with size (0.2, 0.06). Simulate a mouse click at (0.5, 0.43).
- Expected: Button's on_click callback fires
- Expected: The click event is consumed (not forwarded to the game input system)

### T-UI-14: Input Routing — Click Outside Button
Same button. Simulate a mouse click at (0.1, 0.1).
- Expected: Button's on_click does NOT fire
- Expected: The click event is NOT consumed (forwarded to game input)

### T-UI-15: Input Routing — Hover State
Move mouse over a Button. Then move it away.
- Expected: Button state changes to "hover" when mouse enters its bounds
- Expected: Button state returns to "normal" when mouse leaves

### T-UI-16: Focus and Keyboard Events
Create a TextInput. Focus it via `rython.ui.focus(text_input)`. Simulate typing "abc".
- Expected: TextInput's text property becomes "abc"
- Expected: Keyboard events are consumed by the TextInput (not forwarded)

### T-UI-17: Tab Navigation
Create 3 focusable Buttons in order. Focus the first. Simulate pressing Tab twice.
- Expected: Focus moves: Button1 -> Button2 -> Button3
- Expected: Only the focused button is in the "focused" state

### T-UI-18: Draw Command Emission
Create a visible Panel with a visible Button child. Run the UIManager's per-frame task.
- Expected: DrawRect commands are emitted for the Panel background
- Expected: DrawRect + DrawText commands are emitted for the Button
- Expected: All UI draw commands have z-values higher than typical game draw commands (rendered on top)

### T-UI-19: Command Queue — Show/Hide
Create a widget. Submit HideUICmd. Drain commands.
- Expected: Widget is hidden after drain
- Expected: Submit ShowUICmd. Drain. Widget is visible again

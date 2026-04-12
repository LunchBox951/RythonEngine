"""Integration tests for rython.ui widget system."""

import rython
from _harness import TestSuite

suite = TestSuite()


# ── Widget creation ───────────────────────────────────────────────────────────


def test_create_label_returns_int():
    wid = rython.ui.create_label("Hello", 0.0, 0.0, 0.5, 0.1)
    assert isinstance(wid, int), f"expected int, got {type(wid)}"
    assert wid >= 0, f"expected >= 0, got {wid}"


def test_create_button_returns_int():
    wid = rython.ui.create_button("Click", 0.0, 0.0, 0.3, 0.08)
    assert isinstance(wid, int), f"expected int, got {type(wid)}"
    assert wid >= 0, f"expected >= 0, got {wid}"


def test_create_panel_returns_int():
    wid = rython.ui.create_panel(0.0, 0.0, 1.0, 1.0)
    assert isinstance(wid, int), f"expected int, got {type(wid)}"
    assert wid >= 0, f"expected >= 0, got {wid}"


def test_create_text_input_returns_int():
    wid = rython.ui.create_text_input("Type here...", 0.1, 0.1, 0.4, 0.06)
    assert isinstance(wid, int), f"expected int, got {type(wid)}"
    assert wid >= 0, f"expected >= 0, got {wid}"


def test_unique_ids():
    a = rython.ui.create_label("A", 0.0, 0.0, 0.1, 0.1)
    b = rython.ui.create_button("B", 0.0, 0.0, 0.1, 0.1)
    c = rython.ui.create_panel(0.0, 0.0, 0.1, 0.1)
    d = rython.ui.create_text_input("D", 0.0, 0.0, 0.1, 0.1)
    ids = {a, b, c, d}
    assert len(ids) == 4, f"expected 4 unique IDs, got {ids}"


def test_create_label_various_positions():
    w1 = rython.ui.create_label("Pos1", 0.0, 0.0, 0.5, 0.1)
    w2 = rython.ui.create_label("Pos2", 0.5, 0.5, 0.3, 0.05)
    assert isinstance(w1, int) and w1 >= 0
    assert isinstance(w2, int) and w2 >= 0
    assert w1 != w2, "expected different IDs for different labels"


# ── Parent-child ──────────────────────────────────────────────────────────────


def test_add_child_label():
    panel = rython.ui.create_panel(0.0, 0.0, 1.0, 1.0)
    label = rython.ui.create_label("child", 0.0, 0.0, 0.2, 0.05)
    rython.ui.add_child(panel, label)


def test_add_child_button():
    panel = rython.ui.create_panel(0.0, 0.0, 1.0, 1.0)
    button = rython.ui.create_button("child btn", 0.0, 0.0, 0.2, 0.05)
    rython.ui.add_child(panel, button)


def test_multiple_children():
    panel = rython.ui.create_panel(0.0, 0.0, 1.0, 1.0)
    c1 = rython.ui.create_label("c1", 0.0, 0.0, 0.1, 0.05)
    c2 = rython.ui.create_button("c2", 0.0, 0.0, 0.1, 0.05)
    c3 = rython.ui.create_text_input("c3", 0.0, 0.0, 0.1, 0.05)
    rython.ui.add_child(panel, c1)
    rython.ui.add_child(panel, c2)
    rython.ui.add_child(panel, c3)


# ── Layout ────────────────────────────────────────────────────────────────────


def test_layout_vertical():
    panel = rython.ui.create_panel(0.0, 0.0, 1.0, 1.0)
    rython.ui.set_layout(panel, "vertical", 5.0, 10.0)


def test_layout_horizontal():
    panel = rython.ui.create_panel(0.0, 0.0, 1.0, 1.0)
    rython.ui.set_layout(panel, "horizontal", 2.0, 5.0)


def test_layout_none():
    panel = rython.ui.create_panel(0.0, 0.0, 1.0, 1.0)
    rython.ui.set_layout(panel, "none", 0.0, 0.0)


# ── Visibility ────────────────────────────────────────────────────────────────


def test_widget_starts_visible():
    wid = rython.ui.create_label("vis", 0.0, 0.0, 0.2, 0.05)
    assert rython.ui.is_visible(wid) is True, "new widget should be visible"


def test_hide_makes_invisible():
    wid = rython.ui.create_label("hide me", 0.0, 0.0, 0.2, 0.05)
    rython.ui.hide(wid)
    assert rython.ui.is_visible(wid) is False, "hidden widget should not be visible"


def test_show_makes_visible():
    wid = rython.ui.create_label("show me", 0.0, 0.0, 0.2, 0.05)
    rython.ui.hide(wid)
    rython.ui.show(wid)
    assert rython.ui.is_visible(wid) is True, "shown widget should be visible"


def test_hide_show_cycle():
    wid = rython.ui.create_button("cycle", 0.0, 0.0, 0.2, 0.05)
    assert rython.ui.is_visible(wid) is True
    rython.ui.hide(wid)
    assert rython.ui.is_visible(wid) is False
    rython.ui.show(wid)
    assert rython.ui.is_visible(wid) is True


# ── Text ──────────────────────────────────────────────────────────────────────


def test_set_text_label():
    wid = rython.ui.create_label("old", 0.0, 0.0, 0.2, 0.05)
    rython.ui.set_text(wid, "new text")


def test_set_text_button():
    wid = rython.ui.create_button("old", 0.0, 0.0, 0.2, 0.05)
    rython.ui.set_text(wid, "Click Me")


def test_set_text_empty():
    wid = rython.ui.create_label("something", 0.0, 0.0, 0.2, 0.05)
    rython.ui.set_text(wid, "")


# ── Click handler ─────────────────────────────────────────────────────────────


def test_on_click_register():
    wid = rython.ui.create_button("clickable", 0.0, 0.0, 0.2, 0.05)
    rython.ui.on_click(wid, lambda: None)


# ── Theme ─────────────────────────────────────────────────────────────────────


def test_theme_button_color():
    rython.ui.set_theme(button_color=(100, 150, 200))


def test_theme_text_and_font():
    rython.ui.set_theme(text_color=(255, 255, 255), font_size=20)


def test_theme_panel_and_border():
    rython.ui.set_theme(panel_color=(30, 30, 30), border_color=(100, 100, 100))


def test_theme_no_args():
    rython.ui.set_theme()


# ── Hardening: bad-id paths must raise, not crash the engine ─────────────────


def test_add_child_invalid_parent_raises():
    # Regression: previously panicked in rython-ui manager.rs via HashMap
    # index, which crossed the PyO3 boundary and aborted the process.
    child = rython.ui.create_label("orphan", 0.0, 0.0, 0.1, 0.05)
    try:
        rython.ui.add_child(9_999_999, child)
    except RuntimeError:
        return  # expected
    raise AssertionError("add_child with invalid parent must raise RuntimeError")


def test_add_child_invalid_child_raises():
    panel = rython.ui.create_panel(0.0, 0.0, 1.0, 1.0)
    try:
        rython.ui.add_child(panel, 9_999_999)
    except RuntimeError:
        return
    raise AssertionError("add_child with invalid child must raise RuntimeError")


def test_set_layout_invalid_id_is_noop():
    # No-op on unknown id; must not crash.
    rython.ui.set_layout(9_999_999, "vertical", 0.0, 0.0)


def test_set_layout_invalid_direction_raises():
    panel = rython.ui.create_panel(0.0, 0.0, 1.0, 1.0)
    try:
        rython.ui.set_layout(panel, "diagonal", 0.0, 0.0)
    except RuntimeError:
        return
    raise AssertionError("Unknown layout direction must raise RuntimeError")


def test_add_child_cycle_raises():
    # Cycle detection — previously unguarded, would have stack-overflowed
    # on is_visible() walking the tree.
    a = rython.ui.create_panel(0.0, 0.0, 1.0, 1.0)
    b = rython.ui.create_panel(0.0, 0.0, 1.0, 1.0)
    rython.ui.add_child(a, b)  # b is now a child of a
    try:
        rython.ui.add_child(b, a)  # would create cycle
    except RuntimeError:
        return
    raise AssertionError("Adding a parent as its own descendant must raise")


# ── Entry point ───────────────────────────────────────────────────────────────


def init():
    tests = [
        # Widget creation
        test_create_label_returns_int,
        test_create_button_returns_int,
        test_create_panel_returns_int,
        test_create_text_input_returns_int,
        test_unique_ids,
        test_create_label_various_positions,
        # Parent-child
        test_add_child_label,
        test_add_child_button,
        test_multiple_children,
        # Layout
        test_layout_vertical,
        test_layout_horizontal,
        test_layout_none,
        # Visibility
        test_widget_starts_visible,
        test_hide_makes_invisible,
        test_show_makes_visible,
        test_hide_show_cycle,
        # Text
        test_set_text_label,
        test_set_text_button,
        test_set_text_empty,
        # Click handler
        test_on_click_register,
        # Theme
        test_theme_button_color,
        test_theme_text_and_font,
        test_theme_panel_and_border,
        test_theme_no_args,
        # Hardening regression tests
        test_add_child_invalid_parent_raises,
        test_add_child_invalid_child_raises,
        test_set_layout_invalid_id_is_noop,
        test_set_layout_invalid_direction_raises,
        test_add_child_cycle_raises,
    ]

    for fn in tests:
        suite.run(fn.__name__, fn)

    suite.report_and_quit()

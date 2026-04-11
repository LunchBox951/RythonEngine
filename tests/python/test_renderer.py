"""Integration tests for the rython.renderer Python API.

All renderer calls are fire-and-forget commands. In headless mode the GPU
backend is absent, so these tests verify that each API entry point can be
called without raising an exception.
"""

import rython
from _harness import TestSuite


suite = TestSuite()


# ── draw_text ─────────────────────────────────────────────────────────────────

def test_draw_text_defaults():
    rython.renderer.draw_text("hello")


def test_draw_text_all_params():
    rython.renderer.draw_text(
        "Hello", font_id="default", x=0.1, y=0.9, size=32,
        r=255, g=0, b=0, z=1.0,
    )


def test_draw_text_empty_string():
    rython.renderer.draw_text("")


# ── set_clear_color ───────────────────────────────────────────────────────────

def test_set_clear_color_basic():
    rython.renderer.set_clear_color(0.1, 0.2, 0.3)


def test_set_clear_color_transparent():
    rython.renderer.set_clear_color(0.0, 0.0, 0.0, 0.0)


def test_set_clear_color_white():
    rython.renderer.set_clear_color(1.0, 1.0, 1.0, 1.0)


# ── set_light_direction ──────────────────────────────────────────────────────

def test_set_light_direction_down():
    rython.renderer.set_light_direction(0, -1, 0)


def test_set_light_direction_normalized():
    rython.renderer.set_light_direction(1, 1, 1)


# ── set_light_color ──────────────────────────────────────────────────────────

def test_set_light_color():
    rython.renderer.set_light_color(1.0, 0.8, 0.6)


# ── set_light_intensity ──────────────────────────────────────────────────────

def test_set_light_intensity_positive():
    rython.renderer.set_light_intensity(2.0)


def test_set_light_intensity_zero():
    rython.renderer.set_light_intensity(0.0)


# ── set_ambient_light ────────────────────────────────────────────────────────

def test_set_ambient_light_defaults():
    rython.renderer.set_ambient_light()


def test_set_ambient_light_custom():
    rython.renderer.set_ambient_light(r=0.2, g=0.2, b=0.2, intensity=0.5)


# ── shadow settings ──────────────────────────────────────────────────────────

def test_set_shadow_enabled_true():
    rython.renderer.set_shadow_enabled(True)


def test_set_shadow_enabled_false():
    rython.renderer.set_shadow_enabled(False)


def test_set_shadow_map_size_1024():
    rython.renderer.set_shadow_map_size(1024)


def test_set_shadow_map_size_4096():
    rython.renderer.set_shadow_map_size(4096)


def test_set_shadow_bias():
    rython.renderer.set_shadow_bias(0.01)


def test_set_shadow_pcf_1():
    rython.renderer.set_shadow_pcf(1)


def test_set_shadow_pcf_8():
    rython.renderer.set_shadow_pcf(8)


# ── multiple draw_text in one init ───────────────────────────────────────────

def test_multiple_draw_text():
    rython.renderer.draw_text("line 1", x=0.1, y=0.1)
    rython.renderer.draw_text("line 2", x=0.1, y=0.2)
    rython.renderer.draw_text("line 3", x=0.1, y=0.3)


# ── entry point ──────────────────────────────────────────────────────────────

def init():
    suite.run("draw_text_defaults", test_draw_text_defaults)
    suite.run("draw_text_all_params", test_draw_text_all_params)
    suite.run("draw_text_empty_string", test_draw_text_empty_string)
    suite.run("set_clear_color_basic", test_set_clear_color_basic)
    suite.run("set_clear_color_transparent", test_set_clear_color_transparent)
    suite.run("set_clear_color_white", test_set_clear_color_white)
    suite.run("set_light_direction_down", test_set_light_direction_down)
    suite.run("set_light_direction_normalized", test_set_light_direction_normalized)
    suite.run("set_light_color", test_set_light_color)
    suite.run("set_light_intensity_positive", test_set_light_intensity_positive)
    suite.run("set_light_intensity_zero", test_set_light_intensity_zero)
    suite.run("set_ambient_light_defaults", test_set_ambient_light_defaults)
    suite.run("set_ambient_light_custom", test_set_ambient_light_custom)
    suite.run("set_shadow_enabled_true", test_set_shadow_enabled_true)
    suite.run("set_shadow_enabled_false", test_set_shadow_enabled_false)
    suite.run("set_shadow_map_size_1024", test_set_shadow_map_size_1024)
    suite.run("set_shadow_map_size_4096", test_set_shadow_map_size_4096)
    suite.run("set_shadow_bias", test_set_shadow_bias)
    suite.run("set_shadow_pcf_1", test_set_shadow_pcf_1)
    suite.run("set_shadow_pcf_8", test_set_shadow_pcf_8)
    suite.run("multiple_draw_text", test_multiple_draw_text)
    suite.report_and_quit()

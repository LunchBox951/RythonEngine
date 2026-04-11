"""Integration tests for rython.input API (headless defaults)."""

import rython
from _harness import TestSuite


suite = TestSuite()


def test_axis_move_x_default():
    val = rython.input.axis("move_x")
    assert val == 0.0, f"axis('move_x')={val}"


def test_axis_move_z_default():
    val = rython.input.axis("move_z")
    assert val == 0.0, f"axis('move_z')={val}"


def test_axis_nonexistent_default():
    val = rython.input.axis("nonexistent_action")
    assert val == 0.0, f"axis('nonexistent_action')={val}"


def test_pressed_jump_default():
    val = rython.input.pressed("jump")
    assert val is False, f"pressed('jump')={val}"


def test_held_jump_default():
    val = rython.input.held("jump")
    assert val is False, f"held('jump')={val}"


def test_released_jump_default():
    val = rython.input.released("jump")
    assert val is False, f"released('jump')={val}"


def test_pressed_nonexistent_default():
    val = rython.input.pressed("nonexistent")
    assert val is False, f"pressed('nonexistent')={val}"


def test_held_nonexistent_default():
    val = rython.input.held("nonexistent")
    assert val is False, f"held('nonexistent')={val}"


def test_released_nonexistent_default():
    val = rython.input.released("nonexistent")
    assert val is False, f"released('nonexistent')={val}"


def test_axis_returns_float():
    val = rython.input.axis("move_x")
    assert isinstance(val, float), f"type={type(val).__name__}"


def test_pressed_returns_bool():
    val = rython.input.pressed("jump")
    assert isinstance(val, bool), f"type={type(val).__name__}"


def test_held_returns_bool():
    val = rython.input.held("jump")
    assert isinstance(val, bool), f"type={type(val).__name__}"


def test_released_returns_bool():
    val = rython.input.released("jump")
    assert isinstance(val, bool), f"type={type(val).__name__}"


def init():
    suite.run("test_axis_move_x_default", test_axis_move_x_default)
    suite.run("test_axis_move_z_default", test_axis_move_z_default)
    suite.run("test_axis_nonexistent_default", test_axis_nonexistent_default)
    suite.run("test_pressed_jump_default", test_pressed_jump_default)
    suite.run("test_held_jump_default", test_held_jump_default)
    suite.run("test_released_jump_default", test_released_jump_default)
    suite.run("test_pressed_nonexistent_default", test_pressed_nonexistent_default)
    suite.run("test_held_nonexistent_default", test_held_nonexistent_default)
    suite.run("test_released_nonexistent_default", test_released_nonexistent_default)
    suite.run("test_axis_returns_float", test_axis_returns_float)
    suite.run("test_pressed_returns_bool", test_pressed_returns_bool)
    suite.run("test_held_returns_bool", test_held_returns_bool)
    suite.run("test_released_returns_bool", test_released_returns_bool)
    suite.report_and_quit()

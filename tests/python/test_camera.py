"""Integration tests for rython.camera API."""

import rython
from _harness import TestSuite


def _close(a, b, tol=0.001):
    """Return True if two floats are within *tol* of each other."""
    return abs(a - b) < tol


suite = TestSuite()


def test_default_position():
    assert _close(rython.camera.pos_x, 0.0), f"pos_x={rython.camera.pos_x}"
    assert _close(rython.camera.pos_y, 0.0), f"pos_y={rython.camera.pos_y}"
    assert _close(rython.camera.pos_z, -10.0), f"pos_z={rython.camera.pos_z}"


def test_set_position_readback():
    rython.camera.set_position(5.0, 10.0, -20.0)
    assert _close(rython.camera.pos_x, 5.0), f"pos_x={rython.camera.pos_x}"
    assert _close(rython.camera.pos_y, 10.0), f"pos_y={rython.camera.pos_y}"
    assert _close(rython.camera.pos_z, -20.0), f"pos_z={rython.camera.pos_z}"


def test_set_rotation_readback():
    rython.camera.set_rotation(0.5, 1.0, 0.0)
    assert _close(rython.camera.rot_pitch, 0.5), f"rot_pitch={rython.camera.rot_pitch}"
    assert _close(rython.camera.rot_yaw, 1.0), f"rot_yaw={rython.camera.rot_yaw}"
    assert _close(rython.camera.rot_roll, 0.0), f"rot_roll={rython.camera.rot_roll}"


def test_set_look_at_origin():
    rython.camera.set_position(0.0, 0.0, -10.0)
    rython.camera.set_look_at(0.0, 0.0, 0.0)
    assert _close(rython.camera.target_x, 0.0), f"target_x={rython.camera.target_x}"
    assert _close(rython.camera.target_y, 0.0), f"target_y={rython.camera.target_y}"
    assert _close(rython.camera.target_z, 0.0), f"target_z={rython.camera.target_z}"


def test_set_look_at_arbitrary():
    rython.camera.set_look_at(10.0, 5.0, 3.0)
    assert _close(rython.camera.target_x, 10.0), f"target_x={rython.camera.target_x}"
    assert _close(rython.camera.target_y, 5.0), f"target_y={rython.camera.target_y}"
    assert _close(rython.camera.target_z, 3.0), f"target_z={rython.camera.target_z}"


def test_property_mutation():
    rython.camera.pos_x = 42.0
    assert _close(rython.camera.pos_x, 42.0), f"pos_x={rython.camera.pos_x}"


def test_multiple_set_position_last_wins():
    rython.camera.set_position(1.0, 2.0, 3.0)
    rython.camera.set_position(10.0, 20.0, 30.0)
    rython.camera.set_position(100.0, 200.0, 300.0)
    assert _close(rython.camera.pos_x, 100.0), f"pos_x={rython.camera.pos_x}"
    assert _close(rython.camera.pos_y, 200.0), f"pos_y={rython.camera.pos_y}"
    assert _close(rython.camera.pos_z, 300.0), f"pos_z={rython.camera.pos_z}"


def init():
    suite.run("test_default_position", test_default_position)
    suite.run("test_set_position_readback", test_set_position_readback)
    suite.run("test_set_rotation_readback", test_set_rotation_readback)
    suite.run("test_set_look_at_origin", test_set_look_at_origin)
    suite.run("test_set_look_at_arbitrary", test_set_look_at_arbitrary)
    suite.run("test_property_mutation", test_property_mutation)
    suite.run("test_multiple_set_position_last_wins", test_multiple_set_position_last_wins)
    suite.report_and_quit()

"""Integration tests for Vec3 and Transform core types."""

import rython
from _harness import TestSuite


# ---------------------------------------------------------------------------
# Vec3 tests
# ---------------------------------------------------------------------------

def test_vec3_construction():
    v = rython.Vec3(1.0, 2.0, 3.0)
    assert v.x == 1.0
    assert v.y == 2.0
    assert v.z == 3.0


def test_vec3_zero():
    v = rython.Vec3(0.0, 0.0, 0.0)
    assert v.x == 0.0
    assert v.y == 0.0
    assert v.z == 0.0


def test_vec3_negative_values():
    v = rython.Vec3(-1.0, -2.5, -3.75)
    assert v.x == -1.0
    assert v.y == -2.5
    assert v.z == -3.75


def test_vec3_property_mutation():
    v = rython.Vec3(1.0, 2.0, 3.0)
    v.x = 10.0
    v.y = 20.0
    v.z = 30.0
    assert v.x == 10.0
    assert v.y == 20.0
    assert v.z == 30.0


def test_vec3_add():
    a = rython.Vec3(1.0, 2.0, 3.0)
    b = rython.Vec3(4.0, 5.0, 6.0)
    c = a + b
    assert abs(c.x - 5.0) < 0.001
    assert abs(c.y - 7.0) < 0.001
    assert abs(c.z - 9.0) < 0.001


def test_vec3_sub():
    a = rython.Vec3(5.0, 7.0, 9.0)
    b = rython.Vec3(1.0, 2.0, 3.0)
    c = a - b
    assert abs(c.x - 4.0) < 0.001
    assert abs(c.y - 5.0) < 0.001
    assert abs(c.z - 6.0) < 0.001


def test_vec3_mul_scalar():
    v = rython.Vec3(1.0, 2.0, 3.0)
    r = v * 3.0
    assert abs(r.x - 3.0) < 0.001
    assert abs(r.y - 6.0) < 0.001
    assert abs(r.z - 9.0) < 0.001


def test_vec3_rmul_scalar():
    v = rython.Vec3(1.0, 2.0, 3.0)
    r = 3.0 * v
    assert abs(r.x - 3.0) < 0.001
    assert abs(r.y - 6.0) < 0.001
    assert abs(r.z - 9.0) < 0.001


def test_vec3_neg():
    v = rython.Vec3(1.0, -2.0, 3.0)
    n = -v
    assert abs(n.x - (-1.0)) < 0.001
    assert abs(n.y - 2.0) < 0.001
    assert abs(n.z - (-3.0)) < 0.001


def test_vec3_length():
    v = rython.Vec3(3.0, 4.0, 0.0)
    assert abs(v.length() - 5.0) < 0.001


def test_vec3_length_unit():
    v = rython.Vec3(1.0, 0.0, 0.0)
    assert abs(v.length() - 1.0) < 0.001


def test_vec3_normalized():
    v = rython.Vec3(3.0, 0.0, 0.0)
    n = v.normalized()
    assert abs(n.x - 1.0) < 0.001
    assert abs(n.y) < 0.001
    assert abs(n.z) < 0.001
    assert abs(n.length() - 1.0) < 0.001


def test_vec3_normalized_diagonal():
    v = rython.Vec3(1.0, 1.0, 1.0)
    n = v.normalized()
    expected = 1.0 / (3.0 ** 0.5)
    assert abs(n.x - expected) < 0.001
    assert abs(n.y - expected) < 0.001
    assert abs(n.z - expected) < 0.001


def test_vec3_dot():
    a = rython.Vec3(1.0, 0.0, 0.0)
    b = rython.Vec3(0.0, 1.0, 0.0)
    assert abs(a.dot(b)) < 0.001


def test_vec3_dot_parallel():
    a = rython.Vec3(2.0, 3.0, 4.0)
    b = rython.Vec3(2.0, 3.0, 4.0)
    expected = 4.0 + 9.0 + 16.0
    assert abs(a.dot(b) - expected) < 0.001


def test_vec3_dot_antiparallel():
    a = rython.Vec3(1.0, 0.0, 0.0)
    b = rython.Vec3(-1.0, 0.0, 0.0)
    assert abs(a.dot(b) - (-1.0)) < 0.001


# ---------------------------------------------------------------------------
# Transform tests
# ---------------------------------------------------------------------------

def test_transform_default():
    t = rython.Transform()
    assert abs(t.x) < 0.001
    assert abs(t.y) < 0.001
    assert abs(t.z) < 0.001
    assert abs(t.rot_x) < 0.001
    assert abs(t.rot_y) < 0.001
    assert abs(t.rot_z) < 0.001
    assert abs(t.scale_x - 1.0) < 0.001
    assert abs(t.scale_y - 1.0) < 0.001
    assert abs(t.scale_z - 1.0) < 0.001


def test_transform_position():
    t = rython.Transform(x=1.0, y=2.0, z=3.0)
    assert abs(t.x - 1.0) < 0.001
    assert abs(t.y - 2.0) < 0.001
    assert abs(t.z - 3.0) < 0.001


def test_transform_rotation():
    t = rython.Transform(rot_x=45.0, rot_y=90.0, rot_z=180.0)
    assert abs(t.rot_x - 45.0) < 0.001
    assert abs(t.rot_y - 90.0) < 0.001
    assert abs(t.rot_z - 180.0) < 0.001


def test_transform_uniform_scale():
    t = rython.Transform(scale=2.0)
    assert abs(t.scale_x - 2.0) < 0.001
    assert abs(t.scale_y - 2.0) < 0.001
    assert abs(t.scale_z - 2.0) < 0.001


def test_transform_per_axis_scale():
    t = rython.Transform(scale_x=1.0, scale_y=2.0, scale_z=3.0)
    assert abs(t.scale_x - 1.0) < 0.001
    assert abs(t.scale_y - 2.0) < 0.001
    assert abs(t.scale_z - 3.0) < 0.001


def test_transform_scale_override():
    t = rython.Transform(scale=5.0, scale_x=1.0)
    assert abs(t.scale_x - 1.0) < 0.001
    assert abs(t.scale_y - 5.0) < 0.001
    assert abs(t.scale_z - 5.0) < 0.001


def test_transform_property_mutation():
    t = rython.Transform()
    t.x = 10.0
    t.y = 20.0
    t.z = 30.0
    t.rot_x = 15.0
    t.rot_y = 25.0
    t.rot_z = 35.0
    t.scale_x = 2.0
    t.scale_y = 3.0
    t.scale_z = 4.0
    assert abs(t.x - 10.0) < 0.001
    assert abs(t.y - 20.0) < 0.001
    assert abs(t.z - 30.0) < 0.001
    assert abs(t.rot_x - 15.0) < 0.001
    assert abs(t.rot_y - 25.0) < 0.001
    assert abs(t.rot_z - 35.0) < 0.001
    assert abs(t.scale_x - 2.0) < 0.001
    assert abs(t.scale_y - 3.0) < 0.001
    assert abs(t.scale_z - 4.0) < 0.001


def test_transform_all_params():
    t = rython.Transform(
        x=1.0, y=2.0, z=3.0,
        rot_x=10.0, rot_y=20.0, rot_z=30.0,
        scale=2.0,
    )
    assert abs(t.x - 1.0) < 0.001
    assert abs(t.y - 2.0) < 0.001
    assert abs(t.z - 3.0) < 0.001
    assert abs(t.rot_x - 10.0) < 0.001
    assert abs(t.rot_y - 20.0) < 0.001
    assert abs(t.rot_z - 30.0) < 0.001
    assert abs(t.scale_x - 2.0) < 0.001
    assert abs(t.scale_y - 2.0) < 0.001
    assert abs(t.scale_z - 2.0) < 0.001


# ---------------------------------------------------------------------------
# Entry point — called by the engine
# ---------------------------------------------------------------------------

def init():
    suite = TestSuite()

    # Vec3 tests
    suite.run("vec3_construction", test_vec3_construction)
    suite.run("vec3_zero", test_vec3_zero)
    suite.run("vec3_negative_values", test_vec3_negative_values)
    suite.run("vec3_property_mutation", test_vec3_property_mutation)
    suite.run("vec3_add", test_vec3_add)
    suite.run("vec3_sub", test_vec3_sub)
    suite.run("vec3_mul_scalar", test_vec3_mul_scalar)
    suite.run("vec3_rmul_scalar", test_vec3_rmul_scalar)
    suite.run("vec3_neg", test_vec3_neg)
    suite.run("vec3_length", test_vec3_length)
    suite.run("vec3_length_unit", test_vec3_length_unit)
    suite.run("vec3_normalized", test_vec3_normalized)
    suite.run("vec3_normalized_diagonal", test_vec3_normalized_diagonal)
    suite.run("vec3_dot", test_vec3_dot)
    suite.run("vec3_dot_parallel", test_vec3_dot_parallel)
    suite.run("vec3_dot_antiparallel", test_vec3_dot_antiparallel)

    # Transform tests
    suite.run("transform_default", test_transform_default)
    suite.run("transform_position", test_transform_position)
    suite.run("transform_rotation", test_transform_rotation)
    suite.run("transform_uniform_scale", test_transform_uniform_scale)
    suite.run("transform_per_axis_scale", test_transform_per_axis_scale)
    suite.run("transform_scale_override", test_transform_scale_override)
    suite.run("transform_property_mutation", test_transform_property_mutation)
    suite.run("transform_all_params", test_transform_all_params)

    suite.report_and_quit()

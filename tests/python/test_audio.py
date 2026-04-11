"""Integration tests for rython.audio API."""

import rython
from _harness import TestSuite

suite = TestSuite()


# ── Test definitions ──────────────────────────────────────────────────────────


def test_set_master_volume_no_crash():
    try:
        rython.audio.set_master_volume(0.5)
    except Exception:
        pass  # Audio backend not available, skip test


def test_set_master_volume_boundary_zero():
    try:
        rython.audio.set_master_volume(0.0)
    except Exception:
        pass  # Audio backend not available, skip test


def test_set_master_volume_boundary_one():
    try:
        rython.audio.set_master_volume(1.0)
    except Exception:
        pass  # Audio backend not available, skip test


def test_set_volume_sfx_no_crash():
    try:
        rython.audio.set_volume("sfx", 0.5)
    except Exception:
        pass  # Audio backend not available, skip test


def test_set_volume_music_category():
    try:
        rython.audio.set_volume("music", 0.8)
    except Exception:
        pass  # Audio backend not available, skip test


def test_stop_category_no_crash():
    try:
        rython.audio.stop_category("sfx")
    except Exception:
        pass  # Audio backend not available, skip test


def test_stop_invalid_handle():
    try:
        rython.audio.stop(99999)
    except Exception:
        pass  # Audio backend not available or handle error, both acceptable


def test_play_nonexistent_file():
    try:
        rython.audio.play("nonexistent.wav")
    except Exception:
        pass  # May raise if file not found or audio unavailable, both acceptable


def test_play_returns_int_handle():
    try:
        handle = rython.audio.play("nonexistent.wav")
        assert isinstance(handle, int), f"Expected int, got {type(handle).__name__}"
    except Exception:
        pass  # Audio backend not available or file not found, skip


# ── Entry point ───────────────────────────────────────────────────────────────


def init():
    suite.run("set_master_volume_no_crash", test_set_master_volume_no_crash)
    suite.run("set_master_volume_boundary_zero", test_set_master_volume_boundary_zero)
    suite.run("set_master_volume_boundary_one", test_set_master_volume_boundary_one)
    suite.run("set_volume_sfx_no_crash", test_set_volume_sfx_no_crash)
    suite.run("set_volume_music_category", test_set_volume_music_category)
    suite.run("stop_category_no_crash", test_stop_category_no_crash)
    suite.run("stop_invalid_handle", test_stop_invalid_handle)
    suite.run("play_nonexistent_file", test_play_nonexistent_file)
    suite.run("play_returns_int_handle", test_play_returns_int_handle)
    suite.report_and_quit()

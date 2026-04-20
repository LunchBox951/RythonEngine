"""Convenience helpers that live on top of the core ``rython.input`` bridge.

At runtime ``rython.input`` is the compiled ``InputBridge`` singleton —
importing ``rython.input.default`` etc. still works because Python resolves
the submodule through this package's ``__init__``.
"""
from __future__ import annotations

from rython.input.default import build_default_map

__all__ = ["build_default_map"]

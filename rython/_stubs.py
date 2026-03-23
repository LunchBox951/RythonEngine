"""
Pure-Python stub for rython sub-modules not yet implemented.

``physics``, ``audio``, ``input``, ``ui``, ``resources``, and ``modules``
are all instances of :class:`SubModule`.  Accessing any attribute on them
raises ``NotImplementedError`` to mirror the runtime ``PyValueError`` raised
by the Rust SubModule bridge.
"""

from __future__ import annotations

from typing import Any


class SubModule:
    """
    Placeholder for a rython sub-module that has not yet been implemented.

    Accessing any attribute raises ``NotImplementedError``.
    """

    def __init__(self, name: str) -> None:
        object.__setattr__(self, "_name", name)

    def __getattr__(self, attr: str) -> Any:
        name = object.__getattribute__(self, "_name")
        raise NotImplementedError(f"rython.{name} is a stub")

    def __repr__(self) -> str:
        name = object.__getattribute__(self, "_name")
        return f"rython.{name}"

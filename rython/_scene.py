"""
Pure-Python type stub for the rython SceneBridge.
"""

from __future__ import annotations

from typing import Any, Callable, TYPE_CHECKING

if TYPE_CHECKING:
    from rython._entity import Entity
    from rython._types import Transform


class SceneBridge:
    """
    Bridge to the ECS scene exposed as ``rython.scene``.

    Keyword arguments accepted by :meth:`spawn`:

    * ``transform`` — a :class:`~rython.Transform` instance
    * ``mesh`` — a mesh-id string **or** a dict with keys
      ``mesh_id``, ``texture_id``, ``visible``
    * ``tags`` — a list of strings
    """

    def spawn(
        self,
        *,
        transform: "Transform | None" = None,
        mesh: "str | dict[str, Any] | None" = None,
        tags: "list[str] | None" = None,
        **kwargs: Any,
    ) -> "Entity":
        """
        Spawn a new entity and return its handle.

        All arguments are optional keyword-only.  The engine drains the spawn
        queue immediately so the returned :class:`~rython.Entity` is valid on
        the same frame.
        """
        raise NotImplementedError

    def emit(self, event_name: str, **kwargs: Any) -> None:
        """Broadcast a named event with an optional keyword payload."""
        raise NotImplementedError

    def subscribe(self, event_name: str, handler: Callable[..., None]) -> int:
        """
        Subscribe *handler* to *event_name*.

        Returns a subscription ID (reserved for future unsubscribe support).
        """
        raise NotImplementedError

    def attach_script(self, entity: "Entity", script_class: type) -> None:
        """
        Attach a Python script class to an entity.

        The class is registered in the global script-class registry and a
        ``ScriptComponent`` is added so the ScriptSystem will instantiate and
        tick it.
        """
        raise NotImplementedError

    def __repr__(self) -> str:
        raise NotImplementedError

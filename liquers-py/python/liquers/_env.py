"""The process-wide default environment, used by ``lq.evaluate``, ``lq.serve`` and the bare decorators."""

import threading
from collections.abc import Awaitable, Coroutine
from typing import Any

from liquers._native import Environment, Server

_DEFAULT: Environment | None = None
_LOCK = threading.Lock()


def default_environment() -> Environment:
    """The shared ``Environment``, created on first use with the default settings."""
    global _DEFAULT
    with _LOCK:
        if _DEFAULT is None:
            _DEFAULT = Environment()
        return _DEFAULT


def evaluate(query: Any) -> Any:
    """Evaluates ``query`` in the default environment and returns its value."""
    return default_environment().evaluate(query)


def evaluate_async(query: Any) -> Awaitable[Any]:
    """Awaitable ``evaluate`` in the default environment: ``await lq.evaluate_async(query)``."""
    return default_environment().evaluate_async(query)


def serve(**settings: Any) -> Server:
    """Serves the default environment over HTTP: ``lq.serve(port=0)``; see ``Environment.serve``."""
    return default_environment().serve(**settings)


def serve_async(**settings: Any) -> Coroutine[Any, Any, None]:
    """Awaitable ``serve`` of the default environment; see ``Environment.serve_async``."""
    return default_environment().serve_async(**settings)


async def _awaiting(awaitable: Awaitable[None]) -> None:
    """Awaits ``awaitable``: wraps a native future as a coroutine (``Environment.serve_async``)."""
    await awaitable

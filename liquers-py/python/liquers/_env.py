"""The process-wide default environment, used by ``lq.evaluate`` and the bare decorators."""

import threading
from collections.abc import Awaitable
from typing import Any

from liquers._native import Environment

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

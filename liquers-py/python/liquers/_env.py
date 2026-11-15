"""The process-wide default environment: the one ``lq.evaluate``, ``lq.serve``, ``lq.ui`` and the bare decorators use.

``default_environment()`` makes one ``lq.Environment`` on first use, under a lock, and returns it
from then on. ``configure(...)`` sets its settings, ``Environment.configure``'s keywords, at any
point before its first build, so it works after modules have already registered commands with
``@lq.command`` at import. The first evaluation, ``serve``, ``ui``, render or store write builds it,
and the settings are fixed from then on.

# Main components
- `default_environment` — the shared environment, made on first use.
- `configure` — sets its settings before the first build.
- `evaluate`, `evaluate_async`, `serve`, `serve_async`, `ui` — the same methods of the default
  environment, as module-level functions.

# Where it is used
- `liquers/__init__.py` re-exports every public name here at the top level.
- `liquers/_commands.py`: the bare decorators register in `default_environment()`.
- `liquers/_cli.py`: `configure_from_args` configures it from command-line options.

# Related concepts
- `liquers-py/src/environment.rs` (`EnvCell::configure`): the rule that settings change only
  until the first successful build.
- `specs/reference/PYTHON_INTEGRATION.md` §"Default environment".

# Example
```python
import liquers as lq

@lq.first_command                # registers in the default environment, as an import would
def hello() -> str:
    return "hello"

lq.configure(conversion="strict", job_capacity=2)
assert lq.evaluate("hello") == "hello"
```
"""

import threading
from collections.abc import Awaitable, Coroutine
from typing import Any

from liquers._native import UI, Environment, Server

_DEFAULT: Environment | None = None
_LOCK = threading.Lock()


def default_environment() -> Environment:
    """The shared ``Environment``, created on first use with the default settings."""
    global _DEFAULT
    with _LOCK:
        if _DEFAULT is None:
            _DEFAULT = Environment()
        return _DEFAULT


def configure(
    *,
    config: Any = None,
    overrides: dict[str, Any] | None = None,
    conversion: str | None = None,
    pickle: bool | None = None,
    async_loops: int | None = None,
    job_capacity: int | None = None,
) -> None:
    """Configures the default environment before its first build; see ``Environment.configure``.

    Commands already registered with ``@lq.command`` are kept. Raises ``ConfigurationError`` once
    the default environment has been built (by an evaluation, ``serve``, ``ui``, a render or a
    store write).
    """
    default_environment().configure(
        config=config,
        overrides=overrides,
        conversion=conversion,
        pickle=pickle,
        async_loops=async_loops,
        job_capacity=job_capacity,
    )


def evaluate(query: Any) -> Any:
    """Evaluates ``query`` (a query, or a pipeline) in the default environment and returns its value."""
    return default_environment().evaluate(query)


def evaluate_async(query: Any) -> Awaitable[Any]:
    """Awaitable ``evaluate`` in the default environment: ``await lq.evaluate_async(query)``."""
    return default_environment().evaluate_async(query)


def serve(**settings: Any) -> Server:
    """Serves the default environment over HTTP: ``lq.serve(port=0)``; see ``Environment.serve``."""
    return default_environment().serve(**settings)


def ui() -> UI:
    """A new UI model on the default environment; see ``Environment.ui``."""
    return default_environment().ui()


def serve_async(**settings: Any) -> Coroutine[Any, Any, None]:
    """Awaitable ``serve`` of the default environment; see ``Environment.serve_async``."""
    return default_environment().serve_async(**settings)


async def _awaiting(awaitable: Awaitable[None]) -> None:
    """Awaits ``awaitable``: wraps a native future as a coroutine (``Environment.serve_async``)."""
    await awaitable

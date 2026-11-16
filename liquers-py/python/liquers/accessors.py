"""Settable value accessors: read, write and watch a value where it lives, or a part of it.

Rust counterpart: `liquers_core::accessor`. Every name here is also available directly from
`liquers` (`liquers.Accessor is liquers.accessors.Accessor`); `import liquers as lq` and
`lq.Accessor` stays the usual style.

An accessor is a live handle, not data: on a stored key, a keyed asset, a document field, or one
argument of a recipe's query. `Environment.accessor(query)` makes one from a query (usually ending
in an `ns-acc` command), and `Environment.memory_accessor(value)` a shared variable. A write is a
compare-and-set on a token: `set_if(value, token)` raises `StatusConflictError` (whose `token` is
the current one) when someone else wrote first.

# Main components
- `Accessor`: `get`, `snapshot`, `set`, `set_if`, `info`, `log`, `describe`, `refresh`, `project`,
  the event methods `on`, `process_events`, `wait_event`, `events`, and their `_async` forms.
- `AccessorSnapshot`, `AccessorInfo`, `AccessorEvent`, `PartDescription`,
  `AccessorCapabilities`: what the methods return.
- `AccessorSubscription`: a callback registered with `on`; a context manager.

# Where it is used
- `liquers.environment.Environment.accessor` and `Environment.memory_accessor` make accessors; a
  command that returns one gives an `Accessor` too.

# Related concepts
- Callbacks run only when a thread asks for them (`process_events`, `wait_event`, iterating
  `events()`), as with `liquers.lui.UI`.
- The `/api/acc` routes of the HTTP server serve the same accessors.

# Examples
>>> import liquers as lq
>>> env = lq.Environment()
>>> counter = env.memory_accessor(1)
>>> token = counter.set(2)
>>> counter.get()
2
>>> env.shutdown()
"""

from collections.abc import AsyncIterator
from typing import Any

from liquers._backend import (
    Accessor,
    AccessorCapabilities,
    AccessorSnapshot,
    AccessorInfo,
    AccessorEvent,
    PartDescription,
    AccessorSubscription,
)

__all__ = [
    "Accessor",
    "AccessorCapabilities",
    "AccessorSnapshot",
    "AccessorInfo",
    "AccessorEvent",
    "PartDescription",
    "AccessorSubscription",
]


async def _events(accessor: Any) -> AsyncIterator[Any]:
    """`Accessor.events()`: every event from now on, each run through the callbacks first."""
    while True:
        yield await accessor.wait_event_async()

"""The awaitable form of `UI.wait_idle`, which delivers on the awaiting event loop's thread.

`lq.UI.wait_idle_async` calls `wait_idle_async` here: the loop runs in the coroutine, so status
callbacks run on the thread of the loop that awaits it, as the delivery rule says
(`liquers-py/src/ui.rs`, "Who runs the callbacks").

`_IN_COMMAND` and `_run_as_command` mark an `async def` command's coroutine, which the command
adapter (`liquers-py/src/command.rs`) wraps before scheduling it on a pool loop: `wait_idle_async`
inside it raises, because the UI may be waiting for that very command.
"""

from __future__ import annotations

import asyncio
import contextvars
from collections.abc import Coroutine
from typing import Any

_SLICE = 0.1

# Set while an `async def` command's coroutine runs (and in the tasks it starts): a
# `wait_idle_async` there is refused, since the UI may be waiting for that very command. The
# synchronous commands' mark is a thread-local in `liquers-py/src/ui.rs` (`ActionMark`).
_IN_COMMAND: contextvars.ContextVar[bool] = contextvars.ContextVar("liquers_in_command", default=False)


async def _run_as_command(coroutine: Coroutine[Any, Any, Any]) -> Any:
    """Runs an `async def` command's coroutine with the in-command mark set."""
    _IN_COMMAND.set(True)
    return await coroutine


async def wait_idle_async(ui: Any, timeout: float | None) -> bool:
    """Waits until `ui` is idle, delivering its status events; `False` when `timeout` passes."""
    loop = asyncio.get_running_loop()
    deadline = None if timeout is None else loop.time() + timeout
    while True:
        ui.process_events()
        if ui._idle():
            ui.process_events()
            return True
        if deadline is None:
            remaining = _SLICE
        else:
            remaining = deadline - loop.time()
            if remaining <= 0:
                return False
        await ui._next_tick_async(min(_SLICE, remaining))

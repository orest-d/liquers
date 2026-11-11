"""The awaitable form of `UI.wait_idle`, which delivers on the awaiting event loop's thread.

`lq.UI.wait_idle_async` calls `wait_idle_async` here: the loop runs in the coroutine, so status
callbacks run on the thread of the loop that awaits it, as the delivery rule says
(`liquers-py/src/ui.rs`, "Who runs the callbacks").
"""

from __future__ import annotations

import asyncio
from typing import Any

_SLICE = 0.1


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

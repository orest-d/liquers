"""ASYNCQ: awaitable evaluation (LANGUAGE-INTEGRATION_GUIDE.md §ASYNCQ; specs/design/liquers-py-async/).

The tests use plain `asyncio.run` (through `run_async`, which bounds a hang), so no async pytest
plugin is needed. The commands here are sync: awaiting an evaluation does not depend on async
commands, which `test_ASYNCCMD_commands.py` covers.
"""

import asyncio
import gc
import threading
import time

import pytest

import liquers as lq
from conftest import na, run_async


def test_ASYNCQ01_await_successful_evaluation(env):
    @env.command
    def repeat(text: str, n: int = 2) -> str:
        return text * n

    async def main():
        return await env.evaluate_async("hello/repeat-3")

    assert run_async(main) == "hellohellohello"


def test_ASYNCQ02_failure_raises_structured_error(env):
    async def main():
        # Parsed at call time, so the error is raised by the call itself, not by the await.
        with pytest.raises(lq.ParseError) as raised:
            env.evaluate_async("a/b-~")
        assert raised.value.type == lq.ErrorType.ParseError
        with pytest.raises(lq.ExecutionError):
            await env.evaluate_async("hello/boom")

    run_async(main)


def test_ASYNCQ03_two_evaluations_make_progress(env):
    env.evaluate("hello")

    async def main():
        start = time.monotonic()
        a, b = await asyncio.gather(env.evaluate_async("sleepy-1"), env.evaluate_async("sleepy-2"))
        return a, b, time.monotonic() - start

    a, b, elapsed = run_async(main)
    assert (a, b) == (1, 2)
    assert elapsed < 0.75, f"two 0.5 s evaluations took {elapsed:.2f} s together"


def test_ASYNCQ04_cancelling_the_awaitable_detaches(env):
    async def main():
        waiting = asyncio.ensure_future(env.evaluate_async("sleepy-9"))
        await asyncio.sleep(0.05)
        waiting.cancel()
        with pytest.raises(asyncio.CancelledError):
            await waiting

    run_async(main)
    # Cancelling the await did not cancel the evaluation: the asset finishes, and the second
    # request reuses it.
    asset = env.get_asset("sleepy-9")
    assert asset.get() == 9
    assert asset.status == "Ready"


def test_ASYNCQ05_dropping_the_handle_detaches(env):
    finished = threading.Event()

    @env.first_command(volatile=True)
    def tracked() -> str:
        time.sleep(0.2)
        finished.set()
        return "done"

    async def main():
        future = env.evaluate_async("tracked")
        del future
        gc.collect()
        # The policy is detach: the evaluation runs to its end with nobody waiting for it.
        for _ in range(200):
            if finished.is_set():
                return True
            await asyncio.sleep(0.01)
        return False

    assert run_async(main)


def test_ASYNCQ06_no_event_loop_blocking(env):
    env.evaluate("hello")

    async def main():
        ticks = 0

        async def ticker():
            nonlocal ticks
            while True:
                await asyncio.sleep(0.01)
                ticks += 1

        task = asyncio.create_task(ticker())
        assert await env.evaluate_async("sleepy-3") == 3
        task.cancel()
        return ticks

    assert run_async(main) >= 10, "the loop stalled while the evaluation ran"


@na("Python has an async model (`async`/`await`), so no blocking workaround is needed")
def test_ASYNCQ07_documented_non_async_workaround():
    pass


def test_ASYNCQ08_blocking_call_on_a_foreign_loop_warns_and_works(env):
    async def main():
        with pytest.warns(RuntimeWarning, match="evaluate_async"):
            return env.evaluate("hello")

    # Run here, not on a thread: `pytest.warns` watches this thread's warnings.
    assert asyncio.run(main()) == "hello"


def test_ASYNCQ_every_awaitable_method_round_trips(env):
    async def main():
        state = await env.evaluate_state_async("hello")
        assert state.value == "hello"
        await env.store_set_async("data/x.txt", "abc")
        asset = await env.get_asset_async("-R/data/x.txt/-/to_text")
        assert isinstance(asset, lq.AssetRef)
        assert await asset.get_async() == "abc"
        assert (await asset.get_state_async()).value == "abc"
        finished = env.evaluate_async("hello")
        assert await finished == "hello"
        assert await finished == "hello"  # an asyncio.Future can be awaited again

    run_async(main)


def test_ASYNCQ_async_call_without_running_loop_raises(env):
    with pytest.raises(RuntimeError):
        env.evaluate_async("hello")


def test_ASYNCQ_module_level_evaluate_async():
    # The default environment has no fixture commands, so the query is a built-in.
    async def main():
        return await lq.evaluate_async("commands_doc")

    assert "to_text" in run_async(main)

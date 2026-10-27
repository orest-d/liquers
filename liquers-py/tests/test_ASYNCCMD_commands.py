"""ASYNCCMD: `async def` commands (LANGUAGE-INTEGRATION_GUIDE.md §ASYNCCMD; specs/design/liquers-py-async/).

An `async def` command runs on the environment's own event loop, not the caller's, and is awaited
there; the evaluation that runs it may be awaited or blocking.
"""

import asyncio

import pytest

import liquers as lq
from conftest import CALLS, run_async


def test_ASYNCCMD01_async_command_result(env):
    @env.command
    async def shout(text: str) -> str:
        await asyncio.sleep(0)
        return text.upper()

    async def main():
        return await env.evaluate_async("hello/shout")

    assert run_async(main) == "HELLO"
    assert env.evaluate("hello/shout") == "HELLO"


def test_ASYNCCMD02_async_exception(env):
    @env.command
    async def fail_async(state):
        await asyncio.sleep(0)
        raise ValueError("inner async")

    async def main():
        with pytest.raises(lq.ExecutionError) as raised:
            await env.evaluate_async("hello/fail_async")
        return raised.value

    error = run_async(main)
    assert error.type == lq.ErrorType.ExecutionError
    assert "inner async" in error.message
    assert "fail_async" in (error.traceback or "")


def test_ASYNCCMD03_cancellation_in_both_directions(env):
    async def main():
        asset = await env.get_asset_async("hang")
        assert await asyncio.to_thread(CALLS["started"].wait, 10)
        await asset.cancel_async()
        with pytest.raises(lq.CancelledError):
            await asset.get_async()
        return asset

    asset = run_async(main)
    assert CALLS["saw_cancel"].wait(10), "the coroutine never saw the cancellation"
    assert asset.status == "Cancelled"


def test_ASYNCCMD03_coroutine_cancelling_itself_is_cancelled(env):
    @env.first_command(volatile=True)
    async def give_up():
        raise asyncio.CancelledError()

    asset = env.get_asset("give_up")
    with pytest.raises(lq.CancelledError):
        asset.get()
    assert asset.status == "Cancelled"


def test_ASYNCCMD04_nested_async_evaluation():
    # One job slot: the nested evaluation must not wait for the slot its parent holds.
    env = lq.Environment(job_capacity=1)
    try:
        @env.first_command
        def hello() -> str:
            return "hello"

        @env.command
        async def fetch_len(text: str) -> int:
            await asyncio.sleep(0.01)
            return len(text)

        @env.first_command
        async def outer_async(context) -> str:
            return f"outer:{await context.evaluate_async('hello/fetch_len')}"

        async def main():
            return await env.evaluate_async("outer_async")

        assert run_async(main) == "outer:5"
    finally:
        env.shutdown()


def test_ASYNCCMD05_concurrent_calls_do_not_corrupt_state(env):
    @env.command
    async def idem(n: int) -> int:
        await asyncio.sleep(0)
        return n * 2

    async def main():
        return await asyncio.gather(*[env.evaluate_async(f"number-{i}/idem") for i in range(50)])

    assert run_async(main) == [i * 2 for i in range(50)]


def test_ASYNCCMD06_sync_and_async_metadata_differ(env):
    @env.command
    def plain_s(state):
        return state

    @env.command
    async def plain_a(state):
        return state

    assert env.command_metadata("plain_s").is_async is False
    assert env.command_metadata("plain_a").is_async is True


def test_ASYNCCMD_sync_function_returning_coroutine_fails(env):
    async def later(state):
        return state

    @env.command
    def sneaky(state):
        return later(state)

    with pytest.raises(lq.ExecutionError, match="async def"):
        env.evaluate("hello/sneaky")


def test_ASYNCCMD_async_callable_object_detected(env):
    class Doubler:
        async def __call__(self, n: int) -> int:
            await asyncio.sleep(0)
            return n * 2

    env.command(name="double_async")(Doubler())
    assert env.command_metadata("double_async").is_async is True
    assert env.evaluate("number-4/double_async") == 8

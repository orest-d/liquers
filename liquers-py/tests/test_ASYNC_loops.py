"""The event loops `async def` commands run on, the blocking-call guard, the `async_loops` and
`job_capacity` keywords, and shutdown (specs/design/liquers-py-async/, AC-3, AC-5 to AC-7)."""

import asyncio
import subprocess
import sys
import textwrap
import threading
import time

import pytest

import liquers as lq
from conftest import CALLS, run_async


def test_ASYNC_blocking_call_on_pool_loop_raises(env):
    @env.first_command(volatile=True)
    async def blocking_inside(which: str, context) -> str:
        calls = {
            "context": lambda: context.evaluate("hello"),
            "env": lambda: env.evaluate("hello"),
            "asset": lambda: env.get_asset("hello").get(),
            "shutdown": env.shutdown,
        }
        try:
            calls[which]()
        except RuntimeError as e:
            return f"raised: {e}"
        return "did not raise"

    for which in ("context", "env", "asset"):
        assert env.evaluate(f"blocking_inside-{which}").startswith("raised: ")
    assert "async command" in env.evaluate("blocking_inside-shutdown")
    # The environment was not shut down by the refused call.
    assert env.evaluate("hello") == "hello"

    @env.first_command(volatile=True)
    async def uncaught(context) -> str:
        return context.evaluate("hello")

    with pytest.raises(lq.ExecutionError, match="evaluate_async"):
        env.evaluate("uncaught")


@pytest.mark.parametrize(
    "keywords",
    [
        {"async_loops": 0},
        {"async_loops": -1},
        {"job_capacity": 0},
        {"job_capacity": 2, "config": {"assets": {"job_capacity": 3}}},
    ],
)
def test_ASYNC_invalid_pool_and_capacity_rejected(keywords):
    with pytest.raises(lq.ConfigurationError):
        lq.Environment(**keywords)


def test_ASYNC_job_capacity_keyword_limits_running_commands():
    env = lq.Environment(job_capacity=1)
    try:
        running = []
        overlap = []
        lock = threading.Lock()

        @env.first_command(volatile=True)
        def exclusive(i: int = 0) -> int:
            with lock:
                running.append(i)
                overlap.append(len(running))
            time.sleep(0.3)
            with lock:
                running.remove(i)
            return i

        env.evaluate("exclusive-0")
        threads = [threading.Thread(target=env.evaluate, args=(f"exclusive-{i}",)) for i in (1, 2)]
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join(10)
        assert max(overlap) == 1, "two commands ran at once with job_capacity=1"
    finally:
        env.shutdown()


def test_ASYNC_pool_spreads_commands_over_loops():
    env = lq.Environment(async_loops=4, job_capacity=8)
    try:
        barrier = threading.Barrier(4, timeout=10)

        @env.first_command(volatile=True)
        async def meet(i: int = 0):
            before = threading.get_ident()
            # Each of the four calls blocks a worker thread, not its loop, until all four arrive.
            await asyncio.to_thread(barrier.wait)
            return [before, threading.get_ident()]

        async def main():
            return await asyncio.gather(*[env.evaluate_async(f"meet-{i}") for i in range(4)])

        threads = run_async(main)
        assert all(before == after for before, after in threads)
        assert len({before for before, _ in threads}) == 4
    finally:
        env.shutdown()


def test_ASYNC_default_pool_has_one_loop(env):
    async def main():
        return await asyncio.gather(env.evaluate_async("where_async-1"), env.evaluate_async("where_async-2"))

    (b1, a1), (b2, a2) = run_async(main)
    assert b1 == a1 == b2 == a2


def test_ASYNC_shutdown_ends_async_waits():
    from conftest import register_fixture_commands

    env = lq.Environment(job_capacity=1)
    register_fixture_commands(env)

    async def main():
        running = asyncio.ensure_future(env.evaluate_async("slow-5"))
        queued = asyncio.ensure_future(env.evaluate_async("slow-6"))
        await asyncio.sleep(0.2)
        start = time.monotonic()
        await asyncio.to_thread(env.shutdown)
        for waiting in (running, queued):
            with pytest.raises(lq.CancelledError):
                await waiting
        return time.monotonic() - start

    assert run_async(main) < 3


def test_ASYNC_shutdown_cancels_running_coroutine():
    from conftest import register_fixture_commands

    env = lq.Environment()
    register_fixture_commands(env)
    asset = env.get_asset("hang")
    assert CALLS["started"].wait(10)
    env.shutdown()
    assert CALLS["saw_cancel"].wait(10), "shutdown did not cancel the running coroutine"
    with pytest.raises(lq.LiquersError):
        asset.get()


def test_ASYNC_shutdown_does_not_hang_on_a_stubborn_coroutine():
    env = lq.Environment()
    started = threading.Event()

    @env.first_command(volatile=True)
    async def stubborn() -> str:
        started.set()
        while True:
            try:
                await asyncio.sleep(3600)
            except asyncio.CancelledError:
                pass  # ignores every cancellation

    env.get_asset("stubborn")
    assert started.wait(10)
    start = time.monotonic()
    with pytest.warns(RuntimeWarning, match="ignoring cancellation"):
        env.shutdown()
    # The stop grace is 5 s.
    assert time.monotonic() - start < 5 + 2


def test_ASYNC_interpreter_exit_with_live_pool():
    script = textwrap.dedent(
        """
        import asyncio
        import liquers as lq

        env = lq.Environment(async_loops=2)

        @env.first_command
        async def nap() -> str:
            await asyncio.sleep(0.01)
            return "ok"

        assert env.evaluate("nap") == "ok"
        print("exiting")
        """
    )
    start = time.monotonic()
    completed = subprocess.run(
        [sys.executable, "-c", script], capture_output=True, text=True, timeout=15
    )
    assert completed.returncode == 0, completed.stderr
    assert completed.stdout.strip() == "exiting"
    assert time.monotonic() - start < 15

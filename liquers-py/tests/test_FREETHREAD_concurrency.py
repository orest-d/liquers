"""FREETHREAD: the free-threaded audit of liquers-py (specs/design/liquers-py-async/, AC-8).

A scenario that can deadlock runs in a subprocess with a timeout: a deadlocked thread may hold the
GIL, so a thread-and-join check inside this process would hang the whole suite instead of failing.
"""

import asyncio
import subprocess
import sys
import textwrap
import threading
import time

import pytest

import liquers as lq


def run_isolated(script, timeout=30):
    """Runs ``script`` in a fresh interpreter; fails the test if it hangs or exits non-zero."""
    try:
        completed = subprocess.run(
            [sys.executable, "-c", textwrap.dedent(script)],
            capture_output=True,
            text=True,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired:
        raise AssertionError(f"the scenario did not finish within {timeout} s (deadlock)")
    assert completed.returncode == 0, completed.stderr
    return completed.stdout


def test_FREETHREAD03_finalizer_reentry_does_not_deadlock():
    # Re-registering a key drops the replaced, never-built command. If that happens while the
    # environment's lock is held, the callable's __del__ re-enters the environment on the same
    # thread and waits on the lock forever.
    out = run_isolated(
        """
        import gc
        import liquers as lq

        env = lq.Environment()
        finalized = []

        class Reentrant:
            def __call__(self, state):
                return state

            def __del__(self):
                env.validate()
                finalized.append(True)

        env.command(name="reentrant")(Reentrant())
        gc.collect()

        @env.command(name="reentrant")
        def plain(state):
            return state

        gc.collect()
        env.shutdown()
        print("finalized" if finalized else "kept")
        """
    )
    assert out.strip() == "finalized"


def _free_threaded():
    import sysconfig

    return bool(sysconfig.get_config_var("Py_GIL_DISABLED")) and not sys._is_gil_enabled()


def _cpus():
    import os

    return len(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else (os.cpu_count() or 1)


free_threaded_only = pytest.mark.skipif(
    not _free_threaded(), reason="needs a free-threaded interpreter running without the GIL"
)
needs_four_cpus = pytest.mark.skipif(_cpus() < 4, reason="needs at least 4 CPUs to show parallelism")


def _spin(seconds):
    """Pure-Python CPU work for about ``seconds``: only runs in parallel without a GIL."""
    deadline = time.perf_counter() + seconds
    n = 0
    while time.perf_counter() < deadline:
        n += 1
    return n


@free_threaded_only
def test_FREETHREAD01_gil_stays_disabled_after_import():
    out = run_isolated(
        """
        import sys
        import liquers as lq

        lq.Environment().evaluate("commands_doc")
        print(sys._is_gil_enabled())
        """
    )
    assert out.strip() == "False"


def _elapsed(run):
    start = time.perf_counter()
    run()
    return time.perf_counter() - start


@free_threaded_only
@needs_four_cpus
def test_FREETHREAD02_sync_commands_run_in_parallel():
    env = lq.Environment(job_capacity=8)
    try:
        @env.first_command(volatile=True)
        def burn(i: int = 0) -> int:
            _spin(0.5)
            return i

        env.evaluate("burn-0")
        one = _elapsed(lambda: env.evaluate("burn-1"))

        def four():
            threads = [threading.Thread(target=env.evaluate, args=(f"burn-{i}",)) for i in range(2, 6)]
            for thread in threads:
                thread.start()
            for thread in threads:
                thread.join(30)

        assert _elapsed(four) < 2.5 * one
    finally:
        env.shutdown()


@free_threaded_only
@needs_four_cpus
def test_FREETHREAD04_async_commands_on_several_loops_in_parallel():
    env = lq.Environment(async_loops=4, job_capacity=8)
    try:
        @env.first_command(volatile=True)
        async def burn_async(i: int = 0) -> int:
            _spin(0.5)
            return i

        env.evaluate("burn_async-0")
        one = _elapsed(lambda: env.evaluate("burn_async-1"))

        async def gather():
            return await asyncio.gather(*[env.evaluate_async(f"burn_async-{i}") for i in range(2, 6)])

        assert _elapsed(lambda: asyncio.run(gather())) < 2.5 * one
    finally:
        env.shutdown()

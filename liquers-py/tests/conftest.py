"""Shared fixtures for the conformance suites (LANGUAGE-INTEGRATION_GUIDE.md, Appendix A)."""

import asyncio
import threading
import time

import pytest

import liquers as lq

# The installed-wheel tests (liquers-py-package) have their own conftest and run only against a
# wheel in a fresh venv, from the release workflow; the development loop does not collect them.
collect_ignore = ["package"]


def na(reason):
    """A feature marked NA keeps its tests present, skipped with the reason the design records."""
    return pytest.mark.skip(reason=f"NA: {reason}")


class Opaque:
    """A user class with no structural mapping, compared by its payload."""

    def __init__(self, payload):
        self.payload = payload

    def __eq__(self, other):
        return isinstance(other, Opaque) and other.payload == self.payload

    def __hash__(self):
        return hash(self.payload)


#: Call counters of the fixture commands, reset by every ``env`` fixture.
CALLS = {}


def register_fixture_commands(e):
    """The source and helper commands the suites draw their inputs from."""
    calls = CALLS
    calls["slow"] = 0

    @e.first_command
    def hello() -> str:
        return "hello"

    @e.first_command
    def number(n: int = 0) -> int:
        return n

    @e.command
    def boom(state):
        raise ValueError("inner")

    @e.first_command
    def make_obj():
        return Opaque("payload")

    @e.command
    def same_obj(value):
        return value

    @e.first_command
    def outer(context):
        return "outer:" + context.evaluate("hello")

    @e.first_command(volatile=True)
    def slow(seconds: float = 60.0, context=None):
        calls["slow"] += 1
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            if context.is_cancelled():
                raise lq.CancelledError("slow was cancelled")
            time.sleep(0.01)
        return "done"

    @e.first_command
    def sleepy(i: int = 0) -> int:
        time.sleep(0.5)
        return i

    @e.first_command
    async def nap(i: int = 0) -> int:
        await asyncio.sleep(0.5)
        return i

    calls["started"] = threading.Event()
    calls["saw_cancel"] = threading.Event()

    @e.first_command(volatile=True)
    async def hang():
        calls["started"].set()
        try:
            await asyncio.sleep(3600)
        except asyncio.CancelledError:
            calls["saw_cancel"].set()
            raise

    @e.first_command(volatile=True)
    async def where_async(i: int = 0):
        before = threading.get_ident()
        await asyncio.sleep(0.01)
        return [before, threading.get_ident()]



@pytest.fixture
def env():
    """An isolated environment per test (ENVIRON05), with the fixture commands registered."""
    e = lq.Environment()
    register_fixture_commands(e)
    yield e
    e.shutdown()


def run_async(main, timeout=30):
    """Runs ``asyncio.run(main())`` on a thread and returns its result, failing the test if it has
    not finished after ``timeout`` seconds instead of hanging the suite."""
    outcome = {}

    def target():
        try:
            outcome["value"] = asyncio.run(main())
        except BaseException as e:  # re-raised on the test's thread
            outcome["error"] = e

    thread = threading.Thread(target=target, daemon=True)
    thread.start()
    thread.join(timeout)
    if thread.is_alive():
        raise AssertionError(f"the async test body did not finish within {timeout} s")
    if "error" in outcome:
        raise outcome["error"]
    return outcome.get("value")

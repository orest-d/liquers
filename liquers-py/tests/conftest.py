"""Shared fixtures for the conformance suites (LANGUAGE-INTEGRATION_GUIDE.md, Appendix A)."""

import time

import pytest

import liquers as lq


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



@pytest.fixture
def env():
    """An isolated environment per test (ENVIRON05), with the fixture commands registered."""
    e = lq.Environment()
    register_fixture_commands(e)
    yield e
    e.shutdown()

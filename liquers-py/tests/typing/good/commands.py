"""Decorated commands keep their signatures (Phase 3, Example 1)."""

from typing import Any, assert_type

import liquers as lq


@lq.command
def repeat(text: str, count: int = 2) -> str:
    return text * count


@lq.first_command
def source(n: int) -> int:
    return n


env = lq.Environment()


@env.command
def bare(value: float) -> float:
    return value


@env.command(name="twice")
def twice(value: int) -> int:
    return value * 2


@env.first_command
async def fetch(url: str) -> bytes:
    return b""


@lq.command(namespace="ns", label="Shout", doc="Upper case.", filename="x.txt", volatile=True,
            cached=False, env=env)
def shout(text: str, context: lq.Context) -> str:
    context.info("shouting")
    return text.upper()


assert_type(repeat("a", 3), str)
assert_type(source(1), int)
assert_type(bare(1.0), float)
assert_type(twice(2), int)


async def uses_fetch() -> None:
    assert_type(await fetch("u"), bytes)


q = lq.parse("a/b")
assert_type(q, lq.Query)
assert_type(env.evaluate(q), Any)
assert_type(env.evaluate_state("a"), lq.State)
assert_type(env.command_metadata("twice"), lq.CommandMetadata)
assert_type(env.unregister("twice"), bool)

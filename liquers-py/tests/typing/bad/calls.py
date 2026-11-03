"""Wrong calls, each an error under both checkers (Phase 3, Example 1).

Every intended error carries both checkers' ignore comments. `test_bad_samples_fail_exactly`
checks this file as written (no error: every ignore is used), then a copy without the comments
(errors on exactly the marked lines).
"""

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


lq.parse(42)  # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]
lq.Environment(pickles=True)  # type: ignore[call-arg]  # pyright: ignore[reportCallIssue]
lq.Environment(conversion="lenient")  # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]
repeat()  # type: ignore[call-arg]  # pyright: ignore[reportCallIssue]
repeat(1)  # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]
source("x")  # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]
bare("x")  # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]
twice("x")  # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]
lq.RecordView.from_records([{"a": 1}], fields={"a": 1})  # type: ignore[dict-item]  # pyright: ignore[reportArgumentType]


async def f() -> None:
    await fetch(1)  # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]
    await env.evaluate_state("a")  # type: ignore[misc]  # pyright: ignore[reportGeneralTypeIssues]

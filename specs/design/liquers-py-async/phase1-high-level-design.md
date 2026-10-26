# Phase 1: High-Level Design — liquers-py-async

## Purpose

Let asyncio programs await evaluations and register `async def` commands (`ASYNCQ`, `ASYNCCMD`),
without Python slowing queries that never touch it (`VISION.md`, Python data science).

## Problem Example

```python
@lq.command
async def fetch_len(text: str) -> int:
    await asyncio.sleep(0)
    return len(text)

assert await lq.evaluate_async("hello/fetch_len") == 5
```

`hello`, `repeat`: `tests/test_EVAL_evaluation.py`. **Today** evaluation is sync only and
`async def` commands are unsupported. **After** both work without blocking the caller's loop.

## Scope and Acceptance Criteria

- **AC-1** Await an evaluation (was `liquers-py` AC-14)
  WHEN `await lq.evaluate_async("hello/repeat-3")` runs in an asyncio program
  THEN it returns `"hellohellohello"` without blocking the loop; cancelling it: Decision 4
- **AC-2** Async Python commands (was `liquers-py` AC-15)
  WHEN a command is declared as `async def`
  THEN it is awaited, its metadata marks it async, and its exceptions map as in `liquers-py` AC-7
- **AC-3** No blocking inside liquers-py's own loops (was `liquers-py` AC-16)
  WHEN any blocking call of the binding (`evaluate`, `evaluate_state`, `context.evaluate`, an
  `AssetRef` wait) is made on one of liquers-py's loop threads
  THEN it raises `RuntimeError`; on any other running loop (Jupyter) it works with a `RuntimeWarning`
- **AC-4** Nested async evaluation
  WHEN an `async def` command awaits `context.evaluate_async(...)` on a query that runs another
  `async def` command
  THEN both complete without deadlock (blocking instead of awaiting is AC-3's error)
- **AC-5** Pure-Rust queries never wait on Python
  WHEN a query whose commands are all Rust is evaluated while a Python command holding the GIL is
  still running
  THEN it completes first; the GIL is never taken on a runtime worker; they share only job-queue
  capacity (Decision 5)
- **AC-6** A pool of command loops
  WHEN an environment is built with `Environment(async_loops=4)` and several `async def` commands
  run at once
  THEN they spread over four loop threads, each command wholly on one; the default is one
- **AC-7** Cancellation reaches the coroutine
  WHEN the asset running an `async def` command is cancelled (`AssetRef.cancel()`, replacement,
  `Environment.shutdown()`)
  THEN the command's asyncio task is cancelled on its loop and the asset ends `Cancelled`
- **AC-8** Free-threaded support is verified
  WHEN liquers-py is imported on a free-threaded CPython (3.14t) and its suite runs, with Python
  commands on several threads and loops at once
  THEN the GIL stays disabled and every test passes; shared state reached from Python is audited
  for races; until then the module declares `gil_used = true`

Non-goals: async stores and recipe providers; the caller's loop; several job queues
(`CORE-ONE-JOB-QUEUE-FOR-ALL-COMMANDS`).

## Core Interactions

- Commands: coroutines awaited on liquers-py's loops. Assets: cancellation both ways
  (`AssetRef::cancel`). Bindings: `async_loops`; an awaitable for every blocking call (Phase 2).

## Crate Placement

`liquers-py` only (`src/runtime.rs`, `src/command.rs`). `pyo3-async-runtimes` (`tokio-runtime`) is set once
with `tokio::init_with_runtime` to the existing runtime, `TaskLocals` per loop; it tracks PyO3
(0.30.0 needs `pyo3 ^0.30`), so PyO3 upgrades bump both.

## Documentation Intent

- Reference: `specs/reference/PYTHON_INTEGRATION.md`. Guides: `specs/guides/PYTHON_GUIDE.md`,
  `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §6. Also `liquers-py/README.md`, `specs/README.md`.

## Decisions

Recorded with Orest on 2026-10-26:

1. **Bridge:** `pyo3-async-runtimes`, not own code; it tracks PyO3 and bridges both directions.
2. **Command loops:** a liquers-py-owned pool, one by default (AC-6). Runs start on their own tokio
   task (`liquers-core/src/assets.rs` `JobQueue`), so the caller's loop is unreachable (and absent
   over HTTP); the pool gives parallelism on free-threaded Python.
3. **Sync on a running loop:** raise on liquers-py's loops, warn elsewhere (AC-3).
4. **Cancelling an awaiting task** ends only that wait, like sync `block_on`; `await
   asset.cancel_async()` cancels the evaluation.
5. **Job queue:** one queue, capacity exposed as `Environment(job_capacity=…)` (core
   `AssetManagerOptions::with_job_capacity`). Queues per kind of command are future configuration
   (`CORE-ONE-JOB-QUEUE-FOR-ALL-COMMANDS`).
6. **Free-threaded Python** is in scope (AC-8): audited and tested here.

## Open Questions

None. Loop choice and pool lifetime (an implementation detail) are settled in Phase 2: round-robin,
stopped by `shutdown()` and at exit.

## Design Dependencies

- `PY-FREE-THREADED-SUPPORT-UNVERIFIED` — **absorbed** (AC-8).
- `CORE-ONE-JOB-QUEUE-FOR-ALL-COMMANDS` — **overlaps**: would replace `job_capacity`.
- `executor-neutral-core` — **overlaps** (weak): the bridge relies on tokio.

## Scope Changes

- **2026-10-26** — added AC-5, AC-6 (Orest), AC-7 (`ASYNCCMD03`); widened AC-3. Phases: 1.
- **2026-10-26** — Orest: AC-8 (free-threaded audit) added; questions 1-2 decided. Phases: 1.
- **2026-10-26** — test-script fixes and a free-threading classifier absorbed for AC-8. Phases: 2.

## References

- `specs/design/liquers-py/` (scope moved from it on 2026-10-18)
- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §ASYNCQ, §ASYNCCMD, Appendix A
- `PY-ASYNC-EVALUATION-NOT-SUPPORTED`

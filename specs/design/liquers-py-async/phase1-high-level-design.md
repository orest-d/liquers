# Phase 1: High-Level Design — liquers-py-async

## Purpose

Let asyncio programs await Liquers evaluations and register `async def` functions as commands,
completing the guide's `ASYNCQ` and `ASYNCCMD` features for `liquers-py`.

## Problem Example

```python
@lq.command
async def fetch_len(text: str) -> int:
    await asyncio.sleep(0)
    return len(text)

assert await lq.evaluate_async("hello/fetch_len") == 5
```

**Today** (after `liquers-py`) `lq.evaluate` is synchronous only, and an `async def` command is
not supported. **After** this design both work, and cancelling the awaiting task cancels the
evaluation.

## Scope and Acceptance Criteria

- **AC-1** Await an evaluation (was `liquers-py` AC-14)
  WHEN `await lq.evaluate_async("hello/repeat-3")` runs in an asyncio program
  THEN it returns `"hellohellohello"` without blocking the loop, and cancelling the task cancels the
  evaluation
- **AC-2** Async Python commands (was `liquers-py` AC-15)
  WHEN a command is declared as `async def`
  THEN it is awaited, its metadata marks it async, and its exceptions map as in `liquers-py` AC-7
- **AC-3** No blocking inside the loop (was `liquers-py` AC-16)
  WHEN the synchronous `lq.evaluate` or `context.evaluate` is called from a coroutine on a running
  loop
  THEN it raises `RuntimeError` pointing at the async variant
- **AC-4** Nested async evaluation
  WHEN an `async def` command awaits `context.evaluate_async(...)` on a query that runs another
  `async def` command
  THEN both complete without deadlock

Non-goals: async stores and recipe providers (STORE/RECIPE design).

## Core Interactions

- Commands: async closures awaiting a Python coroutine.
- Assets: asyncio cancellation maps to `AssetRef::cancel`.

## Crate Placement

`liquers-py` only, on top of `liquers-py`'s runtime and command adapter. The bridge is liquers-py's
own code, not `pyo3-async-runtimes`, so the PyO3 version stays independent.

## Documentation Intent

- Reference: none.
- Guide: `LANGUAGE-INTEGRATION_GUIDE.md` §6 Python (async notes).
- Other documents: `liquers-py/README.md` matrix rows for ASYNCQ and ASYNCCMD.
- Documents to update: `specs/README.md`.

## Open Questions

1. **Open design: where an `async def` command runs.** Recommended: on a background loop thread
   owned by liquers-py, because the asset manager offers no route back to the caller's loop
   (`Context` carries no Python handle, and its payload slot changes caching). The cost is that a
   coroutine cannot use objects bound to the caller's loop, such as a client session created there.
   The alternative carries the caller's loop through the evaluation.

## References

- `specs/design/liquers-py/` (scope moved from it on 2026-10-18)
- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §ASYNCQ, §ASYNCCMD, Appendix A

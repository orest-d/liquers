---
id: PY-ASYNC-EVALUATION-NOT-SUPPORTED
kind: feature
title: liquers-py has no awaitable evaluation and cannot register async def commands
status: draft
priority: P3
complexity: L
area: [py]
design: liquers-py-async
created: 2026-10-20
github:
---

## Problem

Every waiting call of `liquers-py` (`Environment.evaluate`, `evaluate_state`, `get_asset`,
`AssetRef.get`, `Context.evaluate`) blocks on the process runtime (`block_on`,
`liquers-py/src/environment.rs`); there is no awaitable variant, and `@lq.command` on an
`async def` function registers a callable whose result is an un-awaited coroutine. The guide's
`ASYNCQ` (Profile) and `ASYNCCMD` (Optional) features are therefore absent
(`liquers-py/README.md` `EVAL` and `COMMAND` rows; `reference/PYTHON_INTEGRATION.md` §8).

## Impact

An asyncio program must call Liquers from a thread executor, and a command cannot await I/O.
Synchronous use, Jupyter included, is unaffected.

## Expected behaviour

As `design/liquers-py-async/` Phase 1 states: awaitable variants of the blocking calls, without
blocking the loop; `async def` commands awaited on a pool of loops owned by liquers-py; cancellation
in both directions; nested async evaluation without deadlock; pure-Rust queries never waiting on
Python.

## Discovery

Filed 2026-10-20 during the bulk-design pass so that `design/liquers-py-async/` (created
2026-10-18 with `issues: []`, split from `design/liquers-py/`) has a source issue.

# Phase 5: Documentation - liquers-py-async

## Completion Preconditions

- [x] Implementation is finished and validated (CPython 3.13 and 3.14t, below)
- [x] All user comments are answered or incorporated (Orest's 2026-10-26 decisions; `proceed all`)
- [x] All review comments are answered or incorporated (two reviewer passes per phase; PR #181)
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR when practical (PR #181)

## Implementation Summary

`liquers-py` now has an asyncio surface, and it has been audited and tested on free-threaded CPython.

- **Awaitable evaluation (AC-1).** The awaitable methods are:
  - on `Environment`: `evaluate_async`, `evaluate_state_async`, `get_asset_async`, `store_set_async`;
  - on `AssetRef`: `get_async`, `get_state_async`, `cancel_async`;
  - on `Context`: `evaluate_async`;
  - at module level: `lq.evaluate_async`.

  Each returns an `asyncio.Future` of the caller's loop. The future comes from `pyo3-async-runtimes`,
  which is bound to liquers-py's own runtime. Cancelling the future detaches the evaluation.
- **`async def` commands (AC-2, AC-4, AC-7).** An `async def` command is detected at registration,
  which sets `is_async` in its metadata. It runs on the environment's loop pool
  (`Environment(async_loops=N)`, default 1). Cancelling the asset, replacing it or shutting down
  cancels the coroutine. A coroutine that cancels itself ends `Cancelled`.
- **Blocking-call rule (AC-3).** A blocking call made on a pool thread raises `RuntimeError`. Under
  any other running loop it warns.
- **Capacity (AC-5, AC-6).** `Environment(job_capacity=N)` sets how many commands run at once.
  Every command shares one queue, as decided.
- **Free-threading (AC-8).**
  - The audit found one deadlock, in the environment lock: a `__del__` re-entered the lock on the
    same thread, and waiters held the GIL. It is fixed.
  - The module now declares `gil_used = false` explicitly, and the free-threading classifier is added.
  - The test script is fixed so it runs on 3.14t.

The implementation matches the approved design. These additions and differences change no contract:

- An async command's wait also races the shutdown signal. Without this, a coroutine that ignores
  cancellation would keep its asset running after `shutdown()`.
- `LoopThread` holds the loop object rather than `TaskLocals`.
- `Pending::store` hands a refused future back, so it is cancelled after the lock is released.
- When scheduling fails, the coroutine is closed, so Python does not warn that it was never awaited.
- `LazyLoopPool` has a closed state: after `shutdown()` no pool can start.
- `RUNTIME05` now waits for a cancelled sync command that is still running to return before it
  counts handles.
- Steps 4 to 9 landed in one commit. The `Shut` type threads through every file those steps touch,
  so the intermediate states could not be tested separately.

Details are in [`reference/PYTHON_INTEGRATION.md`](../../reference/PYTHON_INTEGRATION.md) §3, §4
and §7, and in [`guides/PYTHON_GUIDE.md`](../../guides/PYTHON_GUIDE.md): §5 *Async commands*, and
§6 *Awaiting a query* and *Threads and free-threaded Python*.

## Documentation Delivered

### New Reference Documents
None. The async contract extends the existing Python reference, as Phase 2 planned.

### New Guide Documents
None. The existing guide gained new sections, as planned.

### Existing Documents Reviewed or Updated
`affects_docs` is authoritative. It lists:
- `reference/PYTHON_INTEGRATION.md`
- `guides/PYTHON_GUIDE.md`
- `guides/LANGUAGE-INTEGRATION_GUIDE.md`

Each was reviewed against the code and tests and then updated. Each has a 2026-10-27 History row
and `reviewed:` date. In the guide, the stale limitation "memory is never freed" was removed; it
was fixed on 2026-10-22.

`liquers-py/README.md` gained:
- free-threaded support;
- the `ASYNCQ` and `ASYNCCMD` rows;
- the `RUNTIME05` caveat.

### Links and Capability Map
`specs/README.md` has a new capability line for Python async and free-threading. It links the
reference and the guide.

## Issues Filed

- `CORE-ONE-JOB-QUEUE-FOR-ALL-COMMANDS`: commands cannot be placed on queues by their cost. This is
  where Orest's future configurable queues belong.
- `PY-TESTS-NOT-RUN-IN-CI`: the Python suite runs only locally.
- `CORE-JOB-QUEUE-KEEPS-ENVIRONMENT-PINS-AFTER-SHUTDOWN`: found while testing, and it predates this
  work. A job still running at shutdown keeps its environment alive, which makes `RUNTIME05` fail
  now and then.

Closed: `PY-ASYNC-EVALUATION-NOT-SUPPORTED`, `PY-FREE-THREADED-SUPPORT-UNVERIFIED`.

## Important Learning

- `into_future_with_locals` does not cancel the Python task on drop. Cancelling a command therefore
  needs `run_coroutine_threadsafe` and an explicit guard.
- A coroutine that cancels itself surfaces as `concurrent.futures.CancelledError`. That class is
  different from `asyncio.CancelledError`.
- A plain `Mutex::lock` taken while attached is a deadlock hazard on every build, not only on a
  free-threaded one. The rules are to lock with `lock_py_attached` and to drop Python objects after
  the guard (`LANGUAGE-INTEGRATION_GUIDE.md` §6 Python).
- A 3.14t venv needs its own tag (`sys.abiflags`). Its Rust tests need `LIBDIR` on
  `LD_LIBRARY_PATH`.

## Conformance and Remaining Work

- Requested: `ASYNCQ`, `ASYNCCMD`, and the free-threaded audit with tests.
- Approved: AC-1 to AC-8.
- Implemented: all eight, each proved by the Phase 3 tests.

Nothing remains in this design. The configurable queues, CI and the shutdown pin leak are tracked
by the issues above.

## Validation

- `PYTHON=python3.13 bash scripts/test-liquers-py.sh`: 209 passed and 6 skipped (3 NA, 3 that run
  only on a free-threaded build). 22 Rust tests passed.
- `PYTHON=$(uv python find 3.14t) bash scripts/test-liquers-py.sh`: 212 passed and 3 skipped (NA).
  22 Rust tests passed.
- `RUNTIME05` fails about 1 run in 15 under load, on either interpreter. The cause is
  `CORE-JOB-QUEUE-KEEPS-ENVIRONMENT-PINS-AFTER-SHUTDOWN`.
- `cargo clippy -p liquers-py --no-default-features --lib`: no new findings.
- `docs_index.py --check` passes.

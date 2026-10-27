# Phase 4: Implementation Plan — liquers-py-async

## Overview

The order is:
1. The audit fix and the script fixes, which are independent and are needed to test on 3.14t.
2. The bridge foundation.
3. Awaitable evaluation, proved with sync commands.
4. Async commands.
5. The explicit declaration.

There are no prerequisites. All work lands on this branch in PR #181, with Phase 5 before merge,
and the existing suite stays green after every step.

## Progress

- [x] Step 1: Fix the environment lock and record the PR (17e0fabd)
- [x] Step 2: Fix the test script for free-threaded interpreters (1b1a876a)
- [x] Step 3: Add pyo3-async-runtimes and bind it to the runtime (1f5bdf2c)
- [ ] Step 4: Add the loop pool, pool-thread flag and blocking-call check
- [ ] Step 5: Guard the blocking calls
- [ ] Step 6: Add the async_loops and job_capacity keywords and the shutdown signal
- [ ] Step 7: Add the awaitable methods
- [ ] Step 8: Run async def commands on the pool
- [ ] Step 9: Declare free-threaded support and run both interpreters

## Implementation Steps

### Step 1: Fix the environment lock and record the PR
- Files / symbols:
  - `liquers-py/src/environment.rs`: every `EnvCell` method (`envref`, `invalidate`, `validate`,
    `register`, `unregister`, `import_metadata`, `shutdown`), their callers, and its Rust tests.
  - New `tests/test_FREETHREAD_concurrency.py` (test 03).
  - `specs/design/liquers-py-async/DESIGN.md` gets `gh_pr: [181]` and loses its `status:`.
  - The two source issues move to `in_progress`.
- Change:
  - `EnvCell` methods take `py` and lock with `MutexExt::lock_py_attached`.
  - `register` and `unregister` move removed specs out of the guard before dropping them.
  - The Rust tests run inside `Python::initialize()` + `Python::attach`.
- Depends on: none.
- Proof: `environment::tests::lock_is_taken_detached` and
  `test_FREETHREAD03_finalizer_reentry_does_not_deadlock`, both of which deadlock (time out) at HEAD.
- Rollback: revert.

### Step 2: Fix the test script for free-threaded interpreters
- Files / symbols: `scripts/test-liquers-py.sh`.
- Change:
  - The venv tag adds `sys.abiflags`.
  - The Rust half prepends `sysconfig` `LIBDIR` to `LD_LIBRARY_PATH`.
  - Delete the mis-tagged `target/liquers-py-venv-cpython3.14` venv.
- Depends on: none (runs in parallel with Step 1).
- Proof: `PYTHON=$(uv python find 3.14t) bash scripts/test-liquers-py.sh` passes both halves.
- Rollback: revert.

### Step 3: Add pyo3-async-runtimes and bind it to the runtime
- Files / symbols: `liquers-py/Cargo.toml`; `src/runtime.rs` `runtime()`; `src/environment.rs`
  tests.
- Change:
  - Add the dependency with the `tokio-runtime` feature.
  - Call `init_with_runtime` once. `Err(())` is accepted only when `get_runtime()` is the same
    runtime; otherwise it is `Error::unexpected_error`.
  - Add the AC-5 regression guard.
- Depends on: none.
- Proof: `runtime::tests::bridge_uses_the_liquers_runtime`, and
  `environment::tests::pure_rust_query_completes_while_gil_is_held` as a guard.
- Rollback: revert.

### Step 4: Add the loop pool, pool-thread flag and blocking-call check
- Files / symbols: new `src/aio.rs`, with `LoopPool`, `LoopThread`, `LazyLoopPool` (and its
  `get_with` seam), `Pending`, `ON_LOOP` and `check_blocking_call`; `src/lib.rs` (`mod aio`).
- Change:
  - Build each item as specified in Phase 2, including the stop sequence with `STOP_GRACE` and its
    done channels.
  - `Drop for LoopPool` stops the pool on a detached thread; the `atexit` hook holds a `Weak`.
  - `CancelOnDrop::drop` matches on `runtime()` rather than propagating with `?`.
- Depends on: 3.
- Proof: `aio::tests::check_blocking_call_raises_only_on_pool_threads`,
  `pool_picks_round_robin_and_stop_is_idempotent`, `failed_pool_start_is_retried`,
  `pending_cancels_a_future_stored_after_drop`.
- Rollback: revert; nothing uses the module yet.

### Step 5: Guard the blocking calls
- Files / symbols:
  - `src/environment.rs`: `evaluate`, `evaluate_state`, `get_asset`, `store_set`, `shutdown`.
  - `src/context.rs`: `evaluate`.
  - `src/asset.rs`: `get`, `get_state`, `cancel`.
  - New `tests/test_ASYNCQ_evaluation.py` (ASYNCQ08).
- Change: each method calls `check_blocking_call` first. `shutdown` uses its own message.
- Depends on: 4.
- Proof: `test_ASYNCQ08_blocking_call_on_a_foreign_loop_warns_and_works`, which has no warning at
  HEAD.
- Rollback: revert.

### Step 6: Add the async_loops and job_capacity keywords and the shutdown signal
- Files / symbols:
  - `src/environment.rs`: `Environment::new`, `EnvCell` (now holding `Arc<LazyLoopPool>` and
    `Arc<Shut>`), `shutdown`, `block_on`.
  - `src/asset.rs`, `src/context.rs`, `src/command.rs`: `Arc<AtomicBool>` becomes `Arc<Shut>`.
  - New `tests/test_ASYNC_loops.py`.
- Change:
  - The keywords are extracted as `i64` and validated, with conflicts raising
    `Error::configuration_error`.
  - `shutdown` sets the flag, fires `notify`, stops the asset managers, then the pool.
- Depends on: 4.
- Proof: `test_ASYNC_invalid_pool_and_capacity_rejected` and
  `test_ASYNC_job_capacity_keyword_limits_running_commands` (`TypeError` at HEAD).
- Rollback: revert.

### Step 7: Add the awaitable methods
- Files / symbols:
  - `src/aio.rs`: `ToPy`.
  - The `_async` methods in `src/environment.rs`, `asset.rs` and `context.rs`.
  - `python/liquers/_env.py` and `__init__.py`.
  - `conftest.py`: add `sleepy`.
  - `tests/test_ASYNCQ_evaluation.py`.
- Change:
  - Each method runs `runtime()?`, parses eagerly, spawns the core future raced against
    `Shut::notify`, and wraps the `JoinHandle` with `future_into_py_with_locals`.
  - A `JoinError` is `Error::unexpected_error`.
- Depends on: 3, 5, 6.
- Proof:
  - `test_ASYNCQ01` to `test_ASYNCQ07` (07 is the `na` skip);
  - `test_ASYNCQ_every_awaitable_method_round_trips`;
  - `test_ASYNCQ_async_call_without_running_loop_raises`;
  - `test_ASYNCQ_module_level_evaluate_async`;
  - `test_ASYNC_shutdown_ends_async_waits`.
- Rollback: revert.

### Step 8: Run async def commands on the pool
- Files / symbols:
  - `src/command.rs`: `PyCallable`, `PyCommandSpec::new` (takes `is_async`), `call_py_command`,
    `prepare_call`, `call_async_py_command`, `CancelOnDrop`, `#[pyclass] Completer`.
  - `src/environment.rs`: `_register_command` computes `is_async` and passes the pool.
  - `src/error.rs`: both `CancelledError` classes map to `Error::cancelled`.
  - `conftest.py`: add `nap`, `hang`, `where_async`.
  - New `tests/test_ASYNCCMD_commands.py`.
- Change:
  - The three steps of Phase 2, with `future.cancelled()` checked before `result()`.
  - `CommandMetadata::with_async`.
  - A coroutine returned by a sync callable is `Error::execution_error`.
- Depends on: 4, 6, 7.
- Proof:
  - `test_ASYNCCMD01` to `test_ASYNCCMD06`, plus `test_ASYNCCMD03_coroutine_cancelling_itself_is_cancelled`,
    `test_ASYNCCMD_sync_function_returning_coroutine_fails` and
    `test_ASYNCCMD_async_callable_object_detected`.
  - In `test_ASYNC_loops.py`: `test_ASYNC_blocking_call_on_pool_loop_raises`,
    `test_ASYNC_pool_spreads_commands_over_loops`, `test_ASYNC_default_pool_has_one_loop`,
    `test_ASYNC_shutdown_cancels_running_coroutine`,
    `test_ASYNC_shutdown_does_not_hang_on_a_stubborn_coroutine` and
    `test_ASYNC_interpreter_exit_with_live_pool`.
- Rollback: revert; `async def` returns to being unsupported.

### Step 9: Declare free-threaded support and run both interpreters
- Files / symbols: `src/lib.rs` (`#[pymodule(gil_used = false)]`); `liquers-py/pyproject.toml`
  (classifier); `tests/test_FREETHREAD_concurrency.py` (tests 01, 02 and 04).
- Change: the explicit declaration, the classifier, and the parallelism tests.
- Depends on: 1, 2, 8.
- Proof: `bash scripts/test-liquers-py.sh` and the 3.14t run both pass, with the FREETHREAD tests
  running on 3.14t and not skipped.
- Rollback: set `gil_used = true`.

## Testing Plan

Run `bash scripts/test-liquers-py.sh` after every step, and the 3.14t variant from Step 2 on. At the
end, also run `cargo clippy -p liquers-py --no-default-features --lib`.

`scripts/check-build-matrix.sh` does not cover `liquers-py`, and no `cfg(feature)` changes. CI does
not run the Python suite (`PY-TESTS-NOT-RUN-IN-CI`), so the PR reports both local runs.

## Rollback Plan

Each step is one commit. If Steps 1 to 6 land alone, they still ship safely: the deadlock fix, the
script fixes, the guards and the keywords, but no async API yet. The README keeps ASYNCQ and
ASYNCCMD as not implemented until Step 8.

## Documentation Updates

Phase 5 writes the documents of Phase 2's architecture:

- `PYTHON_INTEGRATION.md`, covering the async API, cancellation, the pool and audit findings 3-4.
- `PYTHON_GUIDE.md`.
- `LANGUAGE-INTEGRATION_GUIDE.md` §6. Besides the † rows, it notes that the tests use `asyncio.run`
  and `env.command_metadata`.

Each of these three gets a History row and a `reviewed:` bump. Phase 5 also updates the
`liquers-py/README.md` matrix and `specs/README.md`, and regenerates `specs/index.*`.
`command_registry.yaml` is unchanged. Phase 5 closes both source issues.

## Phase 5 Entry Criteria
- [ ] Implementation finished and validated on CPython 3.13 and 3.14t
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour

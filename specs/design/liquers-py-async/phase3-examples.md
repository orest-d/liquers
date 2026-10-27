# Phase 3: Examples & Use-cases — liquers-py-async

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Await an evaluation, register an `async def` command | The two new capabilities | AC-1, AC-2 |
| 2 | Example | Rust queries beside slow Python | AC-5's goal; the pool | AC-5, AC-6 |
| 3 | Tests | `tests/test_ASYNCQ_evaluation.py` | Guide ASYNCQ01-08, every awaitable | AC-1, AC-3 |
| 4 | Tests | `tests/test_ASYNCCMD_commands.py` | Guide ASYNCCMD01-06 | AC-2, AC-4, AC-7 |
| 5 | Tests | `tests/test_ASYNC_loops.py` | Guards, keywords, pool, shutdown, exit | AC-3, AC-5, AC-6, AC-7 |
| 6 | Tests | `tests/test_FREETHREAD_concurrency.py` | Free-threaded audit | AC-8 |
| 7 | Tests | Rust unit tests in `aio.rs`, `environment.rs`, `runtime.rs` | Rust-side invariants | AC-3, AC-5, AC-6, AC-7, AC-8 |

Every query was checked with `liquers-validate`, with `--command` for the test-local commands.

## Example 1: await and `async def`

```python
import asyncio, liquers as lq

env = lq.Environment()

@env.first_command
def hello() -> str:
    return "hello"

@env.command
async def fetch_len(text: str) -> int:
    await asyncio.sleep(0.01)                  # stands in for an HTTP call
    return len(text)

async def main():
    assert await env.evaluate_async("hello/fetch_len") == 5
    assert env.command_metadata("fetch_len").is_async

asyncio.run(main())
env.shutdown()
```

The coroutine runs on the environment's loop, never on the caller's.

## Example 2: Rust queries beside slow Python

The difference from Example 1 is four loops, plus a queue big enough for both kinds of work:

```python
env = lq.Environment(async_loops=4, job_capacity=8)
env.store_set("d/x.txt", "abc")

async def main():
    naps = [env.evaluate_async(f"nap-{i}") for i in range(4)]            # async def nap(i)
    assert await env.evaluate_async("-R/d/x.txt/-/to_text") == "abc"    # Rust only, not delayed
    await asyncio.gather(*naps)
```

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| Sync `context.evaluate`, `env.evaluate`, `asset.get()` or `env.shutdown()` inside an `async def` command | `RuntimeError`, and the asset fails | `test_ASYNC_blocking_call_on_pool_loop_raises` |
| Sync `env.evaluate` inside `asyncio.run` | `RuntimeWarning`, then the value | `test_ASYNCQ08_blocking_call_on_a_foreign_loop_warns_and_works` |
| An `_async` call with no running loop | `RuntimeError` | `test_ASYNCQ_async_call_without_running_loop_raises` |
| The awaitable is cancelled mid-run | `asyncio.CancelledError`; the asset still reaches Ready | `test_ASYNCQ04_cancelling_the_awaitable_detaches` |
| The asset is cancelled while its coroutine sleeps | The coroutine sees `CancelledError`, the status is `Cancelled`, and the awaiter gets `lq.CancelledError` | `test_ASYNCCMD03_cancellation_in_both_directions` |
| The coroutine raises `asyncio.CancelledError` itself | `Cancelled`, not `Error` | `test_ASYNCCMD03_coroutine_cancelling_itself_is_cancelled` |
| `shutdown()` while an `_async` await is queued or running | `lq.CancelledError` within 3 s | `test_ASYNC_shutdown_ends_async_waits` |
| A coroutine ignores cancellation at shutdown | `shutdown()` returns within `STOP_GRACE` + 2 s, with a warning | `test_ASYNC_shutdown_does_not_hang_on_a_stubborn_coroutine` |
| The process exits without `shutdown()` | Exit code 0 within 15 s | `test_ASYNC_interpreter_exit_with_live_pool` |
| Bad keywords | `lq.ConfigurationError` for `async_loops=0`, `-1`, `job_capacity=0`, or a `job_capacity` conflicting with `config` | `test_ASYNC_invalid_pool_and_capacity_rejected` |
| A sync function returns a coroutine | `ExecutionError` | `test_ASYNCCMD_sync_function_returning_coroutine_fails` |
| A `__del__` that re-enters the environment during re-registration | No deadlock | `test_FREETHREAD03_finalizer_reentry_does_not_deadlock` |

## Test Plan

The Python tests are plain functions that call `asyncio.run`, so no new dependency is needed. A
test that could hang runs its body in a thread joined with a timeout, because the script installs
no pytest timeout plugin. Timing tests warm the environment with `env.evaluate("hello")`, use
distinct or `volatile` queries, and nap at least 0.5 s.

New fixtures go in `conftest.py`:
- `async def nap(i: int = 0)`, which sleeps for 0.5 s;
- `async def hang()`, which sets the `CALLS` events `started` and `saw_cancel`;
- `async def where_async(i: int = 0)`, which returns the thread id before and after an await;
- a sync `sleepy(i: int = 0)` (0.5 s, not volatile).

Cancellation tests wait for `started` before they cancel, and wait for `saw_cancel` with a timeout.
`repeat`, `fetch_len`, `outer_async` and `idem` are test-local.

`tests/test_ASYNCQ_evaluation.py`, ASYNCQ01-07 without `async def` commands:
- `test_ASYNCQ01_await_successful_evaluation` (AC-1): `"hello/repeat-3"`.
- `test_ASYNCQ02_failure_raises_structured_error` (AC-1): `lq.ParseError`, raised at call time.
- `test_ASYNCQ03_two_evaluations_make_progress` (AC-1): two `sleepy` calls take less than 1.5× one.
- `test_ASYNCQ04_cancelling_the_awaitable_detaches` (AC-1): cancels `sleepy-9`, after which `get_asset("sleepy-9").get()` returns without a second run.
- `test_ASYNCQ05_dropping_the_handle_detaches` (AC-1): `del` the future and `gc.collect()`, inside the same `asyncio.run`; the command's completion event fires.
- `test_ASYNCQ06_no_event_loop_blocking` (AC-1).
- `test_ASYNCQ07_documented_non_async_workaround` (AC-1): skipped with `na(...)`, because Python has an async model.
- `test_ASYNCQ08_blocking_call_on_a_foreign_loop_warns_and_works` (AC-3).
- `test_ASYNCQ_every_awaitable_method_round_trips` (AC-1): `evaluate_state_async`, `get_asset_async`, `store_set_async`, `AssetRef.get_async` / `get_state_async`, plus a second `await` of one finished future.
- `test_ASYNCQ_async_call_without_running_loop_raises` (AC-1).
- `test_ASYNCQ_module_level_evaluate_async` (AC-1).

`tests/test_ASYNCCMD_commands.py`:
- `test_ASYNCCMD01_async_command_result` (AC-2).
- `test_ASYNCCMD02_async_exception` (AC-2): `ExecutionError`, and the traceback names the coroutine.
- `test_ASYNCCMD03_cancellation_in_both_directions` (AC-7): `cancel_async` on `hang`. Replacement drops the command future on the same `select!` path, so it has no separate test.
- `test_ASYNCCMD03_coroutine_cancelling_itself_is_cancelled` (AC-7).
- `test_ASYNCCMD04_nested_async_evaluation` (AC-4): `outer_async` awaits `context.evaluate_async("hello/fetch_len")`, with `job_capacity=1`.
- `test_ASYNCCMD05_concurrent_calls_do_not_corrupt_state` (AC-2): 50 `number-{i}/idem` calls.
- `test_ASYNCCMD06_sync_and_async_metadata_differ` (AC-2): through `env.command_metadata(...)`.
- `test_ASYNCCMD_sync_function_returning_coroutine_fails` (AC-2).
- `test_ASYNCCMD_async_callable_object_detected` (AC-2): `async def __call__`.

`tests/test_ASYNC_loops.py`:
- `test_ASYNC_blocking_call_on_pool_loop_raises` (AC-3): the four calls in the table.
- `test_ASYNC_invalid_pool_and_capacity_rejected` (AC-6).
- `test_ASYNC_job_capacity_keyword_limits_running_commands` (AC-5): `job_capacity=1` passed as the keyword; two volatile sync commands started from threads run one after the other.
- `test_ASYNC_pool_spreads_commands_over_loops` (AC-6): with `async_loops=4`, four `where_async` calls meet at a `threading.Barrier(4)` through `asyncio.to_thread`, on four distinct threads, each thread the same before and after.
- `test_ASYNC_default_pool_has_one_loop` (AC-6): two concurrent `where_async` calls report one thread.
- `test_ASYNC_shutdown_ends_async_waits` (AC-7): `job_capacity=1`; `slow-5` runs while `slow-6` is queued.
- `test_ASYNC_shutdown_cancels_running_coroutine` (AC-7): `hang`.
- `test_ASYNC_shutdown_does_not_hang_on_a_stubborn_coroutine` (AC-7).
- `test_ASYNC_interpreter_exit_with_live_pool` (AC-6, AC-7): runs in a subprocess.

`tests/test_FREETHREAD_concurrency.py` (AC-8). Tests 01, 02 and 04 are skipped unless
`Py_GIL_DISABLED` is set and `sys._is_gil_enabled()` is false; 02 and 04 are also skipped when
`len(os.sched_getaffinity(0)) < 4`.
- `test_FREETHREAD01_gil_stays_disabled_after_import`.
- `test_FREETHREAD02_sync_commands_run_in_parallel`: four distinct CPU-bound queries finish in under 2.5× one.
- `test_FREETHREAD03_finalizer_reentry_does_not_deadlock`: on every build. A callable whose `__del__` calls `env.validate()` is registered under one key, and the key is re-registered before any build, in a thread joined with a 10 s timeout.
- `test_FREETHREAD04_async_commands_on_several_loops_in_parallel`: `async_loops=4`, with the same bound as 02.

Rust unit tests (`cargo test -p liquers-py --no-default-features --lib`), each under
`Python::initialize()` + `Python::attach`, as `error.rs` and `value.rs` tests do:
- `runtime::tests::bridge_uses_the_liquers_runtime` (AC-5).
- `aio::tests::check_blocking_call_raises_only_on_pool_threads` (AC-3).
- `aio::tests::pool_picks_round_robin_and_stop_is_idempotent` (AC-6).
- `aio::tests::failed_pool_start_is_retried` (AC-6): through the `get_with` seam.
- `aio::tests::pending_cancels_a_future_stored_after_drop` (AC-7): a plain `concurrent.futures.Future` is stored after the guard drops, and comes out cancelled.
- `environment::tests::lock_is_taken_detached` (AC-8): the FREETHREAD03 scenario in Rust, run in a spawned thread with `recv_timeout`.
- `environment::tests::pure_rust_query_completes_while_gil_is_held` (AC-5): a regression guard, skipped when `Py_GIL_DISABLED`; it passes today and is not a proof of any step.

To run: `bash scripts/test-liquers-py.sh`, then the same with `PYTHON=$(uv python find 3.14t)`.

## Learning Log

The facts the design rests on are recorded in Phase 2. One surprise belongs to Phase 5: a coroutine
that cancels itself surfaces as `concurrent.futures.CancelledError`, not `asyncio.CancelledError`,
when it is run through `run_coroutine_threadsafe`.

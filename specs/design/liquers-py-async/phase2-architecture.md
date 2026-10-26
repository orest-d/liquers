# Phase 2: Solution & Architecture — liquers-py-async

## Overview

Each `_async` method spawns its core future on liquers-py's runtime and wraps the `JoinHandle` with
`pyo3-async-runtimes` (`tokio::future_into_py_with_locals`), so a dropped awaitable detaches
(Decision 4). An `async def` command's coroutine goes to a `LoopPool` loop by
`asyncio.run_coroutine_threadsafe` and is cancelled when the command's future drops. Pool threads
are flagged so blocking calls there raise. The audit fixes one deadlock. Rejected (beyond
Decisions 1-2):
- `into_future_with_locals` for commands: dropping it does not cancel the task (`lib.rs`
  `PyEnsureFuture` only forwards the result), so AC-7 would fail.
- `asyncio.run` per call: a loop per call breaks loop-bound resources and pays start-up each time.

## Known-Issue Preflight

| Issue | Status | Priority | Impact | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `PY-ASYNC-EVALUATION-NOT-SUPPORTED` | accepted | P3 | Source feature | — | No | Closed by this design |
| `PY-FREE-THREADED-SUPPORT-UNVERIFIED` | accepted | P2 | AC-8 | — | No | Absorbed |
| `CORE-ONE-JOB-QUEUE-FOR-ALL-COMMANDS` | draft | P2 | Python commands share the queue | No | No | `job_capacity` (Decision 5) |
| `LANGUAGE-INTEGRATIONS-REBUILD-ENVIRONMENT-ON-REGISTRATION` | draft | P3 | Rebuilds retire environments | No | No | Pool owned by `EnvCell` |
| `PY-TESTS-NOT-RUN-IN-CI` (new) | draft | P2 | AC-8 proved by local runs only | No | No | Filed; not auto-fixed: the CI matrix and its cost are the owner's call |

Also touching, not blocking, no action (P3): `CORE-TOKIO-REMOVAL` (the bridge needs tokio),
`MULTI-LANGUAGE-TRACEBACK-IS-A-FLAT-STRING`, `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT`. Probe
on 3.14.6t: the suite passes, the GIL stays off with `liquers`, `polars`, `PIL`; two
`scripts/test-liquers-py.sh` defects, fixed in Phase 4. No command namespaces.

## Interfaces

New `liquers-py/src/aio.rs`, all sync (an asyncio loop owns its `std::thread`):

```rust
pub(crate) struct LoopPool { loops: Vec<LoopThread>, next: AtomicUsize }
pub(crate) struct LoopThread { locals: TaskLocals, thread: Mutex<Option<JoinHandle<()>>> }
impl LoopPool {
    /// Threads set `ON_LOOP`, `set_event_loop`, `run_forever`; waits detached; `atexit` holds a `Weak`.
    pub(crate) fn start(py: Python<'_>, size: NonZeroUsize) -> Result<Self, Error>;
    pub(crate) fn pick(&self) -> &LoopThread;              // round-robin
    /// Cancel tasks, gather them, `shutdown_asyncgens`, `close`; join ≤ `STOP_GRACE` (5 s), else warn.
    pub(crate) fn stop(&self, py: Python<'_>) -> Result<(), Error>;
}
impl Drop for LoopPool { /* hands `stop` to a detached std::thread, never a worker */ }
/// On `EnvCell` (rebuilds share it); started by the first async command; a failure is not cached.
pub(crate) struct LazyLoopPool { size: NonZeroUsize, pool: Mutex<Option<Arc<LoopPool>>> }

thread_local! { static ON_LOOP: Cell<bool> = const { Cell::new(false) }; }
/// AC-3: `RuntimeError` on a pool thread; else `RuntimeWarning` if a loop runs here.
pub(crate) fn check_blocking_call(py: Python<'_>, call: &str) -> PyResult<()>;
/// Made before step 1 (a cancel during conversion is kept). `Drop` invalidates the `Context` and
/// cancels an unfinished future via `runtime()?.spawn_blocking` + `Python::attach` (AC-7).
struct CancelOnDrop(Arc<Mutex<Pending>>);
struct Pending { cancelled: bool, future: Option<Py<PyAny>>, context: Context }
/// Converted (`Err` via `error_to_pyerr`) by `IntoPyObject` in the bridge's `spawn_blocking` (AC-5).
pub(crate) enum ToPy { Value(Result<Value, Error>), State(Result<CoreState<Value>, Error>),
    Asset(Result<CoreAssetRef<PyEnvironment>, Error>), Unit(Result<(), Error>) }
```

`src/command.rs`: `PyCallable` gains `is_async` (`inspect.iscoroutinefunction` of the object or of
`type(obj).__call__`) and `loops: Arc<LazyLoopPool>`; `PyCommandSpec` calls
`CommandMetadata::with_async` (the Python `is_async` getter exists). For `is_async`,
`call_py_command` delegates to `call_async_py_command` (same parameters):
1. Create `CancelOnDrop`. On the runtime's `spawn_blocking`: `prepare_call` (split from
   `call_with_gil`), call, `run_coroutine_threadsafe` on `pick()`, attach a `#[pyclass] Completer`
   done-callback; store the future, or cancel it at once if `cancelled` is set.
2. Await the Completer's oneshot. A dropped sender is `Error::cancelled`.
3. On `spawn_blocking`: `result()`, converted as the sync path. `asyncio.CancelledError` maps to
   `Error::cancelled`, so the asset ends `Cancelled`. Then disarm and invalidate the `Context`.

A sync callable that returns a coroutine is `Error::execution_error` ("declare it `async def`").

Python (`liquers._native`). Every `_async` method calls `runtime()?` first, so the bridge never
builds its own runtime, and returns an `asyncio.Future`:

| Object | New |
|---|---|
| `Environment(...)` | `async_loops: int = 1`, `job_capacity: int \| None = None`; both checked in `__new__` |
| `Environment` | `evaluate_async`, `evaluate_state_async`, `get_asset_async`, `store_set_async` |
| `AssetRef` | `get_async`, `get_state_async`, `cancel_async` (Decision 4's explicit cancel) |
| `Context` | `evaluate_async`: `context.evaluate` + `wait_for_dependency`, as the sync `Context.evaluate` |
| `liquers` | `evaluate_async` (default environment, `_env.py`) |

`job_capacity` sets `config.assets.job_capacity`; a conflicting `config` value is a
`ConfigurationError`. `check_blocking_call` guards `evaluate`, `evaluate_state`,
`get_asset`, `store_set`, `shutdown`, `Context.evaluate` and `AssetRef.get` / `get_state` / `cancel`.
Exempt with no awaitable: `AssetRef.status` / `metadata` (one lock read) and `shutdown` (sync by
nature; on a pool thread it would join itself, so it raises there).

AC-4 holds because a nested evaluation is a core dependency wait. With no free slot, `JobQueue`
runs the dependency inline in the waiting asset (`assets.rs`), so `job_capacity=1` cannot deadlock.
The inner coroutine may share the outer one's loop, because the outer awaits. AC-5 reading:
`attach_off_worker` (`value.rs`) takes the GIL for `py.Object` (de)serialization only inside
`block_in_place`, which first hands the worker's queue to another thread. So no queued task waits on
the GIL; everything else attaches on blocking or pool threads.

## Integration Points

- `liquers-py/Cargo.toml`: `pyo3-async-runtimes = { version = "0.30", features = ["tokio-runtime"] }`.
- `src/aio.rs` (new); `src/command.rs`; `src/environment.rs` (keywords, methods, guards, the pool on
  `EnvCell`, and `shutdown`: asset managers first, then `LoopPool::stop`, which cancels running
  coroutines, since `JobQueue::shutdown` does not cancel running assets); `src/asset.rs`;
  `src/context.rs`; `src/runtime.rs` (`init_with_runtime`; `Err(())` accepted only if
  `get_runtime()` is ours); `src/lib.rs` (explicit `#[pymodule(gil_used = false)]`).
- `python/liquers/_env.py`, `__init__.py`; `pyproject.toml` classifier `Free Threading :: 2 - Beta`.
- `scripts/test-liquers-py.sh`: the venv tag adds `sys.abiflags`; the Rust half sets `LD_LIBRARY_PATH`
  from `sysconfig` `LIBDIR`.
- `liquers-core`: unchanged. Cancel and replacement drop the command future (`assets.rs`
  `tokio::select!` on `cancellation.requested()`); `CancelOnDrop` relies on that.

## Error Handling

Rows marked † are where Python deviates from the guide's sample tests; Phase 3 tests and
LANGUAGE-INTEGRATION_GUIDE §6 record them.

| Case | Result |
|---|---|
| Blocking call on a pool thread † | `RuntimeError` naming the `_async` variant (AC-3, `ASYNCQ08` "rejected") |
| Blocking call under another running loop | `RuntimeWarning`, then it runs (`ASYNCQ08` "works") |
| `_async` call with no running loop | `RuntimeError` from `get_current_locals` |
| Awaitable cancelled or dropped † | Awaiter gets `asyncio.CancelledError`; the evaluation goes on. `ASYNCQ04` / `ASYNCCMD03` use `cancel_async`; `ASYNCQ05` policy: detach |
| Asset cancelled, replaced, or environment shut down | Awaiters raise `lq.CancelledError` (an `lq.Error`, unlike `asyncio.CancelledError`); the coroutine gets `CancelledError` (AC-7) |
| Coroutine raises `CancelledError`, or the oneshot sender drops | `Error::cancelled` |
| Coroutine raises anything else | As sync commands, traceback kept |
| Coroutine ignores cancellation past `STOP_GRACE` | `shutdown()` warns and detaches the thread |
| Sync function returns a coroutine | `Error::execution_error` |
| `async_loops=0` or `job_capacity=0` | `Error::configuration_error`, raised in `Environment(...)` |
| A loop fails to start | `Error::general_error`; that command fails, and the next one retries |

`ASYNCQ07` is NA, because Python has an async model. A result is an `asyncio.Future`, so it can be
awaited more than once. Progress streams are out of scope. Timeouts are the caller's
`asyncio.timeout`.

## Relevant Commands

None new.

## Concurrency

The free-threaded audit (AC-8) covers every `static`, lock, atomic and `#[pyclass]` in
`liquers-py/src` and the `liquers` package:

| # | Finding | Action |
|---|---|---|
| 1 | No action needed for these. Process state is `OnceLock` (`RUNTIME`, `builtin_keys`) and atomics (`shut`, `LIVE_HANDLES`). liquers-lib's `DECODERS` `RwLock` copies out a `fn` pointer. `Context.inner` is locked only to clone or invalidate. `_env.py` uses `threading.Lock`, and error classes are built at import. There is no `unsafe`. | None |
| 2 | `EnvCell::lock` (`std::sync::Mutex`) is taken while attached. `register` (`specs.retain`), `unregister` and `shutdown` (`specs.clear`) can drop the last `PyCallable` under it, and that decref can run `__del__` or GC. On 3.14t, the stop-the-world waits on a thread that is attached and blocked on this lock. On a GIL build, a switch to that thread deadlocks the same way. `envref()` also holds the lock across `build()`, which stalls attached waiters (not a deadlock). | Use `MutexExt::lock_py_attached(py)`, with a py-less variant for Rust tests; drop removed specs after the guard |
| 3 | Non-frozen classes (`Query`, `Key`, `Metadata`, and others) have an atomic PyO3 borrow flag, so concurrent mutation of one object raises `RuntimeError`. Handles (`Environment`, `Context`, `AssetRef`, `State`) are `frozen`. | Document |
| 4 | One Python command may run on many threads and loops at once; its state is the user's concern. | Document |

The audit fix and the explicit `gil_used = false` land in one PR, so AC-8's interim
`gil_used = true` never ships. It stays the fallback if a 3.14t defect is found later.

## Documentation Architecture

| Path | Kind / audience | Change |
|---|---|---|
| `specs/reference/PYTHON_INTEGRATION.md` | reference / integrators | Async API, pool, cancellation table, AC-3 rule, `job_capacity`, free-threading guarantees |
| `specs/guides/PYTHON_GUIDE.md` | guide / Python users | "Async" and "Free-threaded Python" sections |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | guide / integrators | §6 Python: async notes and the † deviations |
| `liquers-py/README.md` | crate (outside `affects_docs`) | ASYNCQ and ASYNCCMD rows; 3.14t tested |
| `specs/README.md` | map | Design status |

## Risks

| Assessment | Finding |
|---|---|
| Change size | ~10 files in `liquers-py`, one script |
| Tests | Existing tests unchanged; new `test_ASYNCQ_*`, `test_ASYNCCMD_*` and `test_FREETHREAD_*` run on 3.13 and 3.14t with `asyncio.run` (no `pytest-asyncio`) |
| Compatibility | Additive API; `gil_used` is unchanged in effect |
| Concurrency | Interpreter exit with live threads is handled by the `atexit` stop. Dropping an `Environment` without `shutdown()` stops its pool asynchronously |
| Certainty | High for the bridge and PyO3 helpers (source read); medium for exit behaviour (Phase 3 tests) |

---
id: ENVIRONMENT-ORDERLY-SHUTDOWN-DESIGN
kind: design
title: An environment can be shut down in an orderly way
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/context, core/assets]
issues: [ENVIRONMENT-ORDERLY-SHUTDOWN]
created: 2026-10-20
---
# An environment can be shut down in an orderly way

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20: Phases 1-4 written, not
approved, nothing implemented.

## Phase 1: High-Level Design

### Purpose

Give an application one `async` call that stops an environment: it settles the work in flight
(waits for it, or cancels it), refuses later work with a typed error instead of hanging, runs an
asynchronous cleanup hook once, and stops the manager's background tasks (soft reboot,
per-tenant environment, graceful server stop). Dropping the last `EnvRef` stays the teardown for
applications that do not call it (`environment-manager-reference-cycle`).

### Problem Example

From the issue, checked at HEAD:

```rust
let envref = builder.build().await?;                       // queued manager
let asset = envref.get_asset_manager().get_asset(&q).await?;   // running
envref.get_asset_manager().shutdown();                     // the only stop today (inherent, sync)
let again = envref.get_asset_manager().get_asset(&q2).await?;  // returns a Submitted asset ...
again.get().await;                                         // ... which never runs: hangs
```

`DefaultAssetManager::shutdown` (`liquers-core/src/assets.rs`) stops the job queue and the
monitor, but neither settles `asset` nor refuses later work (`JobQueue::shutdown`: a later submit
"leaves an asset parked in `Submitted`"). `ImmediateAssetManager` has no stop, and nothing offers
an asynchronous cleanup point.

Should be: `envref.shutdown(ShutdownMode::Drain).await?` returns once `asset` has finished,
`asset.get()` still yields its value, and the later `get_asset(&q2)` is `Err(NotAvailable)`.

### Scope and Acceptance Criteria

- **AC-1** Drain waits for work in flight
  WHEN `EnvRef::shutdown(ShutdownMode::Drain)` is called while an asset is running and another is
  queued, on either built-in manager
  THEN it returns after both finish, and the held `AssetRef`s return their values
- **AC-2** Cancel interrupts work in flight
  WHEN `EnvRef::shutdown(ShutdownMode::Cancel)` is called while an asset is running
  THEN the asset ends `Cancelled` (the outcome `asset-cancellation-outcome` defines), and shutdown
  returns after it has settled
- **AC-3** Later work fails with a typed error
  WHEN `get_asset`, `get`, `apply`, `set_state`, `set_binary`, `remove` or `expire` is called
  after shutdown
  THEN it returns `Err` with `ErrorType::NotAvailable` and a message naming shutdown, without
  hanging or panicking
- **AC-4** Idempotent
  WHEN `shutdown` is called a second time, during or after the first
  THEN it returns `Ok` once the first has completed, and the cleanup hook has run exactly once
- **AC-5** Cleanup hook, in order
  WHEN an `Environment` overrides `Environment::on_shutdown`
  THEN it runs once, after the manager is closed and before the background tasks stop
- **AC-6** Background tasks stop
  WHEN a queued environment is shut down
  THEN its job-queue and expiration-monitor tasks end (runtime alive-task count back to baseline)
- **AC-7** Reads survive
  WHEN a finished `AssetRef` from before shutdown is read, or `get_asset_info` / `listdir` is called
  THEN they succeed
- **AC-8** A manager without lifecycle support
  WHEN `shutdown` is called on an external manager that has not opted in
  THEN it returns `Err` with `ErrorType::NotSupported` and changes nothing

Non-goals: freeing the environment (the reference-cycle design); a service registry (decision 2);
a timeout argument (decision 1); binding API (decision 4); `AsyncStore::sync`, a start-up repair.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. Adds `pub` items in `liquers-core`: `ShutdownMode`,
  `ManagerLifecycle`, `EnvRef::shutdown`, trait methods and an `Error` constructor (rules 3 and
  4).
- **Leading issue:** Proposed resolution, contract: the two modes and what is refused while
  draining.
- **Explanation:** run claims already mark work in flight, `AssetRef::cancel` settles a run and
  `JobQueue::shutdown` stops the loop. The design adds a shared counter and closed flag, and four
  contract choices with recommended answers.
- **Open questions:**
  1. **Proposed resolution, modes:** `Drain` (wait until idle, then close) and `Cancel` (close,
     cancel what runs, wait until it settles). No timeout argument: a caller wraps `Drain` in a
     timeout and then calls `Cancel`. The alternative, `Drain { timeout }`, needs a timer, which
     wasm32 lacks.
  2. **Proposed resolution, cleanup hook:** one provided `Environment::on_shutdown(envref)`,
     default no-op, static like `apply_recipe`. No registry until a second service exists. None
     exists today.
  3. **Proposed resolution, admission:** `Drain` refuses nothing until idle, so work in flight can
     still request dependencies. It then closes atomically, only at count zero. Under continuous
     load it may not return (hence 1). Reads (`get_asset_info`, `listdir`, `contains`, a held
     `AssetRef`) are never refused.
  4. **Proposed resolution, bindings:** unchanged (`liquers-py`'s cell flag and inherent
     `shutdown`; `liquers-web` drops the global). Either may adopt `EnvRef::shutdown` later.
  5. **Implementation detail (resolved):** an external manager opts in through
     `AssetManager::lifecycle`, default `None` (AC-8).

### Design Dependencies

- `environment-manager-reference-cycle` (ready, not implemented): **requires** (order). Both
  change `RunClaim`, `InlineRunClaim` and `try_claim_for_run*` (its pin, this guard), so it lands
  first. It split this feature off (decision 4; E5 keeps them two designs).
- `asset-cancellation-outcome` (complete): owns what a cancelled asset reports (AC-2).
- `asset-garbage-collection`: **overlaps**. `stop_background_tasks` also stops its collector task.
- `asset-registration-contract`: **overlaps**. Shutdown ends work, not registrations.

### Consolidated Findings

- The in-flight count lives in the run claims, so it covers both managers and unregistered work
  (`apply`, `cached: false`, volatile), which a count of map entries would miss. The queued
  manager's unstarted jobs count too (`JobQueue::waiting_jobs_count`).
- Close and admission share one atomic word (closed bit plus count): "close only if idle" and
  "admit only if open" cannot interleave wrongly.
- A claim refused after the close fails its asset with the shut-down error, so waiters wake (as the
  reference-cycle design does for a failed environment upgrade).
- Runtime-free (`Notify`, atomics), so the inline path is the same on wasm32.

## Phase 2: Architecture

### Solution

A `ManagerLifecycle<E>` owned by each built-in manager records the open/closed state and the work
in flight. `AssetRef::try_claim_for_run` and `try_claim_for_run_inline` take a `WorkGuard` from it,
held inside `RunClaim` / `InlineRunClaim`, so the count rises when a run is claimed and falls when
the claim is dropped. `AssetManager::close(mode)` runs the drain or cancel protocol over it.
`EnvRef::shutdown` sequences close, the hook and the task stop. Entry points that start or change
work check `is_closed()` first.

Rejected: a flag without an in-flight count (no `Drain`); counting map entries (misses
unregistered work); spawning the hook (no spawn on wasm32); `Drop` cleanup (cannot await).

Known-issue preflight: `INLINE-RUN-CLAIM-REPAIR-SKIPPED-UNDER-LOCK-CONTENTION` leaves an asset
`Processing` with its claim (and guard) dropped, so it cannot stall `Drain`. Nothing blocks.

### Changes

`liquers-core/src/assets.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShutdownMode { Drain, Cancel }

/// Open/closed state and work in flight of one asset manager.
pub struct ManagerLifecycle<E: Environment> { /* state: AtomicU64 (closed bit | count),
    active: scc::HashMap<u64, WeakAssetRef<E>>, idle: tokio::sync::Notify,
    done: tokio::sync::Notify, phase: AtomicU8 (Open, Closing, Closed) */ }
impl<E: Environment> ManagerLifecycle<E> {
    pub fn new() -> Self;
    pub fn is_closed(&self) -> bool;
    pub(crate) fn begin_work(&self, asset: &AssetRef<E>) -> Result<WorkGuard<E>, Error>;
}
pub(crate) struct WorkGuard<E: Environment>;   // Drop: remove from `active`, decrement, notify `idle` at 0

// AssetManager<E>, provided methods
fn lifecycle(&self) -> Option<&ManagerLifecycle<E>> { None }
/// Extra work a manager holds outside run claims (the queued manager's unstarted jobs).
async fn has_pending_work(&self) -> bool { false }
/// Close: `Ok(true)` for the call that closed, `Ok(false)` after waiting for an earlier one.
async fn close(&self, mode: ShutdownMode) -> Result<bool, Error>;
fn stop_background_tasks(&self) {}
```

- `close` (provided): no lifecycle → `Err(Error::not_supported(..))`. `Drain`: loop { create
  `idle.notified()`; break if `!has_pending_work().await` and close-if-idle succeeds; await it }.
  `Cancel`: set the closed bit, `cancel()` every upgradable entry of `active`, wait for count zero
  the same way. Then phase `Closed`, `done.notify_waiters()`. A call that finds phase `Closing` waits on `done`; if it is a
  `Cancel`, it first sets the closed bit and cancels what is active, which escalates a `Drain` in
  progress.
- Both built-in managers: a `lifecycle` field returned by `lifecycle()`. The queued one's
  `has_pending_work` is `job_queue.waiting_jobs_count().await > 0` and its `stop_background_tasks`
  calls the inherent `shutdown()`. A closed check (`Err(Error::shut_down(..))`) opens `get_asset`,
  `get`, `apply`, `get_dependency_asset` (where overridden), `set_state`, `set_binary`, and the
  provided `remove` and `expire`.
- `RunClaim`, `InlineRunClaim`: a `_work: Option<WorkGuard<E>>` field, filled in
  `try_claim_for_run*` from `envref.get_asset_manager().lifecycle()`. A refusal sets the asset to
  `Error` with the shut-down error and notifies, so waiters wake.

`liquers-core/src/context.rs`:

```rust
// Environment, provided
fn on_shutdown(envref: EnvRef<Self>) -> crate::maybe_send::BoxFuture<'static, Result<(), Error>> {
    let _ = envref;
    Box::pin(async { Ok(()) })
}
impl<E: Environment> EnvRef<E> {
    /// Close the manager (`mode`), run `Environment::on_shutdown` once, stop background tasks.
    pub async fn shutdown(&self, mode: ShutdownMode) -> Result<(), Error>;
}
```

`liquers-core/src/error.rs`: `pub fn shut_down(what: &str) -> Self` with `ErrorType::NotAvailable`
and the message "the environment has been shut down (…)", the wording `liquers-py` already uses.

`close`, `shutdown`, `on_shutdown` are async; the rest is sync and lock-free. No command, registry
or format change.

Documents (History row, `reviewed:` bump each): `reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`
(shutdown beside teardown), `guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md` (stopping an environment),
`guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` §4 (the `lifecycle` opt-in).

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| Lost wake-up between idle check and wait | create `notified()` before checking | high |
| `Drain` never returns under load | decision 3, documented; AC-1 uses a bounded workload | high |
| Conflict with the reference-cycle edits of the claims | land that design first (Design Dependencies) | high |
| An entry point missed by the closed check | AC-3 enumerates the seven methods, with timeouts | medium |
| wasm32 | no task, no timer; `bash scripts/check-build-matrix.sh` wasm rows | high |

Recovery: one commit per step. Without steps 3-4 nothing calls the lifecycle, so behaviour is
unchanged.

## Phase 3: Examples and Tests

### Examples

The Problem Example is `shutdown01`. A graceful server stop wraps `Drain` in
`tokio::time::timeout` and calls `Cancel` when it elapses; the second call escalates the drain in
progress (Phase 2, `close`).

### Tests

New file `liquers-core/tests/environment_shutdown.rs`, each test on `SimpleEnvironment` and
`ImmediateEnvironment` unless stated, with `slow` (an async command gated by a test-held `Notify`)
and `hello`; every await is under `tokio::time::timeout(2s)`.

- `shutdown01_drain_waits_for_running_and_queued_work` (AC-1): start `slow`, queue a second (queued
  manager, capacity 1), call `Drain`, check it is still pending, open the gate, then both values
  are readable.
- `shutdown02_cancel_settles_running_work` (AC-2): start `slow`, call `Cancel`; the asset status is
  `Cancelled` and shutdown returned.
- `shutdown03_later_calls_fail_not_available` (AC-3): after shutdown, each of the seven entry
  points returns `Err` with `ErrorType::NotAvailable`.
- `shutdown04_second_call_is_a_noop` (AC-4): a counting `on_shutdown` on a test environment wrapper.
  Two sequential calls and two concurrent calls: count 1, all `Ok`.
- `shutdown05_hook_runs_after_close_before_tasks_stop` (AC-5): the hook observes
  `lifecycle().is_closed()` true and, on the queued manager, the job-queue task still alive.
- `shutdown06_queued_background_tasks_exit` (AC-6, queued only):
  `Handle::current().metrics().num_alive_tasks()` returns to its pre-build value.
- `shutdown07_reads_survive` (AC-7): a `Ready` `AssetRef` from before shutdown returns its value;
  `get_asset_info` and `listdir` on an `AsyncMemoryStore` key succeed.
- `shutdown08_external_manager_not_supported` (AC-8): in
  `liquers-core/tests/external_asset_manager.rs`, `MinimalInlineAssetManager` (no lifecycle)
  returns `ErrorType::NotSupported`.

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test environment_shutdown --test
external_asset_manager`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. Prerequisite: `environment-manager-reference-cycle` implemented. Proof: its
  `environment_teardown` suite passes.
- [ ] 2. `tests/environment_shutdown.rs` and `shutdown08` (compile after step 5). Rollback: delete.
- [ ] 3. `error.rs` `Error::shut_down`; `assets.rs` `ShutdownMode`, `ManagerLifecycle`,
  `WorkGuard`, plus unit tests `lifecycle_close_if_idle_refuses_with_work` and
  `lifecycle_admit_after_close_fails`. Proof: `cargo test -p liquers-core --lib lifecycle`.
- [ ] 4. `assets.rs` `RunClaim`, `InlineRunClaim`, `try_claim_for_run*`: the guard and the refusal
  path. Proof: `cargo test -p liquers-core --lib --tests` (nothing changes while open). Rollback:
  revert 3-4.
- [ ] 5. `assets.rs` trait methods and both managers (field, overrides, the seven checks);
  `context.rs` `Environment::on_shutdown`, `EnvRef::shutdown`. Proof: `shutdown01`-`08`.
- [ ] 6. `DOC_04_…`, `ENVIRONMENT_CONSTRUCTION_GUIDE.md`, `ASSET_MANAGER_IMPLEMENTATION_GUIDE.md`
  §4 (History, `reviewed:`); doc comments of the inherent `shutdown`s point to `EnvRef::shutdown`;
  close the issue. Proof: `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`, `-p liquers-lib --lib --tests`;
`bash scripts/test-liquers-py.sh`; `bash scripts/check-build-matrix.sh` (wasm32);
`python3 scripts/docs_index.py --check`; diff review (only `liquers-core` and three documents).

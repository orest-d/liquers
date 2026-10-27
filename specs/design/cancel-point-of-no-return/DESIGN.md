---
id: CANCEL-POINT-OF-NO-RETURN
kind: design
title: A cancel cannot overtake a command that has returned Ok
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [core/assets, core/plan]
issues: [CANCEL-CAN-OVERTAKE-A-RETURNED-COMMAND]
created: 2026-10-20
---
# A cancel cannot overtake a command that has returned Ok

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20: Phases 1-4 written; Phase 4
implemented on 2026-10-27 (automatic fix). Origin: `asset-cancellation-outcome` (complete) decision D2.

## Phase 1: High-Level Design

### Purpose

Make D2 of `asset-cancellation-outcome` hold exactly: once a command has returned `Ok`, a cancel
request no longer drops the evaluation at the bookkeeping awaits that follow it. Today the run
races the whole compute future against the request, so any suspension after the command returns
lets a pending request discard a finished result.

### Problem Example

Verified at HEAD. `AssetRef::run_with_future` / `run_with_future_inline` race
`compute` (`evaluate_recipe_outcome` → `Environment::apply_recipe` → `interpreter::apply_plan_state`)
against `CancellationRequest::requested()`, compute polled first. After the command returns, the
same future still awaits the asset lock: `prefix_metadata` (`context.get_metadata()`), a trailing
`Filename` step (`context.set_filename`), `check_resolved_volatility`, `take_pending_dependencies`.

The common case makes this likely, not rare: a cancel requested *while a synchronous command runs*
(the guide's "if it returns `Ok`, the asset ends ready and is stored") is already pending when the
command returns. Scenario, queued manager, key `out.txt` with recipe `finish_anyway/out.txt`
(`liquers-validate --command finish_anyway -- 'finish_anyway/out.txt'`: `Ok`):

1. `finish_anyway` (sync) starts; the test calls `asset.cancel()`;
2. the command sees `is_cancelled()`, finishes anyway and is about to return `Ok("finished")`;
3. at that moment another task holds `asset.data` (the service loop writing a log line; in the
   test, a held write guard);
4. `prefix_metadata` suspends on the lock, `select!` polls the request: `CancelRequested`.

Today: the asset ends `Cancelled` and nothing is stored. Should be: `Ready`, `"finished"` stored,
the request dropped (`ASSET_LIFECYCLE.md` §5 table: "a completed run wins and the request is
dropped").

### Scope and Acceptance Criteria

- **AC-1** A returned command wins over a pending cancel (queued)
  WHEN a cancel is requested while a sync command runs, the command returns `Ok`, and the asset lock is held when the interpreter resumes
  THEN the asset ends `Ready`, its value is stored, and the request is cleared
- **AC-2** The same on the inline manager
  WHEN the scenario of AC-1 runs under `ImmediateEnvironment`
  THEN the asset ends `Ready` with the value
- **AC-3** A suspended async command is still cancelled
  WHEN a cancel is requested while an async command is suspended at an await
  THEN the evaluation is dropped there and the asset ends `Cancelled`
- **AC-4** A request between commands lands in the next command
  WHEN a cancel is requested during the first command of a two-command plan, that command returns `Ok`, and the second command suspends at an await
  THEN the asset ends `Cancelled` and the second command does not complete
- **AC-5** Unchanged cases
  WHEN a cancel is requested before the job starts, or while waiting for a dependency
  THEN the outcomes of `asset-cancellation-outcome` AC-3/AC-4 hold (its tests stay green)

Non-goals: making a cancel take effect at step boundaries where nothing suspends (a sync second
command still runs, as today); cancelling a plan with no command (`-R/data/x.csv`), which stays
cancellable until it finishes; HTTP or binding changes.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — a bug fix restoring documented behaviour (D2;
  `COMMAND_DESIGN_GUIDE.md` "a completed run wins"; `ASSET_LIFECYCLE.md` §5), inside
  `liquers-core` only: crate-private methods on the crate-private `CancellationRequest` and
  `Context`, a private field, two call sites in `interpreter.rs`. No `pub` signature, type,
  command or format change.
- **Leading issue:** None
- **Explanation:** the race already has the right shape; it only lacks a signal for "a command
  has returned and no cancellable step has begun since". The request object is shared by the run
  and every `Context` of it, so the interpreter can set that signal without the asset lock.
- **Open questions:** None. Settled in Phase 2: the signal is re-armed at the start of every step
  that can suspend on user code or a dependency, so between commands the old behaviour is kept.

### Design Dependencies

- `asset-cancellation-outcome` — **requires** (complete, E4; not extended): defines the race,
  D2 and D6 (`CancellationRequest::clear` after a successful finalization).
- `inline-run-claim-repair-under-lock-contention` — **overlaps**: both edit around
  `run_with_future_inline`; this one adds one call after the claim. No order.
- `dependency-resolved-once-per-evaluation` (complete) noted this issue in its preflight
  ("the spawned child changes where a cancel lands"); unaffected.

### Consolidated Findings

- Nested plans share one `Context` and one request, but never run two actions at once: a
  `Step::Plan` runs its steps sequentially, and a command's own nested evaluations
  (`Context::evaluate`, `apply_ad_hoc`, dependencies) are other assets with their own requests. So
  one flag, set on `Ok` and cleared at the next step that can suspend, is sound.
- A run starts uncommitted: a claim taken over after a dropped run must not inherit "committed".
- Only `GenericEnvironment::apply_recipe` (and a test environment) implement
  `Environment::apply_recipe`; both reach `apply_plan_state`, so the interpreter is the right
  place. A custom environment that bypasses it simply never commits, which is today's behaviour.
- `COMMAND_DESIGN_GUIDE.md` §Sync versus async currently words the guarantee as "once the last step
  … has returned and nothing else awaits" — the bug as documented. It and `DOC_03` change.

## Phase 2: Architecture

### Solution

1. `CancellationRequest` (`liquers-core/src/assets.rs`) gains `committed: AtomicBool` and:

   ```rust
   pub(crate) fn commit(&self);   // a command returned Ok: the request no longer drops the run
   pub(crate) fn reopen(&self);   // a step that can suspend begins: the request applies again
   ```

   `requested()` completes only while `is_requested() && !is_committed()`; it stays parked
   otherwise and re-checks on every notification. `reopen` stores `false` and, when a request is
   pending, calls `notify.notify_waiters()` so a parked `requested()` re-checks. `clear()` also
   stores `false`. `request()`, `is_requested()` and `cause()` are unchanged, so
   `Context::is_cancelled` still reports a request a command may want to see.
2. `Context` (`context.rs`) gains `pub(crate) fn commit_cancellation(&self)` and
   `pub(crate) fn reopen_cancellation(&self)`, forwarding to its shared request.
3. `interpreter.rs`:
   - `do_step`, `Step::Action` arm: when `execute_async(...).await` returns `Ok`, call
     `context.commit_cancellation()` before mapping the value (synchronously, in the same poll as
     the command's return).
   - `apply_plan_state` loop: before `do_step_state(step, …)`, call
     `context.reopen_cancellation()` when the private `fn step_can_suspend(step: &Step) -> bool`
     answers `true`. Explicit match: `Action`, `Evaluate`, `GetAsset`, `GetAssetBinary`,
     `GetAssetMetadata`, `GetAssetRecipe`, `GetAssetDirectory`, `GetResource`,
     `GetResourceMetadata`, `GetResourceDirectory` → `true`; `Info`, `Warning`, `Error`, `SetCwd`,
     `Filename`, `UseQueryValue`, `UseKeyValue`, `Plan` → `false` (a nested plan's own steps
     decide).
4. `run_with_future` and `run_with_future_inline`: `cancellation.reopen()` right after obtaining
   `cancellation`, before the race, so every run starts uncommitted.

`settle_run` and `complete_evaluation` are unchanged: a committed run reaches
`RunOutcome::Computed(Ok)`, and finalization clears the request (D6).

Rejected alternatives:

- **Commit when `apply_recipe` returns** (in `evaluate_recipe_outcome`): environment-agnostic but
  leaves `prefix_metadata` and a trailing `Filename` step inside the window.
- **Move the post-command bookkeeping out of the race** (split `apply_recipe`): the bookkeeping is
  spread over every step kind and nested plans; a much larger refactor.
- **Check the request at every step boundary** (`reopen` returning `Err` when a request is
  pending): makes step boundaries cancellation points, so a sync second command would no longer run
  after a request — a behaviour change beyond D2.

Known-issue preflight: `INLINE-RUN-CLAIM-REPAIR-SKIPPED-UNDER-LOCK-CONTENTION`,
`INLINE-MANAGER-DYNAMIC-DEPENDENCY-CHAIN-NESTS` (P3) touch neighbouring code only. Nothing blocks.

### Changes

- `liquers-core/src/assets.rs`: `CancellationRequest` (field, `commit`, `reopen`, `requested`,
  `clear`), `run_with_future`, `run_with_future_inline`.
- `liquers-core/src/context.rs`: two `pub(crate)` forwarding methods.
- `liquers-core/src/interpreter.rs`: `do_step` (`Step::Action`), `apply_plan_state`, new private
  `step_can_suspend`.
- No `pub` API, command, registry or format change. Sync atomics; executor-agnostic.
- Documents: `specs/guides/COMMAND_DESIGN_GUIDE.md` §Sync versus async (the guarantee as "once a
  command has returned `Ok` and no later step that can suspend has begun");
  `specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` (cancellation race), each with a
  History row and `reviewed:` bump.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A committed run stays uncancellable during a later slow step | every step that can suspend reopens first; AC-4 | high |
| A request made while committed is lost | the flag is a gate, not a drop: `reopen` notifies and `requested()` re-checks; AC-4 | high |
| A re-claimed run inherits "committed" | `reopen` at run start (both harnesses) | high |
| Existing cancellation tests change outcome | only the post-`Ok` window changes; run `asset_cancellation`, `asset_failure_contract`, `manager_parametric` | high |
| A command that loops on `is_cancelled()` after its own nested `Ok` | nested commands are other assets; the flag is per request | high |

Recovery: revert; the race returns to today's shape.

## Phase 3: Examples and Tests

### Examples

The Problem Example is AC-1's test, with the recipe `finish_anyway/out.txt` in a memory store at
the root (`env_with` in `liquers-core/tests/asset_cancellation.rs`). AC-4 evaluates the query
`first/then_wait` (`liquers-validate --command first --command then_wait -- 'first/then_wait'`:
`Ok`, encoded `first/then_wait`, two `Action` steps). Not `first/-/then_wait`, which encodes as
`-R/first/-/then_wait`, a resource key.

### Tests

In `liquers-core/tests/asset_cancellation.rs` (`multi_thread`, 4 workers, `within` bounds):

- `ac13_returned_command_wins_while_lock_is_held` — sync command: sets `started`, spins until
  `context.is_cancelled()`, then spins until the test sets `locked`, returns `Ok("finished")`. Test:
  `wait_started`; `tokio::spawn(asset.cancel())`; wait until `asset.is_cancelled()`; take
  `asset.data.write()`; set `locked`; sleep 100 ms; drop the guard; join. Asserts `Ready`, value
  `"finished"`, stored bytes, `!asset.is_cancelled()`. Fails at HEAD (`Cancelled`). Covers AC-1.
- `ac13_returned_command_wins_while_lock_is_held_inline` — the same on `ImmediateEnvironment`; the
  evaluation runs in a spawned task, the command publishes its `context.get_asset_ref()` for the
  test. Covers AC-2.
- `ac02_cancel_suspended_async_command` — existing. Covers AC-3.
- `ac14_cancel_during_first_command_lands_in_the_next` — `first` (sync) spins until cancelled and
  returns `Ok`; `then_wait` (async) sets `entered`, awaits a never-opened latch, sets `completed`.
  Asserts `Cancelled`, `entered` set, `completed` not set. Covers AC-4.
- `ac03_cancel_while_waiting_for_dependency`, `ac04_cancel_before_the_job_starts`,
  `ac01_completed_sync_run_wins_over_late_cancel` — existing. Cover AC-5.
- Unit, `assets.rs` `mod tests`: `cancellation_request_commit_gates_requested` — `request`, then
  `commit`: `requested()` stays pending (polled once); `reopen`: it completes. Covers AC-1, AC-4.

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test asset_cancellation`, then
`cargo test -p liquers-core --lib --tests`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `liquers-core/tests/asset_cancellation.rs` — add `ac13_…`, `ac13_…_inline`, `ac14_…`; the two `ac13` fail at HEAD — `cargo test -p liquers-core --test asset_cancellation`. Rollback: delete the tests.
- [x] 2. `assets.rs` `CancellationRequest` (`committed`, `commit`, `reopen`, `requested`, `clear`) and unit test `cancellation_request_commit_gates_requested` — `cargo test -p liquers-core --lib cancellation_request`. Rollback: revert.
- [x] 3. `context.rs` `commit_cancellation` / `reopen_cancellation`; `interpreter.rs` `do_step` (`Action`), `apply_plan_state`, `step_can_suspend`; `assets.rs` `run_with_future*` reopen at start — depends on 2 — step 1 tests pass; `cargo test -p liquers-core --lib --tests`. Rollback: revert steps 2-3 together.
- [x] 4. Docs: `COMMAND_DESIGN_GUIDE.md`, `DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` (History rows, `reviewed:`); close `CANCEL-CAN-OVERTAKE-A-RETURNED-COMMAND` with a resolution note — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests` (commands
that poll `is_cancelled`); `bash scripts/check-build-matrix.sh` (wasm32 rows);
`python3 scripts/docs_index.py --check`; diff review for edits outside the three source files, the
test file and the two documents.

### Implementation notes (2026-10-27)

- `ac14`: the query `first/then_wait` is not one plan by default; the planner cuts `first` off as
  a predecessor asset, so the cancel landed on the dependent waiting in `Dependencies` and never
  reached `then_wait`. The test registers `first` with `cached: false`, which keeps both commands
  steps of one plan, as the scenario intends.
- The sync test commands spin with a ten-second deadline, so a failing assertion cannot leave a
  worker thread spinning and hang the test binary at runtime shutdown.

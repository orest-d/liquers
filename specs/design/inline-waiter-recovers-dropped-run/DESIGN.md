---
id: INLINE-WAITER-RECOVERS-DROPPED-RUN-DESIGN
kind: design
title: A caller parked on an inline run recovers when the runner is dropped
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [core/assets]
issues: [INLINE-DROP-REPAIR-STRANDS-EXISTING-WAITERS]
created: 2026-10-19
---
# A caller parked on an inline run recovers when the runner is dropped

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written; Phase 4 implemented on
2026-10-20 (automatic fix).

## Phase 1: High-Level Design

### Purpose

On a manager with no job queue, a caller that lost the execute-once claim must not wait forever
when the winner's future is dropped. It should notice the `Drop` repair, take the claim itself and
run the asset, or return once someone else has finished it.

### Problem Example

Verified at HEAD in `liquers-core/src/assets.rs`. Two callers ask an `ImmediateEnvironment` for the
same asset. The command is `slow`, an async command that blocks on its first call and returns
`"y"` on later calls:

```rust
let m = envref.get_asset_manager();
let mut a = Box::pin(m.get_asset(&parse_query("slow")?));   // A wins the claim, command blocks
assert!(futures::poll!(a.as_mut()).is_pending());
let mut b = Box::pin(m.get_asset(&parse_query("slow")?));   // B loses, parks in wait_to_finish
assert!(futures::poll!(b.as_mut()).is_pending());
drop(a);                                                     // InlineRunClaim::drop -> Recipe
tokio::time::timeout(Duration::from_secs(2), b).await;       // today: Elapsed, B never returns
```

Today, `AssetRef::run_with_future_inline` sends the loser to `wait_to_finish()` once, and that
returns only on `JobFinished` or `Removed`. `InlineRunClaim::drop` sets `Status::Recipe` and sends
`StatusChanged(Recipe)`. Nobody runs the asset after that, so `JobFinished` never comes.

What should happen: B wakes on `StatusChanged(Recipe)`, claims the asset, runs `slow` (second call,
returns `"y"`) and returns. The asset ends `Ready`.

### Scope and Acceptance Criteria

- **AC-1** A parked caller takes over a dropped run
  WHEN caller B is parked on caller A's inline run and A's future is dropped while the command is running
  THEN B claims the asset, runs the command and returns, the asset is `Ready`, and the command body was entered twice in total
- **AC-2** Execute-once still holds
  WHEN two callers request the same asset at once with a yielding command and neither is dropped
  THEN the command body runs once and both callers get the value
- **AC-3** A finished run releases the waiter without a second run
  WHEN B is parked and A's run ends `Ready`, `Error` or `Cancelled`
  THEN B returns without running the command, and the asset keeps A's terminal status
- **AC-4** Intermediate notifications do not start a second run
  WHEN B is parked and A's asset moves `Processing` → `Dependencies` → `Processing`, or reports progress
  THEN B does not run the command and stays parked until A finishes
- **AC-5** Several parked callers
  WHEN B and C are both parked and A's future is dropped
  THEN exactly one of B and C runs the command, the other waits for it, and both return with the asset `Ready`
- **AC-6** Removal still releases the waiter
  WHEN B is parked and the asset is removed (`AssetNotificationMessage::Removed`)
  THEN B returns, as it does today

Non-goals: the queued path (`RunClaim` re-submits to the `JobQueue` and is not affected); a repair
that `InlineRunClaim::drop` skips because `try_write` failed (filed separately as
`INLINE-RUN-CLAIM-REPAIR-SKIPPED-UNDER-LOCK-CONTENTION`); changes in `liquers-web`. The fix is in
`liquers-core` and reaches wasm32 unchanged. A browser regression test belongs to the
`liquers-web` project and is out of scope here.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible. A bug fix in one function of `liquers-core/src/assets.rs`
  (`run_with_future_inline`) plus one private helper. No new type, no `pub` signature change, no
  command or registry change.
- **Leading issue:** None
- **Explanation:** the repair already sends the notification the waiter needs, and the loser still
  has its own unpolled `compute` future, so it can run the asset after claiming. The contract is the
  one `ASSET_LIFECYCLE.md` §7 already states: one caller runs and the others wait.
- **Open questions:** None. Implementation details settled in Phase 2: subscribe before claiming,
  and the order of the exit checks.

### Design Dependencies

- `environment-manager-reference-cycle`: **overlaps** (`in_review`). It adds an environment pin
  field to `InlineRunClaim` and an upgrade in `try_claim_for_run_inline`. This design only calls
  those and changes the loser branch of `run_with_future_inline`. Either order works, and the
  second to land rebases trivially.
- `evaluate-path-consolidation` (complete, E4): introduced `InlineRunClaim`
  (`INLINE-PATH-LACKS-EXECUTE-ONCE`). This design keeps its execute-once guarantee (AC-2).
- `INLINE-RUN-CLAIM-REPAIR-SKIPPED-UNDER-LOCK-CONTENTION` (issue, filed with this design):
  **overlaps**. It covers the case where the repair itself never happens, which this waiter loop
  cannot see. It has no design yet. No ordering constraint.

### Consolidated Findings

- The fix belongs to the waiter, not to `Drop`. `Drop` cannot await, cannot spawn on wasm32 and
  already notifies, so the waiter re-examines the asset on every notification.
- **Subscribe before claiming.** A `watch` receiver created by `subscribe()` marks the current value
  as seen. If the waiter subscribed after a failed claim, a repair in that gap would leave the
  receiver with nothing new, and the waiter would hang again.
- Waking happens only on a genuine notification, never in a busy loop. Each wake costs one
  `try_claim_for_run_inline` (one write lock).
- The waiter runs its own `compute` future, with its own payload. Inline-run assets that share
  callers come from the asset maps. Payload assets are ad hoc and never shared, so a payload
  mismatch cannot arise.
- `ASSET_LIFECYCLE.md` §7 and the `run_inline` doc comment currently describe the limitation as
  open, so both change with the fix.

## Phase 2: Architecture

### Solution

In `AssetRef::run_with_future_inline`, replace the single claim attempt and `wait_to_finish()`
fallback with a private helper that loops:

```rust
/// Claim the inline run, or wait until it is no longer needed. `Ok(None)`: finished or removed.
async fn claim_or_wait_inline(&self) -> Result<Option<InlineRunClaim<E>>, Error>;
```

Each iteration:

1. `let mut rx = self.subscribe_to_notifications().await;` (first iteration only; the receiver is
   reused afterwards).
2. `try_claim_for_run_inline()`. If it returns `Some(claim)`, return it.
3. If `self.status().await.is_finished()` returns true, or the latest notification is `JobFinished`
   or `Removed`, return `Ok(None)`. These are the exit conditions `wait_to_finish` has today, plus
   the status check, which also covers a waiter that subscribed after `JobFinished` was sent.
4. `rx.changed().await`. On a closed channel, return the same `general_error` that
   `wait_to_finish` returns. Then loop to step 2.

The caller does `let claim = match self.claim_or_wait_inline().await? { Some(c) => c, None =>
return Ok(()) };`. Everything after it (the cancellation check, the `eval_side` / `psm_side` join,
`claim.complete()`, `finish_run_with_result`) stays the same. A waiter that claims after a repair
then runs exactly like a first caller. `resolve_volatility_before_evaluation` and the initial
`is_finished()` check already ran above the claim and stay valid. Statuses that are not finished
and not claimable (`Processing`, `Dependencies`, `Partial`, `Storing`, `Submitted`) all fall
through to step 4. The `match` in `try_claim_for_run_inline` is explicit and is not changed.

`wait_to_finish` itself is unchanged. The winner's `FinishedElsewhere` arm still uses it.

Rejected alternatives:

- **Send `JobFinished` from `Drop` on repair.** This releases the waiter, but it returns with an
  asset in `Recipe` and no value. Every caller of `run_inline` (`ImmediateAssetManager` paths,
  `run_dependency`) then fails with "did not produce a value" or reads a non-final status.
- **A new `AssetNotificationMessage::RunAbandoned`.** It changes a public enum, makes every
  `match` on it bigger, and gives the waiter no information beyond `StatusChanged(Recipe)`.
- **Poll the status on a timer.** It needs a timer on wasm32 and wakes for no reason.

Known-issue preflight: `INLINE-MANAGER-DYNAMIC-DEPENDENCY-CHAIN-NESTS` (P3) concerns stack depth,
not waiting, and is independent. `environment-manager-reference-cycle` touches `InlineRunClaim`'s
fields only (see Design Dependencies). Nothing open blocks this.

### Changes

- `liquers-core/src/assets.rs`
  - `AssetRef::run_with_future_inline`: the loser branch, as above.
  - new private `AssetRef::claim_or_wait_inline`.
  - `InlineRunClaim::drop`: update the comment ("does not release a caller already parked") to say
    the parked caller now wakes on this notification. The code is unchanged.
  - `AssetRef::run_inline`: drop the doc-comment sentence that names this issue.
- No `pub` item, command, serialized format or other crate changes. Sync/async: async, executor
  agnostic (`watch::Receiver::changed` is runtime-free), so wasm32 keeps working.
- Documents: `specs/reference/ASSET_LIFECYCLE.md` §7. Replace the "does not release a caller
  already parked" paragraph with the takeover rule, and add a History row and a `reviewed:` bump.
  `specs/guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` ("Who runs an asset?") is still true and
  needs no change.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| Lost wake-up between a failed claim and the subscription | subscribe first (Consolidated Findings). AC-1 with the drop placed after B parks | high |
| A spin when notifications are frequent (progress, `Dependencies`) | one claim attempt per real notification. AC-4 shows no second run | high |
| Two waiters both run | the claim is one write-locked transition. AC-5 | high |
| Missed repair (`try_write` fails), so the waiter still hangs | out of scope, filed as `INLINE-RUN-CLAIM-REPAIR-SKIPPED-UNDER-LOCK-CONTENTION` | high |
| Existing inline tests depend on the old loser behaviour | no test asserts a hang. Run the full core suite and `manager_parametric` | high |
| wasm32 | no new runtime use. `bash scripts/check-build-matrix.sh` wasm rows | high |

Recovery: the change is local to one function and one helper. Reverting restores the old wait.

## Phase 3: Examples and Tests

### Examples

The Problem Example is test `inline_waiter_takes_over_dropped_run`. A command named `slow`, as a
query, validates with `liquers-validate --command slow -- 'slow'` (status `Ok`, encoded `slow`).

### Tests

All tests use `ImmediateEnvironment<Value>` with `register_async_command` and two or three
`get_asset` futures on one query, polled by hand with `futures::poll!`. They go in
`liquers-core/tests/manager_parametric.rs`, beside `immediate_concurrent_yielding_command_runs_once`.
The gate is a `tokio::sync::Notify` or a `futures::channel::oneshot` held by the test. Each awaited
caller is wrapped in `tokio::time::timeout(2s)` so that a regression fails instead of hanging.

- `inline_waiter_takes_over_dropped_run`: the Problem Example. B returns `Ok`, the value is `"y"`,
  and the entry counter is 2. Covers AC-1. Fails (times out) at HEAD.
- `immediate_concurrent_yielding_command_runs_once`: existing test, unchanged, must stay green.
  Covers AC-2.
- `inline_waiter_returns_when_runner_finishes`: three cases (command returns `Ok`, returns `Err`,
  asset cancelled through `AssetRef::cancel` while A is pending). B returns, the counter is 1, and
  the status is `Ready` / `Error` / `Cancelled`. Covers AC-3.
- `inline_waiter_ignores_intermediate_notifications`: while B is parked, the test calls
  `set_status(Dependencies)` and then `set_status(Processing)` on the asset and sends a progress
  entry. B is still pending, the counter is 1, and then the gate opens and both return. Covers AC-4.
- `inline_two_waiters_one_takes_over`: B and C are parked, A is dropped, both are awaited with
  `futures::join!`. Both return `Ok`, the status is `Ready`, and the counter is 2. Covers AC-5.
- `inline_waiter_released_by_removal`: B is parked, then the test calls
  `AssetRef::notify_removed` on the asset (the message `AssetManager::remove` sends). B returns.
  Covers AC-6.

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test manager_parametric`, then
`cargo test -p liquers-core --lib --tests`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `liquers-core/tests/manager_parametric.rs`: add the five new tests. Only
  `inline_waiter_takes_over_dropped_run` and `inline_two_waiters_one_takes_over` fail at HEAD (by
  timing out). Proof: `cargo test -p liquers-core --test manager_parametric`. Rollback: delete the
  tests.
- [x] 2. `liquers-core/src/assets.rs`: add `AssetRef::claim_or_wait_inline` and use it in
  `run_with_future_inline` (depends on 1). Proof: all six tests in step 1, plus
  `cargo test -p liquers-core --lib --tests`. Rollback: revert the commit.
- [x] 3. `assets.rs`: doc comments of `InlineRunClaim::drop` and `run_inline`. Proof:
  `cargo doc -p liquers-core --no-deps`.
- [x] 4. `specs/reference/ASSET_LIFECYCLE.md` §7: the takeover rule, a History row and a
  `reviewed:` bump. Close `INLINE-DROP-REPAIR-STRANDS-EXISTING-WAITERS` with a resolution note.
  Proof: `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`; `bash scripts/check-build-matrix.sh` (wasm32 rows);
after `cargo clean`, `cargo test -p liquers-web --target wasm32-unknown-unknown --features
debug-handles` as a smoke run of the inline path in its delivery form (no new web tests);
`python3 scripts/docs_index.py --check`; a diff review for debug output and for edits outside
`assets.rs`, the test file and `ASSET_LIFECYCLE.md`.

### Implementation notes (2026-10-20)

- The tests are in `liquers-core/src/assets.rs`'s own test module, not in
  `tests/manager_parametric.rs`: AC-4 needs `AssetRef::set_status` and the notification sender,
  which are crate-private. The AC-3 test is split into three (`…_runner_succeeds`, `…_runner_fails`,
  `…_run_is_cancelled`), and the cancel case polls A alongside `AssetRef::cancel`, which on native
  waits for the run to settle.
- `claim_or_wait_inline` matches `AssetNotificationMessage` explicitly (no `_ =>` arm).

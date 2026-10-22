---
id: INLINE-RUN-CLAIM-REPAIR-UNDER-LOCK-CONTENTION
kind: design
title: A dropped inline run is recoverable even when its asset's lock is busy at drop time
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [core/assets]
issues: [INLINE-RUN-CLAIM-REPAIR-SKIPPED-UNDER-LOCK-CONTENTION]
created: 2026-10-20
---
# A dropped inline run is recoverable even when its asset's lock is busy at drop time

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20: Phases 1-4 written, not
approved, nothing implemented. Follow-up of `inline-waiter-recovers-dropped-run` (implemented in
PR #167), which made a parked caller react to the `Drop` repair but could not help when the repair
never happens.

## Phase 1: High-Level Design

### Purpose

On a manager with no job queue, a runner whose future is dropped must always leave its asset
claimable, so that a parked caller or the next caller runs it. Today that holds only when
`InlineRunClaim::drop` gets the asset's write lock with `try_write`.

### Problem Example

Verified at HEAD (`liquers-core/src/assets.rs` `InlineRunClaim::drop`): the repair
(`Processing`/`Dependencies` → `Recipe` plus `StatusChanged(Recipe)`) runs only inside
`if let Ok(mut lock) = self.asset.data.try_write()`; on `Err` nothing happens. PR #167's
`claim_or_wait_inline` then waits for a notification that never comes, and
`try_claim_for_run_inline` refuses `Processing` for every later caller.

Deterministic form, with the test fixture `SlowGate` / `slow_env` of `assets.rs` (command `slow`
blocks on its first call, returns `"y"` afterwards; `liquers-validate --command slow -- 'slow'`
is `Ok`):

```rust
let mut a = Box::pin(m.get_asset(&query));            // A claims, command blocks
assert!(futures::poll!(a.as_mut()).is_pending());
let mut b = Box::pin(m.get_asset(&query));            // B parks in claim_or_wait_inline
assert!(futures::poll!(b.as_mut()).is_pending());
let guard = gate.asset().data.read().await;           // any reader holds the lock
drop(a);                                              // try_write fails: repair skipped
drop(guard);
timeout(2s, b).await                                  // today: Elapsed; and so is any later get_asset
```

Should be: B takes the run over, runs `slow` (second call, `"y"`) and returns; the asset ends
`Ready`. A caller arriving later, with nobody parked, likewise runs it.

### Scope and Acceptance Criteria

- **AC-1** Parked caller takes over a run whose repair was skipped
  WHEN caller B is parked on caller A's inline run and A's future is dropped while another task holds the asset's lock
  THEN B claims the asset, runs the command and returns, the asset is `Ready`, and the command body was entered twice in total
- **AC-2** A later caller runs it
  WHEN A's future is dropped while the lock is held and no caller is parked
  THEN the next `get_asset` for that query runs the command and returns the value instead of waiting forever
- **AC-3** Execute-once still holds under contention
  WHEN two callers request the same asset at once, nothing is dropped, and other tasks read the asset's status while it runs
  THEN the command body runs once and both callers get the value
- **AC-4** Uncontended repair unchanged
  WHEN A's future is dropped while the lock is free
  THEN behaviour is that of `inline-waiter-recovers-dropped-run` (its tests stay green)
- **AC-5** Removal stays terminal
  WHEN the repair was skipped and the asset is then removed before the parked caller wakes
  THEN the parked caller returns without running the command

Non-goals: the queued `RunClaim` (it repairs from a spawned task that awaits the lock); callers
waiting through `AssetRef::get` without running (unchanged, as for the uncontended repair);
`liquers-web` changes (the fix is in `liquers-core` and reaches wasm32 unchanged).

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — a bug fix in `liquers-core/src/assets.rs` restoring the
  contract `ASSET_LIFECYCLE.md` §7 states (a vanished runner leaves the asset runnable). It adds
  private fields to the private `AssetData` and the crate-private `InlineRunClaim`; no new type, no
  `pub` signature, command or format change.
- **Leading issue:** None
- **Explanation:** `Drop` cannot await or (on wasm32) spawn, but it can set an atomic and send on a
  `watch` channel without the asset lock. The claim, which already runs under the write lock, is
  the natural place to finish the repair.
- **Open questions:** None. Settled in Phase 2: the mark is honoured only by the inline claim, and
  the wake-up uses `send_modify` so no misleading notification is published.

### Design Dependencies

- `inline-waiter-recovers-dropped-run` — **requires** (implemented, PR #167; design `in_review`
  at `implementation`): this design reuses its `claim_or_wait_inline` loop unchanged.
- `environment-manager-reference-cycle` — **overlaps** (in_review): it adds an environment pin
  field to `InlineRunClaim` and an upgrade in `try_claim_for_run_inline`. Different fields of the
  same struct; either order, trivial rebase. (A close T3 call: separate fields do not conflict.)
- `cancel-point-of-no-return` — **overlaps**: both touch `run_with_future_inline`'s neighbourhood;
  this one only the claim. No order.

### Consolidated Findings

- The waiter needs two things the skipped repair withholds: a status it may claim and a wake-up.
  A lock-free mark supplies the first (read by the next claim under the write lock), and
  `watch::Sender::send_modify` on a sender cloned into the claim supplies the second without
  taking the asset lock (tokio 1.54: `Sender` is `Clone`, `send_modify` is sync and notifies
  unconditionally).
- The mark must be cleared by every successful claim, so a mark left by an earlier dropped run
  can never let a third caller take over a live run.
- The mark is set only by an armed `InlineRunClaim`, i.e. only when the inline runner is gone; a
  queued run never sets it, so `try_claim_for_run` is unchanged.
- `ASSET_LIFECYCLE.md` §7 and the `InlineRunClaim::drop` comment ("the safe direction") describe
  the skipped repair as acceptable; both change.

## Phase 2: Architecture

### Solution

1. `AssetData` gains a private `inline_run_abandoned: Arc<AtomicBool>` (initialized `false` in
   `AssetData::new_ext`).
2. `InlineRunClaim` gains `abandoned: Arc<AtomicBool>` and
   `notification_tx: watch::Sender<AssetNotificationMessage>`, both cloned from the `AssetData`
   in `try_claim_for_run_inline` while it holds the write lock.
3. `try_claim_for_run_inline` (explicit match kept):
   - `None | Recipe | Submitted` → as today, and `inline_run_abandoned.store(false)`;
   - `Processing | Dependencies` → if `inline_run_abandoned.swap(false, AcqRel)` the previous
     inline runner is gone: append a warning log entry ("previous inline run was dropped; running
     again"), `set_status(Processing)` and return `Some(claim)`; otherwise `None` as today;
   - every other status → `None`, as today.
4. `InlineRunClaim::drop`, when armed: `try_write` succeeds → today's repair, unchanged. It fails →
   `abandoned.store(true, Release)`, then `notification_tx.send_modify(|_| {})`, which wakes every
   receiver (the parked `claim_or_wait_inline`, `AssetRef::get` waiters re-poll harmlessly) without
   changing the latest message.

`claim_or_wait_inline` needs no change: a wake-up makes it re-check `Removed`, retry the claim (now
granted) or wait again.

Rejected alternatives:

- **Spawn the repair on native** (as `RunClaim`): needs a runtime, does nothing on wasm32, where
  the issue matters most.
- **Send `StatusChanged(Recipe)` from `Drop` without the lock:** wakes the waiter but announces a
  status the asset does not have; observers (UI) would read `Recipe` and see `Processing`.
- **A new `AssetNotificationMessage::RunAbandoned`:** a public enum change for information the
  claim already reads from the mark.
- **A liveness `Weak` of the runner in `AssetData`:** equivalent to the mark but still needs a
  lock-free wake-up, and makes `Dependencies` bookkeeping depend on it.

Known-issue preflight: `INLINE-MANAGER-DYNAMIC-DEPENDENCY-CHAIN-NESTS` (P3, stack depth) and
`CANCEL-CAN-OVERTAKE-A-RETURNED-COMMAND` (P3) are independent. Nothing blocks.

### Changes

- `liquers-core/src/assets.rs`: `AssetData` (field, `new_ext`), `InlineRunClaim` (fields, `Drop`,
  comment), `AssetRef::try_claim_for_run_inline`; `run_inline` doc sentence on recovery.
- No `pub` item, command, serialized format or other crate changes. Sync `Drop`, async claim;
  executor-agnostic (atomics and `tokio::sync::watch`, already used on wasm32).
- Documents: `specs/reference/ASSET_LIFECYCLE.md` §7 — the takeover rule also covers a repair the
  dropped claim could not apply at once (History row, `reviewed:` bump).

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A stale mark lets a caller take over a live run | cleared by every successful claim, set only by an armed claim's `Drop`; AC-3 | high |
| Lost wake-up between the waiter's failed claim and the mark | the mark is stored before `send_modify`; the waiter's receiver was subscribed before its claim (PR #167), so the send is seen | high |
| `send_modify` wakes `get()` waiters spuriously | they re-poll state and wait again; one wake per dropped run | high |
| Existing inline tests rely on the old skip | none asserts a hang; run the whole core suite | high |
| wasm32 | no runtime use; `scripts/check-build-matrix.sh` wasm rows | high |

Recovery: the change is local to the claim and its guard; reverting restores the skip.

## Phase 3: Examples and Tests

### Examples

The Problem Example is test `inline_waiter_takes_over_run_dropped_under_contention`. The guard is a
plain `asset.data.read()` (`AssetRef::data` is `pub`), which makes `try_write` fail
deterministically.

### Tests

In `liquers-core/src/assets.rs` `mod tests`, beside the PR #167 tests, using `SlowGate`,
`slow_env`, `gate.asset()` and `WAITER_TIMEOUT`:

- `inline_waiter_takes_over_run_dropped_under_contention` — the Problem Example; B returns `"y"`,
  status `Ready`, `gate.entered() == 2`. Covers AC-1. Times out at HEAD.
- `inline_later_caller_runs_asset_whose_repair_was_skipped` — A pending, read guard, `drop(a)`,
  drop the guard, then a fresh `m.get_asset(&query)` within `WAITER_TIMEOUT` returns `"y"`,
  `entered() == 2`. Covers AC-2. Times out at HEAD.
- `inline_status_readers_do_not_cause_a_second_run` — A and B pending; while the gate is closed the
  test reads `asset.status()` repeatedly; then the gate opens and both are joined; `entered() == 1`.
  Covers AC-3 (with the existing `manager_parametric::immediate_concurrent_yielding_command_runs_once`).
- `inline_waiter_takes_over_dropped_run`, `inline_two_waiters_one_takes_over`,
  `inline_waiter_released_by_removal` — existing, unchanged. Cover AC-4.
- `inline_waiter_does_not_rerun_removed_asset_after_skipped_repair` — A, B pending; read guard;
  `drop(a)`; drop the guard; `asset.notify_removed().await`; then B returns within the timeout and
  `entered() == 1`. Covers AC-5.

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib inline_`, then
`cargo test -p liquers-core --lib --tests`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core/src/assets.rs` `mod tests` — add the four new tests; `…_dropped_under_contention` and `…_whose_repair_was_skipped` time out at HEAD — `cargo test -p liquers-core --lib inline_`. Rollback: delete the tests.
- [ ] 2. `assets.rs` `AssetData` (`inline_run_abandoned`, `new_ext`), `InlineRunClaim` (fields), `try_claim_for_run_inline` (clone fields, clear or honour the mark) — depends on 1 — `cargo test -p liquers-core --lib inline_`. Rollback: revert the commit.
- [ ] 3. `assets.rs` `InlineRunClaim::drop` — mark and `send_modify` on a failed `try_write`; comment and `run_inline` doc updated — all tests of step 1 pass; `cargo test -p liquers-core --lib --tests`. Rollback: revert steps 2-3 together.
- [ ] 4. `specs/reference/ASSET_LIFECYCLE.md` §7 (History row, `reviewed:`); close `INLINE-RUN-CLAIM-REPAIR-SKIPPED-UNDER-LOCK-CONTENTION` with a resolution note — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`; `bash scripts/check-build-matrix.sh` (wasm32 rows);
after `cargo clean`, `cargo test -p liquers-web --target wasm32-unknown-unknown --features
debug-handles` as a smoke run of the inline path; `python3 scripts/docs_index.py --check`; diff
review for debug output and edits outside `assets.rs`, the reference document and the issue.

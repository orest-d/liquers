---
id: INLINE-RUN-CLAIM-REPAIR-SKIPPED-UNDER-LOCK-CONTENTION
kind: issue
title: A dropped inline run leaves its asset Processing forever when the asset lock is busy at drop time
status: closed
priority: P2
complexity: M
area: [core/assets]
design: inline-run-claim-repair-under-lock-contention
created: 2026-10-19
github:
---

## Problem

`InlineRunClaim::drop` (`liquers-core/src/assets.rs`) repairs a vanished inline runner's status
(`Processing` / `Dependencies` → `Recipe`) only if `self.asset.data.try_write()` succeeds. `Drop`
cannot await, and on wasm32 it cannot spawn, so a contended lock means the code skips the repair
and does nothing else. The comment there calls this "the safe direction": the next caller waits
instead of running the asset twice.

But nobody ever runs that asset again. It stays in `Processing`, a status only a live runner can
hold, and `try_claim_for_run_inline` treats that as active. So every later caller waits in
`wait_to_finish` for a `JobFinished` that will never come. This is not limited to callers already
parked. Once the repair is skipped, every future request for that asset hangs, until the asset is
removed or replaced.

## Problem Example

On a multi-threaded runtime with `ImmediateEnvironment`, task A runs `get_asset("slow")` and task B
calls `asset.status()` in a loop (it holds the read lock briefly, many times). When A's future is
dropped, its `InlineRunClaim::drop` can run while B holds the read lock. `try_write` returns `Err`,
the repair is skipped, and the asset stays `Processing`. Any later `get_asset("slow")` hangs. It
should instead re-park the asset as `Recipe` so the next caller runs it.

## Impact

Managers with no job queue (the immediate/inline managers, wasm32 and `liquers-web`), and only when
a runner is dropped mid-run while another task holds the asset's lock. That is rare, but the outcome
is a permanent hang for that asset, not a single stuck call. The queued `RunClaim` does not have this
problem: it repairs from a spawned task that awaits the lock.

## Expected behaviour

A runner that vanishes always leaves its asset claimable or finished, even when the lock is
contended at drop time. Possible directions, none chosen:

- a lock-free "abandoned" marker on the asset (for example an atomic beside `data` in `AssetRef`)
  that `try_claim_for_run_inline` honours alongside `Processing`;
- deferring the repair to the next lock holder (the waiter, or the next claim attempt), which checks
  for an abandoned claim;
- on native targets, falling back to a spawned repair, as `RunClaim` does. This does not help wasm32.

## Discovery

Found on 2026-10-19 while designing `inline-waiter-recovers-dropped-run`
(`INLINE-DROP-REPAIR-STRANDS-EXISTING-WAITERS`). That design makes parked callers react to the
repair. It cannot help when the repair never happens, so it lists this as a non-goal and an overlap.

## Triage 2026-10-20

Still reproduces at HEAD (`18732dd`): `InlineRunClaim::drop` repairs only inside
`if let Ok(mut lock) = self.asset.data.try_write()`, and PR #167's `claim_or_wait_inline` waits for a
notification the skipped repair never sends. Design: `inline-run-claim-repair-under-lock-contention`
(a lock-free abandoned mark honoured by the next inline claim, plus a `send_modify` wake-up).

## Resolution

Fixed 2026-10-27 by `design/inline-run-claim-repair-under-lock-contention/`. A dropped `InlineRunClaim` that cannot take the asset's lock now marks the run abandoned
(`AssetData::inline_run_abandoned`) and wakes every receiver with `send_modify`; the next
`try_claim_for_run_inline` honours the mark on `Processing` / `Dependencies`, logs a warning and
takes the run over. Tests in `liquers-core/src/assets.rs`:
`inline_waiter_takes_over_run_dropped_under_contention`,
`inline_later_caller_runs_asset_whose_repair_was_skipped`,
`inline_status_readers_do_not_cause_a_second_run`,
`inline_waiter_does_not_rerun_removed_asset_after_skipped_repair`.

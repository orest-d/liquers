---
id: ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY
kind: issue
title: An asset evaluated from a stale dependency is persisted as Ready and only then labelled Expired
status: closed
priority: P1
complexity: M
area: [core/assets]
design: stale-dependency-status-finalization
created: 2026-09-03
github:
---

## Resolution (2026-09-15)

**Closed — fixed by `design/stale-dependency-status-finalization/`**
(PR [#71](https://github.com/orest-d/liquers/pull/71)).

The stale-dependency rule moved out of the harness and into the status authority.
`AssetRef::finalize_status_with_version` now decides `Expired` for an evaluation that consumed a
stale dependency, under the same write lock that installs the version and **before** the
notification and the store write. The relabel in `finish_run_with_result` is deleted, so there is
exactly one reader of the `stale_dependency` flag.

The store therefore holds `Expired` rather than `Ready`, and a process that reloads the entry after
a restart recomputes it instead of serving it. That is the exposure §Problem records as the real
cost — the in-process masking is exactly what does not survive a restart — and it is what raised
this issue to P1.

Two things came with the fix that the issue as written did not ask for:

- **The version is still registered.** `DependencyManager::track_asset` refuses an `Expired` asset,
  so finalizing before the dependency-graph step would have left the graph asserting the key still
  holds its previous content, and assets recorded against that content would never be invalidated.
  A stale-dependency **keyed** asset therefore registers its version directly, through
  `track_keyed_asset`. A delegating asset registers nothing; a volatile one is not a graph node at
  all.
- **The reading half.** `try_fast_track` now declines a stored asset when a recorded dependency is
  in a status it would not itself reuse, asking the manager first and the store only as a fallback.
  Without it the writing half is invisible across a process boundary in the other direction: the
  recorded-version guard is vacuous in a fresh process, which holds no versions to compare against.
  A dependency that cannot be addressed is inconclusive rather than expired and does not block.

**Evidence.** Unit tests U1–U8 on `finalize_status_with_version` in `liquers-core/src/assets.rs`;
`keyed_stale_dependency_is_stored_expired`, `stale_dependency_registers_its_version`,
`delegating_stale_dependency_registers_nothing`,
`volatile_keyed_stale_dependency_stays_volatile` and `non_keyed_stale_dependency_writes_nothing`;
`keyed_version_cascade::fast_track_*` (F0–F4) and the reload tests R1–R3. The keyed-persistence
test was run against the original behaviour reconstructed exactly — the relabel restored in
`finish_run_with_result`, after persistence — and **failed at the store assertion while the
in-memory assertion passed**, which is this issue reproduced and then closed.

The file:line citations in §Problem below predate `evaluate-path-consolidation` and were already
stale when this was filed; they are left as written rather than rewritten, since the code they
name no longer exists in that shape.

One piece of scope is deferred with an issue rather than silently dropped: the stale-dependency
path cannot be driven end to end from a command, because reaching `wait_for_dependency`'s expired
arm requires the dependency to expire between being scheduled and being waited on, and that window
is a race rather than a state a test can arrange. Filed as
`STALE-DEPENDENCY-PATH-HAS-NO-END-TO-END-TEST`.

`EXPIRY-RECORDS-NO-REASON` stays open. This design sets the ordering precedent and records the
recommended shape — a typed reason in metadata rather than a distinct `Stale` status — but does not
implement it.

## Problem

When a dependency expires mid-evaluation, the runtime uses its stale value rather than recomputing,
and marks the dependent asset so the next access recomputes it. That marking happens *after* the
asset has already been written to the store.

The order, in one evaluation:

1. `evaluate_and_store` calls `try_to_set_ready()` (`liquers-core/src/assets.rs:2329`), which sets
   `Status::Ready`.
2. It then calls `persist_with_status_tracking(...)` (`:2345`) — the value and its metadata,
   carrying `Ready`, go to the store.
3. Only afterwards, in `finish_run_with_result` (`:2050`), does the harness apply the stale-
   dependency rule:

```rust
if lock.stale_dependency && lock.status == Status::Ready {
    let _ = lock.set_status(Status::Expired);
    // "Asset evaluated with an expired dependency value; labeled expired for recomputation"
}
```

Nothing re-persists after that. `save_metadata_to_store` (`:831`) is driven only by
`process_service_messages`, and that loop has already terminated: `run_with_future` sends
`JobFinishing` before calling `finish_run_with_result`. So the store keeps `Ready` while memory
holds `Expired`.

## Impact

The in-memory asset and its persisted metadata disagree about a status whose whole purpose is to
force recomputation.

In-process the disagreement is masked: `try_fast_track` accepts a stored `Ready` only when the
recorded dependency versions are not stale, and they are stale here, so the value is not reused.
The exposure is outside that check — another process, a later run against the same store, or any
reader that trusts the stored metadata's status sees `Ready` for a value the producing run concluded was
expired. `AssetInfo` served from the store reports it too.

No data is lost and no wrong value is served **in-process**. Across a process boundary one is:
`AssetRef::try_fast_track` accepts a stored asset whose status is `Ready | Source | Override`, then
validates recorded dependency versions against the dependency manager — but only where the DM has a
version for that key (`if let Some(dm_version) = dm.get_version(...)`). In a fresh process the DM is
empty, so that guard is vacuous and every recorded dependency passes. A stale-dependency asset
stored as `Ready` is therefore **loaded and served without recomputation** by the next process that
asks for it. The in-process masking is exactly what does not survive a restart, which is the case
the persisted status exists for.

Raised to **P1** on 2026-09-04 (project owner, at the Phase 2 gate of
`stale-dependency-status-finalization`, which established the above): a correctness risk with a
workaround — expire the key or recompute deliberately — rather than P0, since no computation is
wrong, nothing is lost, and it needs both a mid-flight dependency expiry and a process boundary.

## Expected behaviour

The stale-dependency rule is part of finalizing the status, so it belongs with
`try_to_set_ready()` — before persistence — not in the harness after it. An asset that used a stale
dependency value should be written as `Expired`, or not written at all, so that the store and the
runtime agree.

## Discovery

Found on 2026-09-03 during Phase 3 of `specs/design/evaluate-path-consolidation/`, while writing a
test for the ordering invariant "status is finalized before persistence". The consolidated design
states that invariant explicitly; checking whether HEAD already honours it showed that it does not.
The design makes the ordering visible but does not by itself fix it — the stale-dependency rule
lives in the harness, which the consolidation keeps. The fix is designed separately in
`specs/design/stale-dependency-status-finalization/`, which is what `design:` now points at.

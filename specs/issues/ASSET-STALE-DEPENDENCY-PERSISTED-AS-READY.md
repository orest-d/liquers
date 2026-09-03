---
id: ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY
kind: issue
title: An asset evaluated from a stale dependency is persisted as Ready and only then labelled Expired
status: draft
priority: P2
complexity: M
area: [core/assets]
design: evaluate-path-consolidation
created: 2026-09-03
github:
---

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
reader that trusts the sidecar's status sees `Ready` for a value the producing run concluded was
expired. `AssetInfo` served from the store reports it too.

No data is lost and no wrong value is served in-process, which is why this is P2 rather than P1.

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
lives in the harness, which the consolidation keeps.

---
id: DEPENDENCY-EXPIRED-STALE-VALUE-UNREACHABLE
kind: issue
title: Execution-time expired dependency always fails its dependent; the stale-value branch is dead
status: closed
priority: P0
complexity: S
area: [core/assets]
design: expired-binary-read-safety
created: 2026-08-08
github:
---
## Problem

`DefaultAssetManager::wait_for_dependency` handles a dependency that expires *while the dependent
is already evaluating* (`liquers-core/src/assets.rs:4055-4078`). Its documented intent is to use
the stale value rather than recompute, because recomputing risks unbounded re-execution when the
dependency's freshness window is shorter than the dependent's evaluation time:

```rust
Status::Expired => {
    match dependency.poll_state().await {
        // Stale value still present: use it and propagate staleness
        Some(state) => {
            parent.note_expired_dependency(dependency).await?;
            // …
        }
        // Expired AND evicted: fail the dependent's evaluation.
        None => { /* Err("expired and was evicted before its value could be used") */ }
    }
}
```

**The `Some(state)` branch is unreachable.** PR #11 made `poll_state` return `None` for
`Status::Expired` (`assets.rs:795`) — deliberately, so that normal reads never expose expired data.
`wait_for_dependency` was not retargeted onto the recovery accessor, so its `poll_state` call now
returns `None` for every expired dependency.

Consequences:

1. Every execution-time expired dependency fails its dependent with *"expired and was evicted
   before its value could be used"* — including when the value is still fully present in memory.
   The error message states something untrue.
2. `note_expired_dependency` has no reachable production caller, so the staleness-propagation
   machinery it feeds (`stale_dependency` → the `Ready`→`Expired` relabel in
   `finish_run_with_result`, `assets.rs:1618-1631`) is also unreachable by this route.

This is the same defect class as `ASSET-EXPIRED-CACHED-BINARY-READ`: a gate was added to
`poll_state`, and an internal caller that needed the *ungated* read was not moved across.

## Expected behaviour

`wait_for_dependency`'s `Status::Expired` arm calls `poll_state_any_status()` (the explicit
recovery read added by PR #11 for exactly this purpose). The `Some` branch then behaves as
documented: use the stale value, call `note_expired_dependency`, and let the dependent be labelled
`Expired` at completion. The `None` branch retains its meaning — genuinely evicted.

## Verification

1. A dependent whose dependency expires mid-evaluation, with the dependency's value still retained,
   completes rather than failing.
2. That dependent ends at `Status::Expired` via the `stale_dependency` relabel, so the next access
   recomputes it.
3. `note_expired_dependency` has a reachable production caller, demonstrated by a test that does not
   call it directly.
4. A dependency that is expired *and* evicted still fails the dependent, with the existing message.

## Discovery

Found during the cross-phase review of the `expired-binary-read-safety` design, while auditing
which internal callers of a gated read need the ungated one. That design fixes the same mistake for
`poll_binary`/`save_to_store`; this one predates it and is out of its scope.

## Resolution

Fixed `DefaultAssetManager::wait_for_dependency` to use the explicit
`poll_state_any_status()` recovery read for execution-time expired dependencies. Regression tests
exercise the production wait path and verify both outcomes: a retained stale value lets the
dependent complete and become `Expired`, while an expired dependency with no retained value still
fails with the existing eviction error.

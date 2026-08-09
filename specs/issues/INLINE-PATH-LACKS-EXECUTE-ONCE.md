---
id: INLINE-PATH-LACKS-EXECUTE-ONCE
kind: issue
title: The inline run path has no execute-once claim, only an is_finished check
status: draft
priority: P2
complexity: M
area: [core/assets, web]
design:
created: 2026-08-09
github:
---

## Problem

The queued path guarantees that an asset's body runs exactly once, via `RunClaim` /
`AssetRef::try_claim_for_run` (`liquers-core/src/assets.rs:5099`): an atomic status transition
under one write lock, with `Status::Processing` and `Status::Dependencies` both non-claimable, and
a `Drop` repair that re-parks an asset whose runner vanished.

The inline path has none of it. `run_with_future_inline` (`:1768`) guards only on

```rust
if self.status().await.is_finished() {
    return Ok(());
}
```

which is precisely the window `RunClaim`'s own doc comment describes itself as closing —
*"closing the double-run window left by `run_with_future`'s `is_finished()`-only guard"*. Two
callers can both observe "not finished" and both run the body.

`RunClaim` cannot simply be reused: it is `#[cfg(not(target_arch = "wasm32"))]`, it takes an
`&Arc<JobQueue<E>>` that the inline manager does not have, and its `Drop` repair re-submits through
that queue using `tokio::spawn` — none of which exists on the target where the inline manager is
the *only* manager.

## Impact

On wasm the executor is single-threaded, so the window is narrow but not absent: any two futures
racing on the same asset across an await point can both enter. On native, `ImmediateEnvironment` is
usable from multiple tasks and the window is real.

The symptom of a double run is a command body executing twice — duplicated side effects, doubled
counters, and two writes to the same store key.

## Expected behaviour

A queue-less claim available on both targets: the same atomic status transition, returning a guard
whose `Drop` resets the asset to a re-runnable state rather than re-submitting to a queue it does
not have. `RunClaim` then becomes the queued specialization of a shared primitive rather than the
only implementation.

That would also **subsume the re-entrancy backstop** added by
`specs/design/keyed-recipe-ownership/` — `ImmediateAssetManager::try_enter_inline` and its id set
exist only because there is no claim on this path. A real claim makes them redundant, and one
mechanism is easier to reason about than two.

## Discovery

Noted on 2026-08-09 while designing `specs/design/keyed-recipe-ownership/`. That design needed a
re-entrancy guard for the inline path, considered generalizing `RunClaim`, and deliberately did not
— execute-once is a different change with a wider blast radius than a keyed-recursion fix, and
folding it in would have made the fix unreviewable. Filed so the smaller guard does not quietly
become the permanent answer.

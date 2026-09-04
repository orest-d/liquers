---
id: INLINE-PATH-LACKS-EXECUTE-ONCE
kind: issue
title: The inline run path has no execute-once claim, only an is_finished check
status: closed
priority: P2
complexity: M
area: [core/assets, web]
design: evaluate-path-consolidation
created: 2026-08-09
github:
---

## Resolution

Closed 2026-09-04 by `design/evaluate-path-consolidation/` Step 6. `InlineRunClaim` is the
queue-less counterpart of `RunClaim`: the same atomic status transition under one write lock, the
same explicit status match in which `Dependencies` is active and therefore not claimable, but a
`Drop` repair that restores a re-runnable status rather than re-submitting to a queue the inline
path does not have.

As this issue insisted, a caller that does not win the claim **waits** rather than being refused —
the cheap refusing guard was tried and reverted before, and this records why that was right.

The existing `immediate_concurrent_same_query_runs_once` could not catch the gap because its
command is a synchronous closure that never suspends. The new test uses a command with a real
yield point, and was observed running the body twice before the fix.

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

`specs/design/keyed-recipe-ownership/` tried to get the same protection cheaply and could not — see
below. A real claim is the only thing that covers this path.

## Why the cheap version does not work

`keyed-recipe-ownership` implemented the small guard — a set of asset ids being run inline, refusing
a second entry with `Error::dependency_cycle` — and **reverted it**, because it breaks working
behaviour.

A manager-global id set cannot tell the two cases apart:

- **re-entrancy on one stack**, where an asset's own evaluation asks for itself — a cycle, which
  the guard should refuse; and
- **two tasks legitimately awaiting the same asset**, where the second caller should join the
  first rather than be turned away.

Both look like "this id is already running". The guard refused the second, and
`liquers-web/tests/async_ASYNCQ.rs` failed as a result: a JavaScript `async` command genuinely
yields to the event loop, so a concurrent request arrives while the first evaluation is still in
flight. The native `manager_parametric.rs::immediate_concurrent_same_query_runs_once` did not catch
it, because its command never yields — the first evaluation finishes before the second is polled.

The behaviour the second caller needs is *wait for the running evaluation*, which is what
`run_with_future_inline` already improvises with its `select!` between `wait_to_finish()` and its
own evaluation future. Making that correct rather than improvised **is** this issue. A guard that
only refuses is not a smaller version of it; it is a different and wrong answer.

A real claim must therefore be scoped to the evaluation stack, not to the manager — or, better,
must make the second caller wait, which is what `RunClaim` plus `wait_for_dependency` achieve on the
queued path.

## Discovery

Noted on 2026-08-09 while designing `specs/design/keyed-recipe-ownership/`, and confirmed the hard
way while implementing it. That design considered generalizing `RunClaim` and deliberately did not
— execute-once has a wider blast radius than a keyed-recursion fix, and folding it in would have
made the fix unreviewable. The guard it wrote instead is the evidence above.

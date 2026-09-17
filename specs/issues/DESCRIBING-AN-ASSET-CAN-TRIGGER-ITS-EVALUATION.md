---
id: DESCRIBING-AN-ASSET-CAN-TRIGGER-ITS-EVALUATION
kind: issue
title: Describing an asset can trigger its evaluation
status: draft
priority: P1
complexity: M
area: [core/assets]
design: store-and-asset-search
created: 2026-09-17
github:
---
## Problem

`AssetManager::get_asset_info(&key)` is the call a client makes to find out *what an asset is*. For
a key the manager already holds, it can start evaluating it.

Both implementations take the same route — the trait default at `liquers-core/src/assets.rs:3967`
and `DefaultAssetManager` at `:5334`:

```rust
if self.lookup_key_asset(key).is_some() {      // a non-triggering handle, right here
    let assetref = self.get(key).await?;       // …discarded, and this may submit
    assetref.get_asset_info().await
}
```

`AssetManager::get(&key)` is not a lookup. When the asset's status is not finished and it cannot be
fast-tracked, it ends with `self.job_queue.submit(asset_ref.clone()).await?` (`:5424`). An asset in
`Status::Recipe` — "not ready, but it has a recipe that can be used to create it" (`metadata.rs:304`)
— is precisely such an asset. So describing it runs it.

The other branches are safe: a key that is not live is described from the store, and a key that is
neither live nor stored is described from the recipe provider, both without evaluating. **Only the
already-live branch triggers**, which makes the behaviour depend on whether something else happened
to touch the key earlier — the worst kind of conditional side effect.

There is no non-triggering alternative at the manager level either. `lookup_key_asset` is
non-triggering but is sync, live-only, and documented as a map lookup rather than as the safe path;
`get` and `get_asset` both schedule.

## Is it a defect, or a deliberate trade?

Worth stating, because the fix is a decision rather than a repair. Calling `get()` yields *more
accurate* information: a live asset sitting at `Status::Recipe` may fast-track from the store and
report real metadata, where the cheap path would describe it as `Recipe`. So the current code buys
accuracy with a side effect.

That trade is wrong for a describe operation. A caller that wants the value asks for the value; a
caller that asks what something *is* should not be charged for computing it, and should not be
surprised by a job appearing in the queue. The accuracy argument also fails on its own terms, since
the very next asset described may be evaluated or not depending on nothing the caller controls.

## Impact

Any enumeration over a directory becomes an evaluation of everything in it that has a recipe and
happens to be live. A UI listing a folder, an audit, a metadata export and a search all have the
same exposure, and the cost is unbounded: a recipe may be an arbitrarily expensive pipeline.

It is the central mechanism `design/store-and-asset-search/` depends on. That design's first
invariant is that **a search never evaluates** — a content search reaching unevaluated recipes could
recompute a whole corpus from one query — and the search path is built on exactly this call. Without
a guaranteed non-triggering describe, the invariant is a policy that each caller must implement for
itself by avoiding the manager's own API, which is how it would get quietly broken.

P1 rather than P0: nothing produces a wrong *result*, the effect is a scheduled job rather than
corruption, and a caller can work around it by resolving live→store→recipe by hand. P1 rather than
P2 because it is a correctness risk with a workaround **and** it blocks planned work (§4.4).

## Expected behaviour

**`get_asset_info` describes and never schedules.** For a live key it reports from the handle
`lookup_key_asset` already returned, accepting `Status::Recipe` as the honest answer for an asset
that has not been evaluated.

Beyond the local repair, the contract needs stating, because the gap is really the absence of a
documented non-triggering path:

- A named, documented way to **obtain an asset without starting it** — the capability `get` does not
  provide. `lookup_key_asset` is most of it, and needs to be recognised as the safe path rather than
  an implementation detail.
- The `AssetRef` accessors that are already non-triggering — `status`, `poll_state`,
  `get_any_status`, `poll_binary`, `get_asset_info` — should be identified as such in one place, so a
  caller can tell at a glance which half of the API is safe.
- A statement in `reference/ASSETS.md` of which `AssetManager` operations may schedule work. Today
  that has to be read out of the implementation.

Questions for the design:

- Whether the non-triggering describe is the *default* for `get_asset_info` with an opt-in
  `resolve` variant for callers that do want fast-tracking, or whether the two get separate names.
- Whether a non-triggering acquisition should also cover keys that are not live — returning a
  description assembled from store or recipe without creating an `AssetRef` at all, which is what
  the safe branches already do.
- Whether `listdir_asset_info`, which calls `get_asset_info` per entry, inherits the fix
  automatically (it should) and whether anything else calls `get` for description purposes.

## Discovery

Found while designing `store-and-asset-search`, 2026-09-17, tracing how a search can enumerate a
subtree without evaluating it. Verified at HEAD: `get_asset_info` routes through `get` for live keys
at `assets.rs:3967` and `:5334`; `get` submits to the job queue at `:5424`; `Status::Recipe` is
defined at `metadata.rs:304`; `EvalMode` (`assets.rs:3725`) chooses between queued and inline
execution and offers no "do not execute" mode.

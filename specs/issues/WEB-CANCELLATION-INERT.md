---
id: WEB-CANCELLATION-INERT
kind: issue
title: Web cancellation surface exists but does nothing
status: accepted
priority: P3
complexity: M
area: [web]
design: 
created: 2026-08-08
github:
---
## Problem

On `wasm32` the environment uses `ImmediateAssetManager`, which evaluates the query **during**
`AssetManager::get_asset`. By the time a caller holds the `AssetRef`, the asset has already reached
a terminal status. `liquers-web` exposes `Asset.cancel()` — the cancellation surface required by
`ASYNCQ04` — but it can never do anything: there is nothing in flight to cancel.

Measured, not inferred. A command that takes 300 ms reports status `ready` immediately after
`getAsset` returns, and `cancel()` leaves it `ready`.

```javascript
const asset = await liquers.getAsset("slow");   // 300 ms command
await asset.status();                            // "ready" — already finished
await asset.cancel();                            // resolves, changes nothing
await asset.status();                            // "ready"
```

This follows directly from Phase 1 decision 5 of `specs/liquers-web` ("evaluate immediately; no
heavy long-running background calculations are expected in the browser; some tradeoffs are
acceptable"), so it is a consequence of a deliberate choice rather than a defect. It is filed
because the consequence was not stated at the time and is easy to mistake for a working feature:
`cancel()` returns a resolved `Promise` whether or not it did anything.

## Impact

A page that starts a long evaluation cannot stop it. In practice the browser is also blocked from
the moment the command's own `await` chain stops yielding, so the missing cancellation matters most
for commands that fetch — precisely the ones Phase 1 decision 6 expects to be common.

## Intended solution

A deferred asset manager for `wasm32` that submits the evaluation and returns a handle before
running it, so `get_asset` yields an asset in `Submitted`/`Processing`. `AssetRef::cancel` already
implements the rest; nothing in `liquers-web` changes, which is why `cancel()` is exposed now
rather than withheld — the surface does not move, only the behaviour behind it.

Until then `cancel()` must be documented as a request that may be ignored, which is also its
contract on native for an asset past `Processing`.

## What the fix actually requires — measured, not estimated

The obvious reading is that this needs a task spawner, and that a browser therefore cannot have it.
**That is not the obstacle.** The cancellation window does not need concurrency at all: a caller
does `get_asset()` → *(window)* → `get()`, so it is enough that the evaluation happen at `get()`
rather than at `get_asset()`. No spawning, no runtime.

The single line responsible is in `ImmediateAssetManager::get_asset`
(`liquers-core/src/assets.rs`, in the `loop`):

```rust
assetref.run_inline().await?;
return Ok(assetref);
```

Moving it is nevertheless **not** a one-line change, and this is the part worth knowing before
scoping the work: `AssetRef::get` (`assets.rs:2325`) polls state and then *waits on the
notification channel*. It never starts a run — on native it must not, because a worker owns the
run. Deferring the inline run out of `get_asset` without changing `get` makes `get` wait forever.

So the fix needs a way for an asset to know that **nobody else will run it**, and for `get` to run
it inline in exactly that case. Either:

- a flag on the asset (`self_driven`, set by `ImmediateAssetManager` at creation) that `get`
  consults before entering the wait loop; or
- an `ImmediateAssetManager`-specific handle that owns the "run on first get" behaviour, leaving
  `AssetRef::get` untouched.

The first is smaller and the second is safer. Either touches the shared `AssetRef::get` path or the
`AssetRef` type, and changes *when* evaluation happens in a documented lifecycle
(`specs/reference/ASSETS.md`) — which is why this is a `liquers-designer` task rather than a patch, and why
it was not folded into `specs/liquers-web` M6.

**Blast radius of the semantic change**, for whoever picks it up: `get_asset` currently guarantees
the returned asset is *finished*. Callers are `liquers-core/src/context.rs:287` (nested evaluation
— follows with `get()`, so unaffected), `liquers-web/src/{eval,asset}.rs` (both follow with
`get()`), `liquers-axum/src/assets/{handlers,websocket}.rs` (native, `DefaultAssetManager`, so
untouched by a change confined to the immediate manager), and several `liquers-core` asset tests
that inspect status straight after `get_asset` — those tests are the ones that would need
revisiting, and they are the ones worth reading first, because they encode the current contract.

## Where it is recorded

- `liquers-web/src/asset.rs` — module documentation, "Limitation: cancellation is inert in this
  phase".
- `liquers-web/tests/eval_EVAL.rs` — `eval06_cancellation_has_defined_terminal_result` asserts the
  inert behaviour **deterministically**. It will fail the day a deferred manager lands, which is
  the intent: the test is the tripwire, not an accommodation.

## Discovery

Found while writing the `EVAL`/`ASYNCQ`/`ASYNCCMD` suites for `specs/liquers-web` milestone M4. The
first version of all three cancellation tests matched on two outcomes — "either it was cancelled or
it had already finished" — and passed. That form of assertion passes regardless of what the
implementation does, and would have kept passing if `cancel()` began throwing or hanging. Probing
which branch actually ran is what surfaced this. It is a concrete instance of the risk Phase 3
named: *conformance tests written to pass rather than to catch*.

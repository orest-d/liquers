---
id: CORE-IMMEDIATE-MANAGER-KEYED-RECURSION
kind: issue
title: Keyed evaluation recurses forever under ImmediateAssetManager, crashing wasm
status: closed
priority: P1
complexity: M
area: [core/assets, web]
design: keyed-recipe-ownership
created: 2026-08-09
github:
---

## Problem

Evaluating any keyed (`-R/`) query under `ImmediateAssetManager` — the manager `wasm32` selects —
recurses until the stack is exhausted. In a browser that surfaces as
`RuntimeError: memory access out of bounds`, after which the wasm instance is unusable and the
`Promise` never settles: a **hang**, with the error only visible as a `pageerror`.

The cycle, read from the browser stack trace:

1. `ImmediateAssetManager::get(key)` (`liquers-core/src/assets.rs:5616`) fetches or creates the
   asset and, when it is not finished, calls `asset_ref.run_inline()` (`:5645`).
2. `run_inline` → `run_with_future_inline(self.evaluate_and_store())` (`:1800`).
3. `evaluate_and_store` → `evaluate_recipe` (`:1826`).
4. `evaluate_recipe` resolves the recipe for a keyed asset and, to establish whether *it* is the
   asset registered for that key, calls `manager.get(&key).await?` (`:1833`) — **its own key**.
5. That asset is mid-evaluation and therefore not finished, so step 1 runs `run_inline` on it
   again. Go to 2.

The identity guard `if asset.id() == self.id()` sits on the line *after* the `get` (`:1834`), so it
never runs — the recursion happens inside the call it is guarding.

Native builds use the queued manager, whose `get` does not evaluate inline, so nothing on the
native test loop exercises this. It is reachable only on `wasm32`, which is why it has gone
unnoticed.

## Impact

**`-R/` does not work in the browser at all.** Not "works slowly" or "fails with an error" — the
wasm instance dies and the caller's `Promise` hangs, which is the least diagnosable failure mode
available. Every browser feature that addresses a resource by key is blocked: `specs/design/liquers-web-store/`
delivers four working stores and the store surface reaches them correctly, but a query that goes
through the asset manager cannot.

No workaround at the integration level. `liquers-web` can read a store directly
(`env.store().get(key)`), and that path is verified, but `env.evaluate("-R/…")` cannot be made to
work without changing the manager.

## Expected behaviour

`evaluate_recipe` establishes whether the asset registered for a key is itself, without triggering
an evaluation of that key.

The obvious candidate is `AssetManager::lookup_key_asset` (`liquers-core/src/assets.rs:5668`),
which is a non-evaluating map lookup and already exists on the trait. The asset is inserted into
the map before `run_inline` is called, so the lookup should find it and the identity comparison is
unchanged. That would be a one-line change, but it alters a path shared with the queued manager and
belongs to somebody with the asset subsystem in their head:

1. `lookup_key_asset` instead of `get` in `evaluate_recipe` — minimal, but needs checking against
   the queued manager's insertion ordering.
2. A re-entrancy guard in `ImmediateAssetManager::get` — refuse to `run_inline` an asset already
   being evaluated on this stack. Broader, and would also catch other cycles.
3. Make the identity check unnecessary: pass down whether this asset *is* the key's asset, which is
   known by the caller.

Whichever is taken, a wasm test evaluating a keyed query is the regression guard, and none exists
today — that absence is why this survived.

## Discovery

Found on 2026-08-09 during M6 of `specs/design/liquers-web-store/`: the browser end-to-end tests
for `-R/` timed out where direct store access succeeded. Diagnosed from the page's stack trace
under Chromium; the cycle above is read from that trace, not inferred. Not fixed there because the
fix is in `core/assets` and affects the native manager's path too.

A second, separate defect sits on the same path and was worked around rather than fixed —
`LIB-RECIPE-PROVIDER-PANIC`.

## Resolution

Fixed by `specs/design/keyed-recipe-ownership/`. `AssetRef::evaluate_recipe` now asks
`AssetManager::owned_key_asset` — a map read that never evaluates — instead of
`AssetManager::get`. Option 1 from *Expected behaviour*, refined: the lookup is volatility-aware,
because the manager never registers a volatile key and the raw map can still hold a stale entry
for one.

Option 2 was taken as well, as a backstop rather than the mechanism: `ImmediateAssetManager` tracks
the ids it is running inline and returns `Error::dependency_cycle` rather than recursing, so a
future bypass of the ownership test is a diagnosable error instead of a dead wasm instance.

The regression guard the issue asks for exists on two levels:
`liquers-core/tests/manager_parametric.rs::keyed_eval_immediate` (native, and verified to abort
with `stack overflow` when the fix is reverted) and `EVAL07` under `wasm-bindgen-test`.

**`-R/` in the browser is not unblocked by this alone.** The five `STORE07`/`STORE11` Playwright
cases were enabled and then re-marked `fixme`: with the recursion gone they fail with
`No recipe found`, because `ImmediateAssetManager::get` has no `try_fast_track` step and every key
in those tests names a plain stored file rather than a recipe. That is
`IMMEDIATE-MANAGER-NO-FAST-TRACK` (P1), found by this work and filed separately. The *Impact*
section above — "`-R/` does not work in the browser at all" — therefore remains true for the
plain-resource case; what changed is that it now reports a typed error instead of killing the wasm
instance and hanging the caller.

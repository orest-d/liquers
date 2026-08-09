---
id: IMMEDIATE-MANAGER-NO-FAST-TRACK
kind: issue
title: ImmediateAssetManager never fast-tracks, so a stored file cannot be read by key on wasm
status: draft
priority: P1
complexity: M
area: [core/assets, web]
design:
created: 2026-08-09
github:
---

## Problem

`DefaultAssetManager::get` tries the store before evaluating: an asset that is not finished gets a
`try_fast_track()` attempt, which loads the stored bytes and metadata when the key already exists
with a `Ready`, `Source` or `Override` status (`liquers-core/src/assets.rs:4536`).

`ImmediateAssetManager::get` has no such step. It resolves the asset and, if unfinished, runs it
inline:

```rust
if status.is_finished() { … return Ok(asset_ref); }
asset_ref.run_inline().await?;
```

`run_inline` → `evaluate_and_store` → `evaluate_recipe`, and for a keyed asset that means asking
the recipe provider. A key naming a **plain stored file** has no recipe, so
`DefaultRecipeProvider::recipe` returns `No recipe found for key <key>`
(`liquers-core/src/recipes.rs:606-612`) and the evaluation fails.

So on wasm — where `ImmediateAssetManager` is the only manager — `-R/<key>` works for a key that
has a recipe and fails for a key that is simply a file in the store.

## Impact

**Reading a stored resource by key does not work in the browser.** That is the ordinary case: a
fetch store serving fixture files, a `localStorage` store holding user data, a JavaScript store
wrapping an existing object. None of them ships a `recipes.yaml`, and none of them can be reached
through `env.evaluate('-R/…')`.

The five `STORE07`/`STORE11` cases in `liquers-web/tests/e2e/store.spec.ts` are exactly this
scenario. They were `fixme`-marked for `CORE-IMMEDIATE-MANAGER-KEYED-RECURSION`; that defect is
fixed, and they are still blocked by this one. The failure mode is much better — a typed
`No recipe found` instead of a wasm stack overflow and an unsettled `Promise` — but the feature is
still unavailable.

Native builds are unaffected: the queued manager fast-tracks.

## Expected behaviour

`ImmediateAssetManager::get` attempts `try_fast_track()` before running an asset inline, as the
queued manager does. The primitive is already shared — `AssetData::try_fast_track` is not
manager-specific and carries its own status gate — so this is a matter of the inline `get` calling
it, not of new machinery.

Worth checking while doing it: the same asymmetry may exist in `get_asset`'s query branch and in
`get_dependency_asset`, and the two managers' `get` bodies have drifted enough that a shared helper
may be the honest fix.

## Verification

1. A key naming a plain stored file evaluates through `-R/<key>` under `ImmediateEnvironment` on
   native — a parametric scenario in `liquers-core/tests/manager_parametric.rs`, which currently
   has keyed coverage only for recipe-backed keys.
2. Remove `fixme` from the five `STORE07`/`STORE11` cases in `liquers-web/tests/e2e/store.spec.ts`.
3. `liquers-web/tests/eval_EVAL.rs::eval07_keyed_query_evaluates` can then drop its recipe and read
   a plain file, which is what it was originally written to do.

## Discovery

Found on 2026-08-09 while implementing `specs/design/keyed-recipe-ownership/`. The wasm regression
test for that design was written to evaluate a plain stored file and failed with
`No recipe found for key d/f.txt` — no longer a crash, which is how the second defect became
visible. The test now goes through a recipe and passes; this issue carries the remainder.

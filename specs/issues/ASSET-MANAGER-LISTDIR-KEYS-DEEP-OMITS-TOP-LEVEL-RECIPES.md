---
id: ASSET-MANAGER-LISTDIR-KEYS-DEEP-OMITS-TOP-LEVEL-RECIPES
kind: issue
title: AssetManager::listdir_keys_deep omits recipe-declared keys of the listed directory itself
status: draft
priority: P2
complexity: S
area: [core/assets]
design: recipe-provider-listing-contract
created: 2026-09-28
github:
---

## Problem

`AssetManager::listdir_keys_deep` (trait default, `liquers-core/src/assets.rs` ≈4166) starts from
`store.listdir_keys_deep(key)`, then adds recipe-declared keys only for the **subdirectories** it
found (`for subkey in folders { … assets_with_recipes(&subkey, …) }`). The recipes declared in
`key/recipes.yaml` itself are never added. `AssetManager::listdir` (≈4108) does add them, so a
shallow and a deep listing of the same directory disagree about its direct children, and
`AssetManager::keys()` (`listdir_keys_deep(&Key::new())`) misses every root-level recipe.

A subdirectory that exists only through recipes (no stored key under it) is not found either,
because the walk only descends into directories the store reports.

## Impact

Anything that wants the key set of a subtree through the manager misses recipe-only keys at the
top of it: `GET /api/assets/key/listdir/{dir}?deep=true` (`axum-assets-endpoints`), the agent
memory corpus index, and `keys()`. The workaround is to combine `listdir` with the deep listing.

## Expected behaviour

The deep listing contains every key the shallow listing contains, plus the same for each
subdirectory, including recipe-declared entries at every level. Either start the recipe pass from
`key` itself, or build the deep listing by recursing over `AssetManager::listdir_keys`.

## Discovery

Final cross-phase review of `specs/design/axum-assets-endpoints/` (2026-09-28), while checking
what `removedir` walks.

## Design (2026-10-05)

Owned by [`design/recipe-provider-listing-contract/`](../design/recipe-provider-listing-contract/DESIGN.md) (Part C), which merged this issue's former
design `listdir-keys-deep-recipe-union` (now `superseded`) with the designs of `RECIPE-CONTAINS-DEFAULT-ASSUMES-ENUMERABILITY`,
`MANIFEST-PROVIDER-FOLDER-LISTING-NEVER-REFRESHES` and `ASSET-MANAGER-LISTDIR-KEYS-DEEP-OMITS-TOP-LEVEL-RECIPES`,
because their implementations depend on each other. The three issues are resolved by one implementation.

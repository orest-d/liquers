# Phase 1: High-Level Design - Deep Listing Includes Recipe Keys at Every Level

> **Superseded on 2026-10-05** by [`recipe-provider-listing-contract`](../recipe-provider-listing-contract/DESIGN.md), Part C (complete deep listings). The maintainer
> merged this design with the two others it depended on after the post-Phase-4 review; the merged
> design carries this content with the review's corrections and owns this design's source issue.
> This folder is kept for the reasoning; do not implement from it.

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** The issue's own invariant — a deep listing contains the shallow listing at
  every level — fixes the contract; the asset manager already has a correct shallow
  `listdir`, so building the deep listing from it is a local change in one trait default plus
  the removal of an identical override.
- **Open questions:** None

## Problem and Evidence

`AssetManager::listdir_keys_deep` (trait default, `liquers-core/src/assets.rs` ≈5300) and an
identical override in `impl AssetManager<E> for DefaultAssetManager<E>` (≈7517) start from
`store.listdir_keys_deep(key)`, collect the stored **subdirectories**, and add
`assets_with_recipes` only for those. Recipes declared for `key` itself are never added, while
`AssetManager::listdir` (≈5234 / ≈7492) unions recipes and store for `key`. So:

- `listdir(key)` ⊄ `listdir_keys_deep(key)` for a folder with a `recipes.yaml`;
- `AssetManager::keys()` (= `listdir_keys_deep(&Key::new())`) misses every root-level recipe;
- a directory that exists only through a recipe provider (no store entry) is never descended.

Consumers: `GET /api/assets/key/listdir/{dir}?deep=true` (`liquers-axum/src/assets/
key_handlers.rs` ≈175), `AssetManager::keys()`, and `AssetManager::removedir` (≈5126), which
walks this listing to `remove` every key under a directory.

## Expected Behaviour and Acceptance Criteria

1. For every directory `d` under (and including) `key`, every key of `listdir_keys(d)` is in
   `listdir_keys_deep(key)`.
2. `listdir_keys_deep(key)` contains no key outside `key`'s subtree and does not contain `key`.
3. Result is sorted and duplicate-free (unchanged).
4. `keys()` includes root-level recipe keys.
5. A directory name reported only by the recipe provider is descended when the provider has
   recipes for it.
6. Both managers (`DefaultAssetManager`, `ImmediateAssetManager`) behave identically.

## Affected Users, Workflows and Systems

HTTP deep listing clients, `keys()` users (agent-memory corpus index), and `removedir`.
Stores, recipe providers and query semantics are unchanged.

## Scope and Non-Goals

In scope: the deep listing algorithm and removing the duplicated override. Non-goals:
changing `listdir`, recipe providers, store `listdir_keys_deep`, or `removedir`'s algorithm.

## Compatibility and Behaviour Changes

- Deep listings grow by the previously missing recipe keys — the fix itself.
- `removedir(d)` now also passes `d`'s own recipe keys to `remove`. For a recipe-only key
  `remove` returns `Ok(())` with `RemoveAction::Nothing`, but **a live asset under that key is
  cancelled and unmapped** — the same treatment subdirectory recipe keys already get. Removing a
  directory and leaving its recipes' live assets mapped was the inconsistent state; this is
  intended.
- Cost: one `AssetManager::listdir` per directory instead of one store deep listing plus an
  `is_dir` per key. Comparable; no hot path (not used by evaluation).

## Documentation Assessment

`specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` / `reference/ASSETS.md`: review for a
statement of what `listdir_keys_deep` returns and add the invariant if one exists. Close the issue.

## Design Dependencies

- `overlaps` `axum-assets-endpoints`: its `?deep=true` endpoint is a consumer; its tests should
  stay green and gain nothing to change.
- `overlaps` `listdir-keys-deep-child-check` (covered): the *store* default's recursion guard;
  unrelated code path, same name.

## Consolidated Findings

- Build the deep listing by recursion over `self.listdir`, which makes criterion 1 true by
  construction rather than by a parallel algorithm that can drift again.
- Descend into a child when `store.is_dir(child)`; for a name that came **only** from the
  recipe provider (not in `store.listdir`), also descend when `has_recipes(child)` — this covers
  criterion 5 without a `has_recipes` call per stored key. With the shipped
  `DefaultRecipeProvider`, recipes imply a stored `recipes.yaml`, so this branch only matters
  for custom providers.
- Delete the `DefaultAssetManager` override (identical body) so both managers share one
  implementation — criterion 6 by construction.
- `removedir` behaviour change above is intended and must be tested (live recipe asset at the
  directory's own level is unmapped).
- Validation: new unit tests on both managers plus existing removedir and axum listing tests.

## Review

Coherent and local; the only cross-cutting effect (removedir) is consistent with existing
subdirectory behaviour.

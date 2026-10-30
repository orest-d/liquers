---
id: RECIPE-RESULT-UNDER-STORAGE-NONE-READS-AS-SOURCE
kind: issue
title: A recipe result stored in a storage-none mount reads back as Source and cannot drop back to its recipe
status: closed
priority: P3
complexity: M
area: [core/store, core/assets]
design: storage-none-computed-values
created: 2026-10-19
github:
---
# A recipe result stored in a storage-none mount reads back as Source and cannot drop back to its recipe

## Problem

A mount with `metadata: { storage: none }` keeps no metadata record: every read re-derives it from
the data. A recipe's computed result written into such a mount is therefore indistinguishable from
a hand-made file once its live asset is gone: its status reads back as `Source`. Removing it with
`AssetManager::remove` then takes the `Source + recipe` branch (delete the user value) instead of
`DropComputed` (rewrite as `Recipe`), and a later listing shows a user value where a recipe was.

Found while testing `agent-memory-mvp` (`recipe_results_drop_back_to_recipe`): with the result in
a `storage: none` mount, `get_asset_info` after `remove` reported `Source`; the test now uses a
mount with stored metadata, where the result drops back to `Recipe` as the migration guide says.

## Impact

- The migration guide's rule that recipe assets are safe to recompute holds only for mounts that
  store metadata. In a `storage: none` checkout mount, a recipe result looks like a document.
- Workaround: keep recipe outputs out of `storage: none` mounts, or give such a mount a metadata
  storage.

## Expected behaviour

Either a `storage: none` mount refuses to store a computed (`Ready`) value, or the pipeline can
derive `Recipe`/`Ready` for a key the recipe provider can make, so a computed result is not taken
for a user value.

## Reproduction at HEAD (bulk design, 2026-10-20)

Persists at 18732dd by code reading: `NoStorage` (`liquers-core/src/store/storage.rs`) keeps no
record, `AssetRef::save_to_store` writes a `Ready` result without asking whether the mount can keep
its status, and `AssetManager::remove` decides from the synthesized `Source`. One more consequence
than filed: with no stored dependency records, a later process fast-tracks the old result as a
`Source` even after its inputs changed. Found while testing `design/agent-memory-mvp/` (complete),
which this issue no longer links as its design. Designed in `design/storage-none-computed-values/`
(recommended: do not write computed values into a mount that keeps no metadata).

## Resolution

Fixed by `specs/design/storage-none-computed-values/` (PR #173). A record the store builds rather
than reads is now flagged `inferred` (never stored). At a key a recipe can make, the asset manager
reads data under an inferred record as a file with no metadata, so the environment's
`external_change` policy decides between `Override` and recomputation, and `remove` deletes the
data and drops the key back to its recipe (`get_asset_info` reports `Recipe`). The original
scenario is `recipe_results_drop_back_to_recipe_in_a_storage_none_mount` in
`liquers-lib/tests/mem_commands.rs`.

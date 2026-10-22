# Phase 4: Implementation Plan — One call reconciles a running asset manager with its store

## Overview

Hook first, because it is independent and crosses crates. Then the enumeration, the new step, and
the composed operation. Then the tests and the documents. Phase 1 decisions 1-3 are assumed as
recommended. Nothing must land before this design.

## Progress

- [ ] Step 1: Recipe-provider invalidation hook
- [ ] Step 2: registered_keys on both managers
- [ ] Step 3: StoreSyncReport and synchronize_with_store
- [ ] Step 4: Integration tests
- [ ] Step 5: Documentation and issue closure

## Implementation Steps

### Step 1: Recipe-provider invalidation hook
- Files / symbols: `liquers-core/src/recipes.rs` `AsyncRecipeProvider::invalidate_caches`
  (provided, no-op), `RecipeProviderChain` override (forward to each member);
  `liquers-records/src/provider.rs` `ManifestRecipeProvider` override (calls `clear_cache`).
- Depends on: none.
- Proof: `chain_forwards_invalidate_caches`, `invalidate_caches_clears_manifests`.
- Rollback: revert; nothing calls it yet.

### Step 2: registered_keys on both managers
- Files / symbols: `liquers-core/src/assets.rs` `AssetManager::registered_keys` (provided, empty),
  overrides in `DefaultAssetManager` (`scc::HashMap::scan`) and `ImmediateAssetManager` (mutex
  snapshot). If `asset-garbage-collection` has landed, read through its `Registration` wrapper
  without touching the last-use stamp.
- Depends on: none (parallel with step 1).
- Proof: a unit test in `assets.rs`, `registered_keys_lists_the_key_map`.
- Rollback: revert.

### Step 3: StoreSyncReport and synchronize_with_store
- Files / symbols: `assets.rs` `StoreSyncReport`; provided `AssetManager::synchronize_with_store`;
  private `reconcile_registrations`; a private helper `expire_changed_commands(keys)` factored out of
  `refresh_command_versions_and_expire`, which keeps its behaviour by calling it.
- Change: the seven steps of Phase 2, in that order, with the `Expire`-only steps skipped in
  `ReportOnly`.
- Depends on: steps 1-2.
- Proof: `cargo test -p liquers-core --lib --tests` (existing refresh tests unchanged).
- Rollback: revert.

### Step 4: Integration tests
- Files / symbols: `liquers-core/tests/store_sync.rs`, `sync01`-`sync07`.
- Depends on: step 3.
- Proof: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test store_sync`.
- Rollback: delete the file.

### Step 5: Documentation and issue closure
- Files: `guides/DEPENDENCY_CONSISTENCY_GUIDE.md`, `reference/ASSETS.md`,
  `reference/DEPENDENCIES_STATUS.md` (History rows, `reviewed:` bumps); close
  `ASSET-MANAGER-CANNOT-BE-SYNCHRONIZED-WITH-THE-STORE` with a resolution note.
- Proof: `python3 scripts/docs_index.py --check`.

## Testing Plan

Steps 1-3 end with `cargo test -p liquers-core --lib --tests` and, for step 1,
`cargo test -p liquers-records --lib --tests`. Final: `cargo test -p liquers-lib --lib --tests`
(its environments use the manifest provider through the chain) and `bash scripts/check-build-matrix.sh`
(the trait gains methods used by the wasm32 build).

## Rollback Plan

Revert in reverse order. Each step is additive: a partial landing leaves unused provided methods
and no behaviour change.

## Documentation Updates

The three `affects_docs` documents; `specs/index.csv` regenerated. No
`specs/command_registry.yaml` change.

## Phase 5 Entry Criteria

None — four-phase design.

# Phase 4: Implementation Plan — Prune stored derived values

## Overview

Prerequisite: `AssetManager::remove_computed` from `assets-api-admin-operations` (its Phase 4 core
step; the HTTP routes are not needed). Decisions 1-3 of Phase 1 assumed as recommended. Then
types, classifier, the provided method, tests, documents.

## Progress

- [ ] Step 1: `remove_computed` available in core
- [ ] Step 2: Policy and report types
- [ ] Step 3: Classifier
- [ ] Step 4: `prune_store` provided method
- [ ] Step 5: Integration tests
- [ ] Step 6: Documentation and issue closure

## Implementation Steps

### Step 1: `remove_computed` available in core
- Files / symbols: `liquers-core/src/assets.rs` `AssetManager::remove_computed`.
- Change: none if `assets-api-admin-operations` has landed; otherwise implement exactly its
  Phase 2 §Interfaces (`RemoveGuard::ComputedOnly` inside `remove`'s decision table) and its core
  tests, and leave its routes to that design.
- Depends on: none.
- Proof: that design's core tests (`remove_computed_*`).
- Rollback: revert.

### Step 2: Policy and report types
- Files / symbols: `assets.rs` `PrunePolicy` (+ builder methods), `PruneKeepReason`,
  `PruneReport`; re-exported where `GcReport` would be.
- Depends on: none.
- Proof: `cargo test -p liquers-core --lib` (serde round trip of `PrunePolicy` in a unit test).
- Rollback: revert.

### Step 3: Classifier
- Files / symbols: `assets.rs` private `async fn classify_for_prune<E>(manager, store, key, now,
  older_than) -> PruneClass` (`Prune`, `Orphan`, `Keep(PruneKeepReason)`), with an explicit
  `match` over `Status` and no `_` arm.
- Depends on: step 2.
- Proof: `prune_classify_is_exhaustive`.
- Rollback: revert.

### Step 4: `prune_store` provided method
- Files / symbols: `AssetManager::prune_store` (provided, in the trait in `assets.rs`).
- Change: Phase 2 §Integration Points 1-4.
- Depends on: steps 1-3.
- Proof: `cargo test -p liquers-core --lib --tests` (no existing test changes).
- Rollback: revert; nothing calls it.

### Step 5: Integration tests
- Files: `liquers-core/tests/store_pruning.rs` (Phase 3 list); `age_record` helper there.
- Depends on: step 4.
- Proof: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test store_pruning`.
- Rollback: delete the file.

### Step 6: Documentation and issue closure
- Files: `specs/reference/ASSETS.md`, `specs/reference/STORE_SEMANTICS.md` §5,
  `specs/guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md` (History row and `reviewed:` bump each); close
  `STORED-DERIVED-VALUES-ARE-NEVER-PRUNED` with a resolution note, and file follow-ups for a
  scheduler / command / admin route if wanted.
- Proof: `python3 scripts/docs_index.py --check`.

## Testing Plan

Each step ends with `cargo test -p liquers-core --lib --tests`. Final: `cargo test -p liquers-lib
--lib --tests` (no change expected), `bash scripts/check-build-matrix.sh` (wasm32 rows: the method
is provided and uses no task or timer), and after `cargo clean` the `liquers-web` Node loop is not
needed (no `liquers-web` change).

## Rollback Plan

Revert in reverse order. Steps 2-4 are additive and unused until called; step 1 is shared with
`assets-api-admin-operations` and is reverted only with it.

## Documentation Updates

`reference/ASSETS.md`, `reference/STORE_SEMANTICS.md`, `guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md`
(History, `reviewed:`); `specs/index.csv` regenerated; no `command_registry.yaml` change.

## Phase 5 Entry Criteria

None — four-phase design.

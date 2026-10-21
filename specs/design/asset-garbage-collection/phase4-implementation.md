# Phase 4: Implementation Plan — Bounded asset cache with reachability-based collection

## Overview

First the map wrapper (no behaviour change), then the limits and the pass, then the triggers, then
the documents. Prerequisite: `asset-registration-contract` states R2 and R4 (documentation only;
it may land in the same PR series). Decisions 1-4 of Phase 1 are assumed as recommended.

## Progress

- [ ] Step 1: Registration wrapper in both managers
- [ ] Step 2: CacheLimits and options
- [ ] Step 3: collect_garbage pass
- [ ] Step 4: Triggers and collector task
- [ ] Step 5: Integration tests
- [ ] Step 6: Documentation and issue closure

## Implementation Steps

### Step 1: Registration wrapper in both managers
- Files / symbols: `liquers-core/src/assets.rs` `DefaultAssetManager`, `ImmediateAssetManager`
  (map fields and every read/insert/remove site), new private `Registration<E>` with `new`, `touch`,
  `stamp`; test helper `register_for_test` replacing direct `insert_async` in the `assets.rs` tests.
- Change: map value type; hits call `touch()`.
- Depends on: none.
- Proof: `cargo test -p liquers-core --lib --tests` (no behaviour change).
- Rollback: revert the commit.

### Step 2: CacheLimits and options
- Files / symbols: `environment_builder.rs` `AssetManagerOptions` (three fields, `with_*`, the zero
  check in the build validation that rejects `job_capacity: 0`); `assets.rs` `CacheLimits`,
  `AssetManager::cache_limits` (provided), both managers' `with_policies` and override.
- Depends on: none (parallel with step 1).
- Proof: `cache_limits_round_trip_and_zero_rejected`.
- Rollback: revert.

### Step 3: collect_garbage pass
- Files / symbols: `assets.rs` `GcReport`, `AssetManager::collect_garbage` (provided, empty),
  overrides in both managers, private `collectable_snapshot`, `collectable_status` (an explicit
  `match` over `Status`), the removal predicate; `untrack_expiration` after a queued key removal.
- Depends on: steps 1-2.
- Proof: `gc_removal_rechecks_reachability`, then gc01, gc02, gc05-gc08 with explicit calls.
- Rollback: revert; the wrapper stays harmless.

### Step 4: Triggers and collector task
- Files / symbols: `assets.rs` over-limit check after successful inserts; `CollectorMessage`,
  `DefaultAssetManager::run_cache_collector` (native only, `#[cfg(not(target_arch = "wasm32"))]`)
  spawned in `start` when limits are set; `Shutdown` from `Drop` and `shutdown()`; the inline
  manager awaits `collect_garbage` after an over-limit insert.
- Depends on: step 3.
- Proof: gc03, gc04; `bash scripts/check-build-matrix.sh` (wasm32 rows).
- Rollback: revert; explicit collection keeps working.

### Step 5: Integration tests
- Files / symbols: `liquers-core/tests/asset_gc.rs` (all of Phase 3's list).
- Depends on: steps 3-4 (write them first and watch them fail if preferred).
- Proof: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test asset_gc`.
- Rollback: delete the file.

### Step 6: Documentation and issue closure
- Files: the four `affects_docs` documents (History row, `reviewed:` bump each). Close `CORE-ASSET-GC`
  with a resolution note that names the two follow-up issues for the remaining scope (byte budget,
  store pruning), per `DOCS_STRUCTURE_GUIDE.md` §5.6.
- Proof: `python3 scripts/docs_index.py --check`.

## Testing Plan

Steps 1-4 each end with `cargo test -p liquers-core --lib --tests`. Final:
`cargo test -p liquers-lib --lib --tests` (the lib's environments use the same managers with no
limits), `bash scripts/check-build-matrix.sh` (no new feature, but a new `cfg` on the task), and
after `cargo clean` the `liquers-web` Node loop (`--features debug-handles`), because the inline
manager's maps change.

## Rollback Plan

Revert in reverse order. Steps 1-2 alone change nothing observable. Without step 4, collection
happens only when called explicitly, which is a coherent intermediate state.

## Documentation Updates

`reference/ASSETS.md`, `reference/ENVIRONMENT_CONFIG.md`, `guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md`,
`guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` (History, `reviewed:`); `specs/index.csv`
regenerated; no `command_registry.yaml` change.

## Phase 5 Entry Criteria

None — four-phase design.

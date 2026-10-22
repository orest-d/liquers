# Phase 4: Implementation Plan — Manager status and a guarded removal

## Overview

Core first (refactor `remove` behind a helper with no behaviour change, then the guarded method),
then the HTTP surface, then documents. Prerequisite: Q1 decided (the plan uses `remove_computed`).

## Progress

- [ ] Step 1: move `remove`'s body into `remove_keyed` with `RemoveGuard::Any`
- [ ] Step 2: `AssetManager::remove_computed` and its core tests
- [ ] Step 3: Assets API routes, handlers and result type
- [ ] Step 4: axum tests and route-presence rows
- [ ] Step 5: documents and issue closure

## Implementation Steps

### Step 1: move `remove`'s body
- Files / symbols: `liquers-core/src/assets.rs` `AssetManager::remove`; new private `RemoveGuard`,
  `remove_keyed`.
- Change: body moved verbatim; `remove` delegates with `RemoveGuard::Any`.
- Depends on: none.
- Proof: `cargo test -p liquers-core --test asset_manager_remove_expire_describe` and
  `cargo test -p liquers-core --lib` unchanged and green.
- Rollback: revert the commit (pure refactor).

### Step 2: `remove_computed`
- Files / symbols: `assets.rs` `remove_keyed` (guard check after the `action` is decided, before
  the live asset is cancelled), `AssetManager::remove_computed` with doc table;
  `liquers-core/tests/asset_manager_remove_expire_describe.rs` `amr70`-`amr74`.
- Change: Phase 2 Interfaces; refused status per Phase 2 Error Handling.
- Depends on: Step 1.
- Proof: `cargo test -p liquers-core --test asset_manager_remove_expire_describe amr7` (Tests 3-7).
- Rollback: revert; Step 1 stays.

### Step 3: HTTP surface
- Files / symbols: `liquers-axum/src/assets/common.rs` `ManagerInfo`; `key_handlers.rs`
  `key_remove_computed_handler`, `manager_info_handler` (exhaustive `match` on `EvalMode`);
  `builder.rs` `build` and the module doc's route-presence rule.
- Change: Phase 2 Integration Points.
- Depends on: Step 2.
- Proof: `cargo test -p liquers-axum --lib` (existing route-presence test still green before its
  extension).
- Rollback: revert; core keeps an unused method.

### Step 4: axum tests
- Files / symbols: `liquers-axum/tests/assets_api_endpoints.rs` `aae93`-`aae96`;
  `builder.rs` `route_presence_for_every_switch_combination` rows.
- Change: Phase 3 Test Plan.
- Depends on: Step 3.
- Proof: `cargo test -p liquers-axum --lib --tests` (Tests 8-12).
- Rollback: n/a (tests).

### Step 5: documents and issue
- Files / symbols: `WEB_API_SPECIFICATION.md` §5.4, §5.5, §5.6; `ASSETS.md` (`Removed` senders);
  `WEB_API_GUIDE.md` §7; `specs/issues/ASSETS-API-ADMIN-OPERATIONS.md`.
- Change: Phase 2 Documentation Architecture; History rows and `reviewed:` bumps; issue `closed`
  with the commits as evidence.
- Depends on: Step 4.
- Proof: `python3 scripts/docs_index.py --check`.
- Rollback: documents only.

## Testing Plan

Core tests after Steps 1-2; axum tests after Steps 3-4; finally `cargo test -p liquers-lib --lib
--tests` (it calls `remove` through `ns-mem`) and `cargo clippy -p liquers-core -p liquers-axum
--lib`. No `cfg(feature)` change, so no build matrix.

## Rollback Plan

Step 1 is a no-behaviour refactor and can stay alone. Reverting Steps 3-4 removes the HTTP
surface; reverting Step 2 removes the method. Nothing is persisted differently, so no data
migration either way.

## Documentation Updates

As Step 5. `specs/index.csv` regenerated. No command registry change (no command touched).
`specs/README.md` needs no new link (the Assets API entry already points at the specification).

## Phase 5 Entry Criteria

None — four-phase design.

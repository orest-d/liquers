# Phase 4: Implementation Plan — Asset expiration events observable by scope

## Overview

Types and the hub first (no behaviour change), then the three emission sites, then the
subscription, then the tests and the documents. Nothing must land before this design. Phase 1
decisions 1-5 are assumed as recommended. This plan does not cover the reconciliation layer
(`EXTERNAL-INDEX-HAS-NO-RECONCILIATION-CONTRACT`).

## Progress

- [ ] Step 1: Event types and the hub in DependencyManager
- [ ] Step 2: Emission at the three record_expiry sites
- [ ] Step 3: Subscription with scope and lag
- [ ] Step 4: Integration tests
- [ ] Step 5: Documentation and issue closure

## Implementation Steps

### Step 1: Event types and the hub in DependencyManager
- Files / symbols: `liquers-core/src/assets.rs` `AssetEvent`, `AssetEventSubject`,
  `AssetEventScope`, `AssetEventReceive`; `liquers-core/src/dependencies.rs` `DependencyManager`
  field `events`, `ASSET_EVENT_BUFFER`, `subscribe_events`, `announce`.
- Change: as Phase 2. `announce` ignores `SendError` (no receivers).
- Depends on: none.
- Proof: unit test in `dependencies.rs`, `announce_without_subscribers_is_silent`, and
  `announce_reaches_subscriber`.
- Rollback: revert.

### Step 2: Emission at the three record_expiry sites
- Files / symbols: `assets.rs` `AssetData::record_expiry` (subject from `key`, else `query`, else
  skip), `expire_stored_copy` (after a successful `set_metadata`), the stored branch of the provided
  `AssetManager::expire`; a private helper `expiry_event(subject, reason, metadata, live)`.
- Depends on: step 1.
- Proof: `cargo test -p liquers-core --lib --tests` (no assertion changes; no subscriber exists).
- Rollback: revert.

### Step 3: Subscription with scope and lag
- Files / symbols: `assets.rs` `AssetEventSubscription` (`new`, `recv`, `try_recv`, `scope`; a
  scope match `Under(k)` = `Key(s)` with `s.has_key_prefix(k)`, `Query` only under `All`);
  provided `AssetManager::subscribe_asset_events`; re-exports beside `AssetNotificationMessage`.
  `RecvError::Lagged(n)` → `Lagged(n)`, `RecvError::Closed` → `Closed`.
- Depends on: steps 1-2.
- Proof: unit test `subscription_filters_by_scope_and_reports_lag` in `assets.rs`.
- Rollback: revert.

### Step 4: Integration tests
- Files / symbols: `liquers-core/tests/asset_events.rs` (events01-events08);
  `liquers-core/tests/external_asset_manager.rs` `external_manager_announces_expiry`.
- Depends on: step 3.
- Proof: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test asset_events --test
  external_asset_manager`.
- Rollback: delete the tests.

### Step 5: Documentation and issue closure
- Files: `reference/ASSETS.md`, `reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`,
  `guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` §8 (History rows, `reviewed:` bumps); a dated
  note in `interoperability-layer.md` §5. Close
  `ASSET-EXPIRATION-EVENTS-CANNOT-BE-OBSERVED-EXCEPT-PER-ASSET`. Phase 5 of this design
  (`workflow: liquers-project`) then records the narrowed scope and the follow-ups.
- Proof: `python3 scripts/docs_index.py --check`.

## Testing Plan

Each step ends with `cargo test -p liquers-core --lib --tests`. Final:
`cargo test -p liquers-lib --lib --tests`; `bash scripts/check-build-matrix.sh` (wasm32 rows,
because `broadcast` is new to the wasm build); after `cargo clean`, the `liquers-web` Node loop as
a smoke run of the inline manager.

## Rollback Plan

Revert in reverse order. Steps 1-2 alone send to a channel nobody can subscribe to, which is
harmless.

## Documentation Updates

The three reference/guide documents above and the folder's own `interoperability-layer.md` note;
`specs/index.csv` regenerated; no `command_registry.yaml` change.

## Phase 5 Entry Criteria

- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour

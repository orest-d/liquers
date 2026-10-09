# Phase 4: Implementation Plan — asset-cancellation-outcome

## Overview
All behaviour lands in `liquers-core`; `liquers-axum` only tightens two tests. Order: the shared
request type and metadata support first (nothing uses them yet), then the run/finalize split that
makes the run the single status writer, then the cancel entry points and the cascade, then the
contexts and tests. No prerequisite issue (Phase 2 preflight: none blocking).

## Progress
- [x] Step 1: `CancellationRequest`, `is_in_flight`, `AssetData.cancellation`, `MetadataRecord::with_cancellation` — `b01da6d`
- [x] Step 2: single status writer — `compute` / `complete_evaluation`, the race in both run harnesses, guarded finalize / fail / cancel — `b01da6d`
- [x] Step 3: cancel entry points — `cancel`, `cancel_for_replacement`, service-loop `Cancel`, replacement sites, `submitted` — `b01da6d`
- [x] Step 4: cascade — both `wait_for_dependency` paths, `fail_due_to_dependency` removed, `State::value_error`, `no_binary_error` — `b01da6d`
- [x] Step 5: `Context::is_cancelled` / `check_cancelled` — `b01da6d`
- [x] Step 6: tests — new `tests/asset_cancellation.rs`, updated unit tests, tightened `aae92` / `aae38` — `b01da6d`
- [x] Step 7: validation across crates and the build matrix — `b01da6d`

## Implementation Steps
### Step 1: Request type and metadata
- Files / symbols: `liquers-core/src/assets.rs` `CancellationRequest`, `is_in_flight`,
  `AssetData::{cancellation, is_cancelled, mark_cancelled, identify_in_error}`;
  `liquers-core/src/metadata.rs` `MetadataRecord::with_cancellation`, `Metadata::with_cancellation`.
- Change: as Phase 2 Interfaces; `AssetData::set_cancelled` removed.
- Depends on: none. Proof: `cargo check -p liquers-core`. Rollback: revert the file hunks.

### Step 2: Single status writer
- Files / symbols: `assets.rs` `AssetRef::{compute, complete_evaluation, settle_run,
  attribute_run_error, finish_run_with_result, run_with_future, run_with_future_inline,
  finalize_status_with_version, try_to_set_ready, fail_asset, finish_cancelled}`, `RunOutcome`.
- Change: `finalize_status_with_version` / `try_to_set_ready` return `bool`; `fail_asset` returns
  `Result<bool, Error>`, routes a cancellation to `finish_cancelled`, and leaves a finalized progress
  bar alone; `persist_with_status_tracking(save_in_background)`; `JobFinished` is sent once, by the
  party that made the terminal transition.
- Depends on: 1. Proof: `cargo test -p liquers-core --lib` (`finished_run_progress_contract_*`,
  save-skip tests) and `ac01_…`, `ac02_…`, `ac07_…`. Rollback: revert; Step 1 alone is inert.

### Step 3: Cancel entry points
- Files / symbols: `AssetRef::{cancel, cancel_for_replacement, cancellation,
  own_cancellation_cause, submitted, to_override}`, `process_service_messages` `Cancel` arm;
  replacement sites in `AssetManager::remove`, `DefaultAssetManager::{set, set_state}`,
  `ImmediateAssetManager::{set_binary, set_state}`, `tests/common/minimal_manager.rs`.
- Depends on: 2. Proof: `ac04_…`, `ac08_…` (both). Rollback: revert with Step 2.

### Step 4: Cascade
- Files / symbols: `AssetRef::cancelled_dependency_cause`, `DefaultAssetManager::wait_for_dependency`,
  `AssetManager::wait_for_dependency` (default), `state.rs` `State::value_error`,
  `AssetRef::no_binary_error`; `fail_due_to_dependency` deleted.
- Depends on: 2. Proof: `ac03_…`, `ac09_…` (both), `ac12_…`, `dependency_failure_error_names_key`,
  `test_wait_for_evicted_expired_dependency_fails_parent`. Rollback: revert the hunks.

### Step 5: Context
- Files / symbols: `context.rs` `Context::{cancellation, is_cancelled, check_cancelled}` and every
  constructor (`new`, `with_volatile`, `clone_context`, `Clone`, the test fixture).
- Depends on: 1. Proof: `ac01_…`, `ac05_…`. Rollback: revert; nothing else reads the field.

### Step 6: Tests
- Files: `liquers-core/tests/asset_cancellation.rs` (new), `liquers-core/src/assets.rs` tests,
  `liquers-axum/tests/assets_api_endpoints.rs`. Depends on: 2-5. Proof: the commands in Step 7.

### Step 7: Validation
- `cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-axum --test assets_api_endpoints`;
  `cargo test -p liquers-lib --lib --tests` (Context and asset users); `cargo check -p liquers-core
  --target wasm32-unknown-unknown` and `-p liquers-web` for the wasm32 path (`run_with_future_inline`
  uses `futures::select_biased!` and `tokio::sync::Notify`). No `#[cfg(feature)]`, optional
  dependency or command signature changes, so neither the full `check-build-matrix.sh` nor a
  `specs/command_registry.yaml` regeneration is needed.

## Testing Plan
Steps 1-5 are checked with `cargo check -p liquers-core --tests` as they land; the full core suite
runs after Step 6, then axum and lib, then the build matrix once.

## Rollback Plan
Revert the PR: no stored format changes (`error_data` on a `Cancelled` record is an existing field).
Steps 2-4 must land together; Step 1 and Step 5 are inert on their own.

## Documentation Updates
Per Phase 2's documentation architecture: new `specs/guides/COMMAND_DESIGN_GUIDE.md`; updates with
`## History` row and `reviewed:` bump to `COMMAND_REGISTRATION_GUIDE.md`,
`ASSET_MANAGER_IMPLEMENTATION_GUIDE.md`, `WEB_API_GUIDE.md`, `ASSETS.md`, `ASSET_SET_OPERATION.md`,
`DEPENDENCIES_STATUS.md`, `ASSET_LIFECYCLE.md`, `api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`,
`WEB_API_SPECIFICATION.md`; `specs/README.md` link; `specs/index.csv` regenerated; source issue closed.

## Phase 5 Entry Criteria
- [x] Implementation finished and validated
- [x] User and review comments answered (none outstanding; pre-approved)
- [x] Documentation checkable against implemented and tested behaviour

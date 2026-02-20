# Implementation Plan: CORE-ASSETS-PERSISTENCE-GAPS (Issue 4)

Status: Draft  
Related feature: `specs/FEATURES/ASSETS-IMPROVEMENTS.md` (sections A and A1)  
Source issue: `specs/todo20260219.md` (Issue 4)

## Decisions (Fixed)
1. `asset.get()` returns computed value even when storing fails or is impossible.
2. Store failures must be recorded as metadata warnings with complete error information.
3. In-memory-only result is allowed.
4. All storing-failed cases are treated as non-serializable-asset behavior.

## Objective
Unify asset mutation/evaluation persistence behavior so all value-producing paths follow one contract:
1. value availability is not blocked by persistence failure,
2. persistence outcome is explicit in metadata,
3. failures are never silent.

## Scope
In scope:
1. `AssetRef` value-producing paths (`evaluate_and_store`, `set_value`, `set_state`).
2. shared persistence helper and metadata warning policy.
3. tests for serialization failure, store failure, and missing store key.

Out of scope:
1. sticky-eviction policy.
2. upload size limits.

## Current Gaps To Fix
1. `set_value` and `set_state` have TODO persistence gaps and inconsistent metadata handling.
2. `evaluate_and_store` can ignore `save_to_store` errors.
3. persistence failure semantics are not first-class in metadata.
4. no test coverage for background save failure and no-store-key failure visibility.

## Implementation Steps

### Phase 1: Persistence Status Model
1. Add internal persistence status type in `liquers-core/src/assets.rs` (not wrapped in `Option`):
   1. `None` (no persistence attempt yet),
   2. `Persisted`,
   3. `NonSerializable`,
   4. `NotPersisted`.
2. Model error details separately from status (e.g. `last_persistence_error: Option<Error>` or metadata log entry source), because:
   1. `NonSerializable` is not an error status by itself,
   2. `NotPersisted` indicates failure and should carry complete error context.
3. Add helper that applies persistence status/error to metadata/log:
   1. writes warning log entry,
   2. includes complete error text,
   3. includes key/query/asset reference where available.
4. Keep status/value behavior unchanged for successful computations (`get()` remains value-first).

### Phase 2: Shared Write-Through Helper
1. Introduce one internal helper used by all value-setting paths (e.g. `apply_value_and_attempt_persist`):
   1. set data in memory,
   2. normalize type metadata (`type_identifier`, `type_name`),
   3. determine final status (`Ready`/`Volatile`/provided status),
   4. attempt store write,
   5. map result to `PersistenceStatus`,
   6. emit notifications.
2. Ensure helper supports both sync and background persistence mode without dropping errors:
   1. synchronous path returns explicit outcome,
   2. background path records outcome into metadata once task completes.

### Phase 3: Integrate Existing Paths
1. Refactor `AssetRef::evaluate_and_store` to use helper and stop discarding persistence errors.
2. Refactor `AssetRef::set_value` to use helper (remove TODO persistence gap).
3. Refactor `AssetRef::set_state` to use helper and fix metadata ordering bug:
   1. merge incoming state metadata first,
   2. enforce inferred `type_identifier/type_name` after merge,
   3. then persist.
4. Keep manager-level `set` / `set_state` behavior compatible, but align warning/error shape where feasible.

### Phase 4: Error Classification and Non-Serializable Handling
1. Classify persistence outcomes into status + details:
   1. serialization failure,
   2. store backend write error,
   3. `store_to_key()` missing.
2. Map classification to statuses:
   1. serialization failure -> `NonSerializable`,
   2. store backend write error -> `NotPersisted`,
   3. missing store key -> `NotPersisted` (unless later reclassified by policy).
3. For each status:
   1. value remains available in memory,
   2. metadata warning is appended with full error for failure cases,
   3. `NonSerializable` produces explicit informational/warning metadata that persistence is not possible for this value representation.
4. Ensure no panic and no silent drop of persistence problems.

### Phase 5: Tests
Add/extend tests in `liquers-core/src/assets.rs` test module (or `liquers-core/tests/` when cleaner):
1. `get_returns_value_when_store_write_fails`:
   1. use failing async store stub,
   2. confirm `asset.get()` succeeds,
   3. confirm metadata has persistence warning and full error text.
2. `get_returns_value_when_store_key_missing`:
   1. recipe without storable key,
   2. confirm `get()` succeeds,
   3. metadata warning present.
3. `set_state_nonserializable_kept_in_memory_with_warning`:
   1. force serialization failure,
   2. confirm in-memory value present,
   3. warning recorded.
4. `set_value_persists_when_possible`:
   1. happy path persists and has no persistence warning.
5. Regression:
   1. existing storage test still passes (`test_asset_storage`),
   2. metadata type fields remain correct after `set_state`.

### Phase 6: Documentation
1. Update `specs/FEATURES/ASSETS-IMPROVEMENTS.md` status/checklist once implemented.
2. If Issue 4 is complete, mark as closed in `specs/todo20260219.md` with test evidence.

## File-Level Change Map (Planned)
1. `liquers-core/src/assets.rs`: core implementation and primary tests.
2. `liquers-core/src/context.rs`: no behavior change expected, but confirm compatibility with refactored setters.
3. `specs/FEATURES/ASSETS-IMPROVEMENTS.md`: progress/status update after implementation.
4. `specs/todo20260219.md`: closure note after verification.

## Verification Commands (Planned)
1. `cargo test -p liquers-core`
2. `cargo test -p liquers-lib` (regression confidence across integration usage)

## Risks
1. Background-save warning update races with concurrent metadata updates.
2. Over-warning spam if repeated failed save attempts append duplicate messages.
3. Behavior drift between manager-level and asset-level setters if not aligned carefully.

## Mitigations
1. Deduplicate persistence warnings by message key/tag.
2. Keep persistence outcome update centralized in one helper.
3. Add focused regression tests for both API layers (`AssetRef` and `AssetManager`).

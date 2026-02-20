# ASSETS-IMPROVEMENTS

Status: Draft

## Summary
Improve asset robustness for externally set data by:
1. supporting non-serializable states safely,
2. protecting non-derivable assets from accidental eviction,
3. enforcing configurable upload size limits.
4. defining persistence-failure behavior consistently for evaluated assets.

## Problem
Current behavior risks:
1. `set_state()` may fail to persist non-serializable values.
2. Source/Override assets can be evicted and lost (especially non-serializable in-memory values).
3. `set()` accepts arbitrary binary size without limits.
4. Value may be available via `get()` even when persistence fails, but this is not explicitly modeled in metadata.
5. Store-write failures can be silent in some evaluation paths.

## Goals
1. Keep `set_state()` usable with non-serializable values.
2. Prevent silent data loss for Source/Override assets.
3. Add hard size guardrails for uploads.
4. Preserve `get()` usability even when storing fails or is impossible.
5. Make persistence failures explicit in metadata with complete error details.

## Non-Goals
1. Streaming upload protocol design.
2. Full memory-pressure policy framework.

## Proposed Feature Scope

### A. Non-Serializable State Handling
1. `set_state()` tries serialization.
2. If serialization fails, store metadata only and keep in-memory state in AssetRef.
3. Mark metadata/log to indicate persistence limitation.
4. Treat all "storing failed/impossible" cases as non-serializable-asset behavior class:
   1. serialization failure,
   2. store backend write failure,
   3. no storable key (`store_to_key() == None`).

### A1. Persistence Failure Semantics (NEW)
1. `asset.get()` must return computed value even if persistence fails or is impossible.
2. Persistence failure must not be silent:
   1. metadata warning entry is required,
   2. warning must include complete error information (full error text/chain available in runtime context),
   3. warning should include key/query/asset-reference context when available.
3. In-memory-only result is allowed and considered valid runtime outcome.
4. Persisted vs non-persisted outcome should be discoverable from metadata/logs.

### B. Sticky Asset Eviction Policy
1. Default non-evictable statuses: `Source`, `Override`.
2. Retain explicit remove/clear semantics.
3. Add visibility metrics/logs for sticky asset count/size.

### C. Upload Size Limit
1. Add configurable `max_binary_size` for `set()`.
2. Reject oversize payloads with typed error.
3. HTTP layer maps to `413 Payload Too Large`.

## Cross-Cutting Requirements
1. All behavior should be testable without external store dependencies.
2. Warning/error messages should include key/query context.
3. Existing successful `set()` / `set_state()` flows should remain backward-compatible.
4. Any path that computes/sets value and then fails to store must follow A1 behavior (non-serializable-asset handling).

## Suggested Milestones
1. Milestone 1: size limit + errors.
2. Milestone 2: unify non-serializable/persistence-failure handling for `set_state`, `set_value`, and recipe evaluation save path.
3. Milestone 3: sticky eviction policy + observability.

## Acceptance Criteria
1. Non-serializable value can be set via `set_state()` without hard failure.
2. Source/Override assets are not evicted by normal LRU path.
3. Oversized binary upload is rejected deterministically.
4. `asset.get()` still returns value when store write fails or store key is unavailable.
5. Metadata contains warning with complete error details for all persistence-failure cases.
6. Unit/integration tests cover all areas including persistence-failure semantics.

## Implementation Notes For Future Fix
1. Decision record:
   1. `get()` is value-first and must not fail solely due to persistence failure.
   2. Store failure/impossibility is represented as metadata warning, not hard runtime failure of `get()`.
   3. In-memory-only results are allowed.
2. Existing risk areas to align with this feature:
   1. evaluation path where save result can be ignored,
   2. setter paths (`set_value`/`set_state`) that may update memory without consistent persistence signaling.
3. Recommended implementation pattern:
   1. centralize persistence attempt into one helper returning `PersistenceStatus` (no `Option` wrapper):
      1. `None` (no persistence attempt yet),
      2. `Persisted`,
      3. `NonSerializable`,
      4. `NotPersisted`.
   2. store full error details separately (metadata log and/or explicit last persistence error field).
   3. always attach persistence status and details to metadata/log before notifying completion.
4. Minimal test matrix:
   1. serializable + writable store -> persisted, no warning.
   2. non-serializable value -> `NonSerializable`, explicit persistence limitation note, `get()` succeeds.
   3. store write error -> warning with complete error, `get()` succeeds.
   4. no store key -> warning, `get()` succeeds.

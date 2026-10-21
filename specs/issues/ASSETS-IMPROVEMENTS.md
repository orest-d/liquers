---
id: ASSETS-IMPROVEMENTS
kind: feature
title: The web API's request-body limit is axum's fixed 2 MiB default, not configurable or documented
status: accepted
priority: P2
complexity: M
area: [axum, docs]
design: http-request-body-limit
created: 2026-08-08
github:
---
# ASSETS-IMPROVEMENTS

## Status at HEAD (2026-10-20, bulk-design triage) — what remains

Re-scoped. Verified at commit 18732dd; the original feature text follows as history.

- **A, A1 — non-serializable values and persistence failures: done.** `PersistenceStatus`
  (`None`, `Persisted`, `NonSerializable`, `NotPersisted`; `liquers-core/src/assets.rs:410`) is
  recorded on every save path (`persist_with_status_tracking`, `record_persistence_result`), with a
  metadata warning carrying the complete error and the asset reference
  (`AssetData::set_persistence_status`); `get` still returns the value. `set_state` stores a value
  with no byte form metadata-only (`STORE_SEMANTICS.md` §2). The "no store key" case is obsolete:
  an unkeyed asset is never written by design (`ASSETS.md` §Key ownership), so it is not a
  persistence failure.
- **B — sticky eviction of `Source` / `Override`: moved.** There is no LRU or memory-pressure
  eviction at HEAD; the maps drop only expired, volatile and stale-terminal entries. A bounded cache
  is `CORE-ASSET-GC`, which must keep non-derivable `Source` values. `ASSET_SET_OPERATION.md`
  §Eviction Handling still describes LRU eviction (corrected by the design below). Found while
  checking: `TO-OVERRIDE-KEEPS-THE-PENDING-EXPIRATION-DEADLINE`.
- **C — upload size limit: remains, re-scoped to the web API.** Every body-taking route of
  `liquers-axum` (`Bytes`, `Json`, `Multipart` extractors) is limited by axum's built-in default of
  2 MiB, which no builder exposes and no document states; an over-size body gets axum's plain-text
  413. Problem example:

  ```text
  $ curl -X PUT --data-binary @big.bin http://localhost:3000/liquer/api/store/data/data/big.bin
  HTTP/1.1 413 Payload Too Large            # big.bin is 3 MB
  Failed to buffer the request body: length limit exceeded
  ```

  Expected: a per-router builder option for the limit (raise, lower, disable), documented with the
  413 it produces. A core `max_binary_size` for `set()` is proposed to be dropped (in-process
  callers already hold the bytes). Re-sized `M` (one crate plus documents). Design:
  `http-request-body-limit`.



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

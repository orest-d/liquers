# ASSETS-IMPROVEMENTS

Status: Draft

## Summary
Improve asset robustness for externally set data by:
1. supporting non-serializable states safely,
2. protecting non-derivable assets from accidental eviction,
3. enforcing configurable upload size limits.

## Problem
Current behavior risks:
1. `set_state()` may fail to persist non-serializable values.
2. Source/Override assets can be evicted and lost (especially non-serializable in-memory values).
3. `set()` accepts arbitrary binary size without limits.

## Goals
1. Keep `set_state()` usable with non-serializable values.
2. Prevent silent data loss for Source/Override assets.
3. Add hard size guardrails for uploads.

## Non-Goals
1. Streaming upload protocol design.
2. Full memory-pressure policy framework.

## Proposed Feature Scope

### A. Non-Serializable State Handling
1. `set_state()` tries serialization.
2. If serialization fails, store metadata only and keep in-memory state in AssetRef.
3. Mark metadata/log to indicate persistence limitation.

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

## Suggested Milestones
1. Milestone 1: size limit + errors.
2. Milestone 2: non-serializable `set_state()` metadata-only persistence.
3. Milestone 3: sticky eviction policy + observability.

## Acceptance Criteria
1. Non-serializable value can be set via `set_state()` without hard failure.
2. Source/Override assets are not evicted by normal LRU path.
3. Oversized binary upload is rejected deterministically.
4. Unit/integration tests cover all three areas.

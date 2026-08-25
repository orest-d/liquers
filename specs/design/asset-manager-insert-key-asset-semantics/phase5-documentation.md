# Phase 5: Documentation - asset-manager-insert-key-asset-semantics

## Completion Preconditions

- [x] Implementation is finished and validated
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated
- [x] Documentation is consistent with implemented and tested behavior
- [x] Documentation is included in the implementation PR when practical

## Implementation Summary

Removed the public `insert_key_asset` trait method. Both bundled managers now expose a crate-private
atomic first-claim helper and serialize keyed mutation/cache/eviction work with durable store I/O.
`to_override` no longer reinserts a stale old ref after persistence.

## Documentation Delivered

### New Reference Documents
None.

### New Guide Documents
None: behavior is crate-private.

### Existing Documents Reviewed or Updated
`reference/ASSETS.md` and `reference/ASSET_SET_OPERATION.md` were reviewed for Phase 5 follow-up.

### Links and Capability Map
[Links added, updated, or replaced in `specs/README.md` and other documentation]

## Issues Filed

None; `QUEUED-MANAGER-EVICTION-RACE` remains separate.

## Important Learning

Map reachability is not lifecycle. Matching map insertion alone cannot order stale store writes
after an await; the manager mutation gate does.

## Conformance and Remaining Work

Requested, approved, and implemented scope match; no work remains in this issue.

## Validation

Passed: 90 asset unit tests, 34 expiration integration tests, 16 manager parity tests, wasm target
check, and `git diff --check`.

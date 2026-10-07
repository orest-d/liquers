# Phase 5: Documentation

**Status: executed and approved 2026-10-07.**

## Summary

Implemented 2026-10-07 as Wave 1 step 12 of
`archive/2026-10-06-p2-p3-s-implementation-order-revised.md`.

- New crate-private `AssetRef::expiry_subject()` (reads the dependency's `expiry_subject()` under
  its own read lock only).
- `DefaultAssetManager::wait_for_dependency`: the two messages now read "Dependency {subject} did
  not produce a value (status …)" and "Dependency {subject} expired and was evicted before its
  value could be used", naming the key, else the query, never the runtime id.
- Tests: `dependency_failure_error_names_key`, `evicted_dependency_error_names_key`; the existing
  `test_wait_for_evicted_expired_dependency_fails_parent` still matches its phrase.

## Conformance and deviations

As designed. `note_expired_dependency` was left unchanged (optional step).

## Documentation

None needed: no reference or guide quotes these messages.

## New issues

None.

# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** It changes the text of two error messages and reuses an existing helper. It
  adds no API, and the persisted shape is unchanged (it is still a `general_error`).
- **Open questions:** None

## Problem

`DefaultAssetManager::wait_for_dependency` (`liquers-core/src/assets.rs`) builds two errors with
the runtime asset id: "Dependency asset {id} did not produce a value (status …)" and "Dependency
asset {id} expired and was evicted before its value could be used". Both reach the dependent's
metadata through `fail_due_to_dependency`, so a persisted record says "asset 1001", which means
nothing outside the process.

## Expected behaviour

Both messages name the dependency by `AssetData::expiry_subject()`: its key, else its encoded
query, else "an anonymous asset". This is the helper Step 4 of
`dependency-audit-and-expiry-provenance` introduced for the log lines of the same path.

Acceptance:

1. A keyed dependency that ends `Cancelled` with no stored error: the dependent's error message
   contains the key text (`data/b.txt`), and no asset id.
2. An expired-and-evicted dependency: the message contains the dependency's key or query, and
   still contains "expired and was evicted before its value could be used" (an existing test
   matches that phrase).

## Scope

- Only the two messages in `DefaultAssetManager::wait_for_dependency`. The trait default (used by
  the immediate manager) returns the dependency's own error and builds no id-based message.
- Not changed: the `Error` type, `error.key`, or any status logic.

## Design Dependencies

- `dependency-audit-and-expiry-provenance` — **overlaps** (complete). It supplies `expiry_subject`.

## Documentation assessment

No reference states these message texts. No guide. Update the source issue only.

## Consolidated Findings

- Read the subject the same way `note_expired_dependency` does: take the dependency's read lock,
  read `expiry_subject()`, and release it before touching the parent. This preserves the
  lock-order rule documented there.
- Add an `AssetRef::expiry_subject(&self) -> String` (crate-private) wrapper so the two sites and
  `note_expired_dependency` share it.
- Existing test at `assets.rs` (substring "expired and was evicted before its value could be used")
  must keep matching.

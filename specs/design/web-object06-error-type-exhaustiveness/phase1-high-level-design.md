# Phase 1: High-Level Design - Compiler-Checked `ErrorType` List for OBJECT06

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** The fix is test-only and its requirement — adding an `ErrorType` variant must
  fail to *compile* until the list is updated, instead of failing a hand-kept count — is met by
  generating the list and an exhaustive `match` from one macro invocation.
- **Open questions:** None

## Problem and Evidence

`liquers-web/tests/objects_OBJECT.rs`:

- `object06_every_enum_variant_roundtrips` asserts `ALL_ERROR_TYPES.len() == 22`; the list has
  **23** entries since `1c4acaf` added `StatusConflict`, so the test fails (`left: 23, right: 22`).
- Verified at HEAD (2026-10-04), the list is **also incomplete**: `ErrorType`
  (`liquers-core/src/error.rs` ≈13) has **24** variants, and `ErrorType::KeyNotAbsolute` is
  missing from `ALL_ERROR_TYPES`, although `liquers_web::error::error_type_name` /
  `error_type_from_name` map it (`key_not_absolute`). So OBJECT06 and ERROR01 never exercised it.
- The list's doc comment claims the exhaustive `match` in `error_type_name` keeps it honest; it
  does not — that match lives in the library, and nothing ties the test list to it. That is how
  both drifts happened.

## Expected Behaviour and Acceptance Criteria

1. `ALL_ERROR_TYPES` contains every `ErrorType` variant exactly once (24 today).
2. Adding a variant to `ErrorType` without adding it to the test list is a **compile error** in
   the test file (under `--target wasm32-unknown-unknown`).
3. No hand-kept variant count remains in the file.
4. OBJECT06 (name round trip, distinct names) and ERROR01 (every type maps) pass under the Node
   loop, now including `KeyNotAbsolute`.

## Affected Systems

`liquers-web` Node conformance loop only. No library code, no core change.

## Scope and Non-Goals

Non-goals: adding a variant iterator to `ErrorType` in `liquers-core` (e.g. `strum`) — a core
dependency for a test convenience; other bindings' lists (`liquers-py` has none checked).

## Documentation Assessment

`specs/design/liquers-web/phase3-examples.md` is a completed design (frozen) — no edit.
`liquers-web/README.md` — no mention of the count; none. Close the issue.

## Design Dependencies

None.

## Consolidated Findings

- The issue understated the drift: the expected count is 24, not 23, because `KeyNotAbsolute`
  was never listed. A plain number change to 23 would leave the test green and the variant
  untested.
- A `macro_rules!` in the test file expanding one variant list into both the
  `const ALL_ERROR_TYPES` and a `fn` containing an exhaustive `match` with no `_ =>` arm gives
  the compile-time guarantee with no new dependency.
- The file is `#![cfg(target_arch = "wasm32")]`, so the guarantee only fires in the wasm loop —
  which is where the test runs; no native mirror is warranted.

## Review

Test-only, small, and the guarantee is structural.

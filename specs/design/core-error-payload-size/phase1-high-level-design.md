# Phase 1: Box the Core Error Payload

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** The landed newtype preserves field access and flat serialization while reducing
  `Error` to one pointer; the issue records measured validation.
- **Open questions:** None

## Problem, Behaviour, and Acceptance

Inline diagnostic strings and contexts made every `Result<T, Error>` carry a 176-byte error
variant. `Error` must become pointer-sized without changing constructors, direct field access,
serialized keys, or call sites. Acceptance is zero `result_large_err` warnings plus wire and suite
regressions.

## Scope and Compatibility

Only `liquers-core/src/error.rs` changes. `ErrorPayload` and conversion helpers are additive public
surface; `Deref`/`DerefMut` retain source compatibility and transparent serde retains stored
metadata compatibility. Allocation on error construction is the deliberate tradeoff.

## Design Dependencies

None.

## Documentation Assessment

Rustdoc on `Error` and `ErrorPayload` owns the representation rationale. No user-facing reference
change was required.

## Consolidated Findings

Use `Error(Box<ErrorPayload>)`, transparent serde, deref compatibility, and explicit payload
conversion. Test size, flat literal JSON, skipped `command_key`, direct mutation, and payload
round-trip; measure clippy before/after and run all affected crates and bindings.

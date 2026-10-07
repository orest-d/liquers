# Phase 5: Documentation - Compiler-Checked `ErrorType` List for OBJECT06

**Status: executed 2026-10-07**, after implementation (Wave 5 step 29 of
`archive/2026-10-06-p2-p3-s-implementation-order-revised.md`). Awaiting approval.

## Completion Preconditions

- [x] Implementation is finished and validated (Phase 4)
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR

## Documentation Plan

### New Reference Documents

None: the change extends behaviour that existing documents own.

### New Guide Documents

None.

### Existing Documents to Update (`affects_docs`)

None — see the candidates below for why. `affects_docs` is `[]`.

### Candidates Considered and Discarded

By area (`web`): `WEB_API_SPECIFICATION.md` ≈138 concerns the axum `ErrorType` → HTTP mapping, not this test; `LANGUAGE-INTEGRATION_GUIDE.md`, `STORE_SEMANTICS.md`, `STORE_IMPLEMENTATION_GUIDE.md`, `RECORD_STREAMS.md`, `COMMAND_DECLARATION.md` do not mention the list. `liquers-web/README.md` (outside `specs/`) does not mention a variant count. The frozen `design/liquers-web/` is not edited. So `affects_docs` is empty: a test-only change.

### Links and Capability Map

None needed in `specs/README.md`: the capability is reached through the documents above.

### Issues to Close

`WEB-OBJECT06-EXPECTS-A-STALE-ERROR-TYPE-COUNT` → `closed`, resolution noting the missing `KeyNotAbsolute`.

## Implementation Summary

`liquers-web/tests/objects_OBJECT.rs`: an `error_types!` macro takes the variant list once and
generates both `ALL_ERROR_TYPES` and a `listed` function whose `match` over `ErrorType` names every
variant with no default arm. The `len() == 22` assertion is gone. The list now has 24 entries: it
was missing `KeyNotAbsolute`, which the stale count had hidden.

## Documentation Delivered

None, as planned (`affects_docs: []`).

## Issues Filed

None.

## Important Learning

A count assertion over an enum is a weaker check than an exhaustive `match`: the count was stale
and also wrong about which variant was absent. The `match` names the missing variant at compile time.

## Conformance and Remaining Work

None.

## Validation

- `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles --test objects_OBJECT`: 14 passed.
- Manual compile check (T2): deleting `KeyNotAbsolute` from the list fails with
  `E0004: non-exhaustive patterns: ErrorType::KeyNotAbsolute not covered`; restored afterwards.
- `python3 scripts/docs_index.py --check`: 0 errors.

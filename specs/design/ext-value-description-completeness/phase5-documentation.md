# Phase 5: Documentation

**Status: executed 2026-10-07; approved 2026-10-08 (maintainer).**

## Summary

Implemented 2026-10-07 as Wave 2 step 18 of
`archive/2026-10-06-p2-p3-s-implementation-order-revised.md`. Test-only, as designed.

- `liquers-lib/tests/value_type_system.rs`: `statically_described` (exhaustive match over
  `ExtValue`, feature-gated arms, `Foreign` → `false` with its reason) and `samples()` (one value
  per described variant in the build). `UIElement` uses `Placeholder`, `egui.Widget` a local
  no-op `SampleWidget`, `egui.Command` a no-op `UiCommand`, the records variants a zero-row
  `RecordBatch` and an `InMemorySource` over it.
- `ext_value_type_descriptions_complete` (T1, kept name) now checks every sample;
  `ext_value_type_descriptions_have_no_stale_entries` (T2) checks the reverse.
- T3: renaming `RecordView`'s `TypeInfo` identifier made T1 and T2 fail; reverted.
- T4: the per-feature runs and `scripts/check-build-matrix.sh` pass.

## Conformance and deviations

As designed. Every variant had a cheap constructor, so no allowance list was needed. No missing
`TypeInfo` was found.

## Documentation

`guides/TYPE_SYSTEM_GUIDE.md` §Verifying it: the test samples every variant but `Foreign`, fails to
compile when a variant is added without a sample, and has a reverse sibling. History row added,
`reviewed:` bumped.

## New issues

None.

## Validation

`cargo test -p liquers-lib --test value_type_system` in each feature configuration;
`bash scripts/check-build-matrix.sh`; `python3 scripts/docs_index.py --check`.

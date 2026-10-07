# Phase 5: Documentation

**Status: executed 2026-10-07; awaiting approval.**

## Summary

Implemented 2026-10-07 as Wave 2 step 19 of
`archive/2026-10-06-p2-p3-s-implementation-order-revised.md`.

- `ValueExtension::try_into_string_option` (default `try_into_string().map(Some)`) in
  `liquers-lib/src/value/extended.rs`; `CombinedValue` delegates it to whichever side holds the
  value.
- `impl ValueExtension for ExtValue::try_into_string_option` (`liquers-lib/src/value/mod.rs`) with
  the same feature-gated variant arms as `try_into_i64_option`; `RecordView` reads its single cell
  through the new `record_view_cell_string_option` helper (`records`-gated).
- Tests: `null_text_cell_string_option_is_none` (T1, also a null `Int` cell; fails before the fix
  with `Some("None")`), `text_cell_string_option_is_some` (T2 and T3) in
  `liquers-lib/tests/record_scalar_reading.rs`; the existing numeric option tests (T4) pass; the
  default-hook unit test also covers the string hook.

## Conformance and deviations

As designed. The non-record arms answer `self.try_into_string().map(Some)` (Phase 2 §3), listed
explicitly with the same feature gates as `try_into_i64_option`, so a variant that later gains a
string conversion gets the option form for free. All of them refuse today.

## Documentation

`reference/RECORD_STREAMS.md`, scalar-reading table: the `Null` row names
`try_into_string_option` too. History row added, `reviewed:` bumped.

## New issues

None.

## Validation

`cargo test -p liquers-lib --lib --tests`; `bash scripts/check-build-matrix.sh`;
`python3 scripts/docs_index.py --check`.

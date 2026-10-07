# Phase 5: Documentation

**Status: executed 2026-10-07; awaiting approval.**

## Summary

Implemented 2026-10-07 as Wave 2 step 17 of
`archive/2026-10-06-p2-p3-s-implementation-order-revised.md`, after `text-value-markdown-format`.

- `SimpleValue::as_bytes`: the textual arm is `"txt" | "html" | "rs" | "py" | "css" | "js"`, as in
  core, and gained `Bytes` (raw), `Query` and `Key` (encoded). New `"bytes" | "b" | "bin"` arm for
  `Bytes` and `Text`. The touched refusals use `Error::from_error`. The final refusal arm is kept
  where core has one, with a comment saying so.
- `SimpleValue::deserialize_from_bytes`: the textual arm reads `Query` / `Key` by parsing,
  `Bytes` raw, and the base scalars and `Text` as `Text`. The new `bytes` arm reads `""` / `Bytes`
  as `Bytes` and `Text` as `Text`.
- Tests (`liquers-lib/src/value/simple.rs`): `every_declared_format_round_trips` replaces
  `every_declared_format_round_trips_or_is_recorded_as_unwritable`, and `UNWRITABLE` is deleted
  (T1); `bytes_are_written_raw_as_bin` (T2); `query_as_txt_matches_core` (T3);
  `writes_the_same_bytes_as_core_value` (T4, every declared non-JSON pair).

## Conformance and deviations

- **Read rule for non-base identifiers.** The new formats (`rs`, `py`, `css`, `js`, `b`, `bin`,
  `bytes`) refuse an identifier `SimpleValue` does not own, instead of reading it as text or bytes.
  `CombinedValue` asks the base serializer first, so claiming those identifiers would hide the
  extension's reader (the trap `text-value-markdown-format` hit with `md`). `txt`, `html` and
  `toml` keep their old "any identifier reads as `Text`" rule; changing that is
  `COMBINED-VALUE-DISCRIMINATION`.
- **`Text` as bytes reads back as `Text`**, where core reads `Bytes`. The identifier is the
  discriminator, and the strict round-trip test requires it.
- **`Bytes` as text reads back as `Bytes`**, where core refuses the identifier. Reading it as text
  would lose every non-UTF-8 byte.
- Phase 2 asked to copy core's identifier dispatch for `Query` and `Key`, which is done. The base
  scalars still read as `Text` (Phase 1 acceptance 2); the remaining read divergence is filed.
- T4 compares bytes for non-JSON formats only: `SimpleValue`'s JSON is tagged and core's untagged,
  by design.

## Documentation

None needed (Phase 1 assessment): `VALUE_TYPE_SYSTEM.md` describes the shared descriptions, which
are now true for `SimpleValue` too.

## New issues

- `SIMPLE-VALUE-READS-TEXT-SCALARS-AS-TEXT` (P3, S): a scalar written as text reads back as
  `Text` in `liquers-lib`, as its type in core.

## Validation

`cargo test -p liquers-lib --lib --tests`; `bash scripts/check-build-matrix.sh`;
`python3 scripts/docs_index.py --check`.

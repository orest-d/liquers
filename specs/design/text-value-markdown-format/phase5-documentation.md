# Phase 5: Documentation - Markdown as a `Text` Data Format

**Status: executed 2026-10-07**, after implementation (Wave 2 step 16 of
`archive/2026-10-06-p2-p3-s-implementation-order-revised.md`). Awaiting approval.

## Completion Preconditions

- [x] Implementation is finished and validated (Phase 4)
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated (none yet)
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR

## Documentation Plan

### New Reference Documents

None: the change extends behaviour that existing documents own.

### New Guide Documents

None.

### Existing Documents to Update (`affects_docs`)

| Document | Planned change |
|---|---|
| `guides/TYPE_SYSTEM_GUIDE.md` | §Choosing a data format at write time (≈198): one line noting that `md` on a `Text` writes the text as is (plain markdown), while `md` on a `RecordView` renders a markdown table (≈138) — the same extension is valid on several types, each with its own meaning |

Each updated `reference/` or `guides/` document gets a `## History` row (source `phase-5`) and a
`reviewed:` bump in the same commit.

### Candidates Considered and Discarded

By area (`core/value`, `lib/value`): `VALUE_TYPE_SYSTEM.md` enumerates per-type formats only for the record identifiers (≈103), not for base types, so it makes no claim this changes; `PROJECT_OVERVIEW.md` lists modules only.

### Links and Capability Map

None needed in `specs/README.md`: the capability is reached through the documents above.

### Issues to Close

`TEXT-VALUE-CANNOT-BE-STORED-AS-MARKDOWN` → `closed`, resolution naming the tests.

## Implementation Summary

- `Value::type_descriptions` declares `md` on `Text` only, through a separate
  `.with_data_formats(["md"])`; `TEXTUAL` is unchanged, so no scalar gains `md`.
- `DefaultValueSerializer for Value` and `for SimpleValue`: a separate `"md"` write arm accepts
  `Text` only (others: `SerializationError` via `Error::from_error`), and `md` joins each text
  read arm, so an untyped `.md` entry reads as `Text`, as `txt` does.
- The lib round-trip test's expected-value table learned `md`; its `UNWRITABLE` list is unchanged.
- **Deviation:** `SimpleValue` reads `md` only for the identifiers `""` and `Text`, in its own arm,
  rather than adding `md` to its text arm (which reads every identifier as `Text`). `CombinedValue`
  asks the base serializer first and the extension only when the base refuses, so widening the
  text arm made a `RecordView` written as `md` read back as `Text`; refusing other identifiers lets
  the extension read the table. Core `Value` has no extension, so its shared text arm is kept.
- Tests: `markdown_is_text_only` and the extended `scalar_identifiers_round_trip_through_the_serializer`
  / `supports_data_format_agrees_with_the_registry` (core), `simple_value_text_round_trips_as_markdown`
  and `record_view_markdown_reads_back_as_a_table_through_the_combined_value`
  (`liquers-lib/tests/record_typeinfo.rs`, the regression test for the deviation), and the
  integration test `text_value_is_stored_and_read_as_markdown`
  (`liquers-core/tests/text_markdown_storage.rs`), which fails with the core change stashed.

## Documentation Delivered

`guides/TYPE_SYSTEM_GUIDE.md` §Choosing a data format: `md` on `Text` (plain markdown) versus on
`RecordView` (a markdown table). History row added, `reviewed:` bumped.

## Issues Filed

None new. Evidence added to `COMBINED-VALUE-DISCRIMINATION` (see Important Learning).

## Important Learning

`md` was already a valid write format for `RecordView`, so the extension now has two meanings
selected by the value's type. In `liquers-lib` that only works if the base serializer refuses an
identifier it does not own. Its `txt`, `html`, `json` and `yaml` arms do not refuse, so a
`RecordView` written as `json` reads back as an `Array`, one written as `html` as `Text`, and a
`RecordSource` manifest as an `Object`. That is the subject of the existing
`COMBINED-VALUE-DISCRIMINATION`; the evidence is added there.

## Conformance and Remaining Work

Conforms to Phases 1–4. No remaining work.

## Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests`;
`cargo test -p liquers-lib --no-default-features --lib --tests`; `python3 scripts/docs_index.py --check`.

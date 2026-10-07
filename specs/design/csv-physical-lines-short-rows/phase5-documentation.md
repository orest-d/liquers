# Phase 5: Documentation - CSV errors name physical lines; short rows are padded with an aggregate warning

**Status: executed 2026-10-07**, after implementation (Wave 4 step 25 of
`archive/2026-10-06-p2-p3-s-implementation-order-revised.md`). Awaiting approval.

## Completion Preconditions

- [x] Implementation is finished and validated (Phase 4)
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated (none yet)
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR

## Documentation Plan

### New Reference Documents

None.

### New Guide Documents

None.

### Existing Documents to Update

| Document | Change |
|---|---|
| `reference/RECORD_STREAMS.md` | CSV notes: line numbering, the padding rule, the aggregate warning and where it goes (asset log or stderr), trailing newline. History row (`reviewed:` already 2026-10-07) |

### Candidates Considered and Discarded

`guides/RECORD_STREAM_GUIDE.md` (no CSV error or padding text); `TABLE_FORMATS` content lives in
RECORD_STREAMS.md.

### Issues to Close

`CSV-ROW-NUMBERS-COUNT-RECORDS-AND-SHORT-ROWS-READ-AS-NULL`.

## Implementation Summary

- `liquers-records/src/formats/csv.rs`:
  - `ParsedRow { line, fields }`. `parse_rows` counts every line break, including those inside a
    quoted cell, where `\r\n` counts once.
  - `RowPosition` formats `CSV line N`, or `CSV line N (record M)` when a multi-line cell has made
    them differ.
  - `ShortRows` aggregates padded rows; `check_row_width` notes them.
  - `cell_value` pads a missing cell with null in a nullable field, `""` in a non-nullable `Text`,
    and returns an error otherwise.
  - Both readers return a `ReadReport`.
- `liquers-records/src/formats/mod.rs`: `ReadReport` and `read_table_with_report`. `read_table`
  keeps its signature and writes warnings to stderr. Both are re-exported from the crate root.
- `liquers-lib/src/records/convert.rs`: `read_table_from_bytes` takes the context, calls
  `read_table_with_report`, and passes each warning to `context.warning`.
- Tests:
  - New: `csv_error_names_physical_line_and_record` (T1),
    `csv_short_rows_are_padded_and_reported_once` (T2),
    `csv_inferred_short_rows_are_padded_and_reported_once` (T5),
    `csv_short_non_nullable_text_pads_empty` (T3), `csv_short_non_nullable_int_is_an_error` (T4),
    `csv_trailing_newline_is_not_a_row` (T6, LF and CRLF),
    `to_record_logs_padded_csv_rows` (T7, `liquers-lib/tests/records_end_to_end.rs`).
  - Three existing assertions changed from `row N` to `line N`.

**Deviation in T7:** Phase 3 stored the CSV at `data/short.csv`. A stored CSV without a
`RecordView` type identifier cannot be loaded at all; a CSV with that identifier is deserialized
before any command runs, so no context exists to log to. T7 therefore takes the bytes from a
fixture command. The defect is filed below.

## Documentation Delivered

As planned above.

## Issues Filed

- [`STORED-UNTYPED-FILE-OF-UNLISTED-FORMAT-CANNOT-BE-READ`](../../issues/STORED-UNTYPED-FILE-OF-UNLISTED-FORMAT-CANNOT-BE-READ.md)
  (P2): a hand-placed `csv` (or `png`, `parquet`, …) whose metadata has no type identifier, or
  `Bytes`, fails to load as a resource.

## Important Learning

The CSV reader is the only place that knows where a record starts, so the line number has to come
from the tokenizer. It cannot be reconstructed from the record index afterwards.

## Conformance and Remaining Work

Conforms to Phases 1–4, apart from the T7 deviation. The stderr path for deserialization was
documented as out of scope in Phase 1.

## Validation

- `cargo test -p liquers-records --all-features --lib`: passed
- `cargo test -p liquers-lib --test records_end_to_end`: 7 passed
- `-R/data/short.csv/-/ns-rec/to_record-csv` validated with `liquers-validate`
- Build matrix and full lib loop are run with the wave

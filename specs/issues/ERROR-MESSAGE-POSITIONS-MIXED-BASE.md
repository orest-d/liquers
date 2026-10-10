---
id: ERROR-MESSAGE-POSITIONS-MIXED-BASE
kind: issue
title: Error messages outside command arguments mix 0-based and 1-based positions
status: draft
priority: P3
complexity: M
area: [records, core/query]
design:
created: 2026-10-10
github:
---
## Problem

User-facing error messages number positions inconsistently. Command-argument numbering is fixed by
`design/context-param-order/` (1-based); this issue is everything else, found by the same sweep on
2026-10-10:

- **Data-file positions.** `liquers-records/src/formats/csv.rs` reports 1-based lines and records
  (`RowPosition`), but `formats/ndjson.rs` (`read_table`, "record {row}") numbers records from an
  `enumerate()`, so record 1 is reported as `record 0`. `formats/shapes.rs` ("data row {row}") and
  the `field {index}` messages in `formats/ipc.rs`, `sources.rs` and `batch.rs` (`concat`) are 0-based too.
- **Query text.** `liquers-core/src/escape.rs` reports a bad character "at offset {i}", 0-based.
- **Rust API indices.** `column index {col} out of range (0..{n})`, `Column::take: index {idx}`,
  `ColumnMut::set: row {row}` and siblings in `column.rs`, `mutable.rs`, `views.rs`, `batch.rs` and
  `liquers-web/src/records.rs` echo the 0-based index the caller passed, with a 0-based range.

## Impact

Low to moderate. A user fixing a malformed NDJSON or JSON-shapes file looks at the wrong record;
CSV and NDJSON disagree about the same position. No workaround beyond knowing the convention.

## Expected behaviour

The maintainer's rule (2026-10-10, for command arguments): positions in error messages are
numbered from 1. Open question for this issue: does it also cover the Rust-API messages, which echo
an index the caller passed (`column(3)` → "index 3 out of range (0..3)")? Two options: (a) 1-based
*ordinals* for positions in user data and query text only, API indices unchanged and labelled
"index"; (b) every message 1-based. Recommended (a).

## Discovery

Sweep of `format!` messages interpolating an index variable, while scoping 1-based argument numbers
into `design/context-param-order/` (Phase 2, 2026-10-10).

---
id: ONE-BASED-ERROR-POSITIONS
kind: design
title: Positions in user data and query text are numbered from 1 in error messages
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [records, core/query]
issues: [ERROR-MESSAGE-POSITIONS-MIXED-BASE]
created: 2026-10-20
---
# Positions in user data and query text are numbered from 1 in error messages

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written on 2026-10-20. Not
implemented.

## Phase 1: High-Level Design

### Purpose

A user who fixes a malformed data file or query reads the position in the error message and goes
there. Today CSV and Markdown count records and lines from 1, while NDJSON, the JSON shapes and
schema-field checks count from 0, so the same mistake is reported one record apart depending on the
format. Apply the maintainer's rule for command arguments (positions in error messages are numbered
from 1, `design/context-param-order/`) to positions in user data and query text, and label every
remaining 0-based Rust-API index as an *index*.

### Problem Example

Verified at HEAD (18732dd) in `liquers-records/src/formats/ndjson.rs`. A two-line NDJSON file whose
**first** object has a `null` in a non-nullable `name` column, read with a declared schema:

```text
{"id": 1, "name": null}
{"id": 2, "name": "b"}
```

`read_declared_objects` numbers rows with `objects.iter().enumerate()` and `resolve_cell` reports

```text
read_table: record 0, column 'name': null is not allowed (not nullable)
```

The same table as CSV (`id,name` header, empty `name` in the first data row) reports
`CSV line 2` through `formats/csv.rs` `RowPosition`, which is 1-based. What should happen: NDJSON
says `record 1`, and so does every reader that shares `objects_to_batch`.

In query text, `liquers_core::escape::decode_token("ab:c")` reports `':' at offset 2 cannot appear
unescaped in a parameter` (a 0-based byte offset), while the parser reports the colon of the
query `f-ab:c` at `(line 1, column 5)`, 1-based (checked with `liquers-validate`).

### Scope and Acceptance Criteria

- **AC-1** NDJSON and JSON records count from 1
  WHEN a declared-schema NDJSON, `json` array or `shapes` `records` read fails on the first object (missing non-nullable column, null in a non-nullable column, a cell that does not convert, an undeclared key, or an item that is not an object)
  THEN the message says `record 1`
- **AC-2** Transposed JSON shapes count rows from 1
  WHEN a `shapes` `split` or `values` read fails on its first data row (wrong cell count, or not an array)
  THEN the message says `data row 1` / `row 1`
- **AC-3** Schema-field positions count from 1
  WHEN an embedded IPC `liquers.schema`, a manifest chunk schema, or `RecordBatch::concat` disagrees at the first field
  THEN the message says `field 1`
- **AC-4** Query-text positions are 1-based columns
  WHEN `escape::decode_token` or `escape::segments` rejects the character at the third position of a token
  THEN the message says `at column 3`, counting characters, as the parser's `(line, column)` does
- **AC-5** API indices are labelled, not renumbered
  WHEN a Rust API is called with an out-of-range 0-based index (`RecordBatch::row_id(5)` on 3 rows, `Column::take`, `ColumnMut::set`, a view's `row_number`)
  THEN the message keeps the index the caller passed and calls it an index (`row index 5 out of range (0..3)`)
- **AC-6** CSV and Markdown are unchanged
  WHEN the existing CSV and Markdown position tests run
  THEN they pass unmodified

Non-goals: `Position` itself (0-based byte `offset`, 1-based `line`/`column`, DOC-02 §Identity and
positions) is not changed. Generated *names* that contain a number — `c0`, `col0`, `index_0`,
manifest chunk keys `_0000` — are data, not messages, and stay 0-based. NDJSON does not gain a
line number beside the record number. Command-argument numbering is already done
(`design/context-param-order/`, complete).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — the change spans three crates (`liquers-records`,
  `liquers-core`, `liquers-lib`; rule 6), and it carries an open decision (rule 5). Once the
  decision is made, the `liquers-records` part alone (Steps 1-4) is eligible as its own fix.
- **Leading issue:** **Proposed resolution — scope of the rule.** The issue's open question: does
  "number from 1" cover the Rust-API messages that echo an index the caller passed? Recommended
  answer (a), which this design assumes: 1-based *ordinals* for positions in user data and query
  text; API indices keep the caller's 0-based value and are labelled "index". Option (b), every
  message 1-based, would print `row 6 out of range (1..3)` for `row_id(5)`, a number the caller never
  wrote.
- **Explanation:** every message site was found by a sweep of `format!` strings interpolating an
  index variable at HEAD; each change is a message string, and the two numbering conventions the
  messages now use are each stated once (AC-1-AC-4, AC-5).
- **Open questions:**
  1. **Proposed resolution — chunk numbers.** `chunk {index}` in `sources.rs` `name_chunk` and in
     `ns-rec/rowid` is the chunk's position *and* its identity: it appears in a `rec_id` and in the
     generated chunk key `_0000`. Recommended: keep it 0-based and label it `chunk index`, like an
     API index, so the message matches the key it names.

### Design Dependencies

- `context-param-order` (complete): **origin** of the rule; not extended (E4).
- `ipc-reader-polars-text-columns` (`in_review`): **overlaps** — it edits `formats/ipc.rs` readers
  but not `check_embedded_schema` or the offsets check touched here. No ordering constraint.

### Consolidated Findings

- Every change is a message string; no type, signature, `ErrorType` or registry change.
- Two conventions result, each stated once: positions in user data and query text are 1-based
  ordinals (`record 1`, `field 1`, `column 3`); values a Rust caller passed stay as passed and are
  called *indices*. Chunk numbers are identities and follow the second rule.
- Generated names (`c0`, `col0`, `index_0`, `_0000`) are data and must not be touched; the sweep
  that found the sites also matches them.
- One existing test changes (`chunk 0 (` in `sources.rs`). CSV and Markdown already conform.
- The decision (a) vs (b) is the only open item; Steps 1-4 are safe under either answer except
  Step 4, which (b) would replace with renumbering.

## Phase 2: Architecture

### Solution

Change message strings only; no signature, type or error-type changes. Each user-data or
query-text position adds 1 at the `format!` site (`row + 1`, `index + 1`). Each API index keeps its
value and gains the word `index`. Rejected: a shared `Ordinal` newtype or helper. The sites are
independent one-line format strings and a helper adds an abstraction for one `+ 1`. Rejected:
renumbering API indices (option b, see Leading issue).

No open issue blocks this. `ERROR-WITH-KEY-SETS-QUERY-FIELD` (`error-with-key-field`) restructures
error *fields*, not these messages.

### Changes

User-data positions → 1-based (`liquers-records`):

| File | Symbol | Message |
|---|---|---|
| `formats/ndjson.rs` | `resolve_cell` (3 messages), `declared_row_values`, `objects_to_batch_ordered` | `record {row}` → `record {}` with `row + 1` |
| `formats/shapes.rs` | `from_json_split`, `from_json_values` | `data row {row}`, `split.data row {row}`, `values row {row}`, `row {row} has`, and the literal `"values row 0"` → 1-based |
| `formats/ipc.rs` | `check_embedded_schema` | `its field {index}` → `index + 1` |
| `formats/ipc.rs` | the offsets check at line 1164 (`offsets decrease at row {row}`) | → `row + 1`; it describes a row of the file being read |
| `sources.rs` | `check_view_matches_schema` | `chunk field {index}` → `index + 1` |
| `batch.rs` | `RecordBatch::concat` (2 messages) | `schemas differ at field {index}` → `index + 1` |

Query-text positions → 1-based columns (`liquers-core/src/escape.rs` `segments`): track a character
count beside the byte offset `i` and report `at column {n}`. These are the only two messages in the
module that carry a position. The parser's own entity errors already report through `Position`.

API indices → labelled, value unchanged:

| File | Messages |
|---|---|
| `liquers-records/src/batch.rs` | `row_id`, `row_number`: `row {row}` → `row index {row}` |
| `liquers-records/src/views.rs` | `RowRangeView`, `RowIndexView` (`row_id`, `row_number`), `PlacedView::{who}`: `row {row}` → `row index {row}` |
| `liquers-records/src/mutable.rs` | `ColumnMut::set`: `row {row}` → `row index {row}` |
| `liquers-records/src/column.rs` | `Column::validate` `offsets decrease at row {index}`, `offsets of cell {i}`, `Vector data too short for row {idx}` / `{i}` → `row index` / `cell index` |
| `liquers-records/src/search/kernels.rs`, `search/rank.rs` | `column {index} is unavailable`, `row {row} is beyond` → `column index`, `row index` |
| `liquers-records/src/sources.rs` | `name_chunk`: `chunk {index} (` → `chunk index {index} (` |
| `liquers-lib/src/records/commands.rs` | `chunk_id_at` (2 messages): `chunk {index}` → `chunk index {index}` |

Messages that already say `index` (`column index {col} out of range`, `Column::take: index`,
`RecordView::take: index`, `liquers-web/src/records.rs`) are already correct and unchanged.

Documents: none are normative for these messages. `specs/reference/RECORD_STREAMS.md` (if it
quotes a message) is checked during implementation. No `affects_docs` beyond the issue.

### Risks

| Assessment | Finding |
|---|---|
| Files likely to change | the eleven files above |
| Existing tests likely to change | `sources.rs` test asserting `chunk 0 (ns-sql/...)` (becomes `chunk index 0 (`); any test matching `record 0` or `at offset` (none found by grep at HEAD) |
| Compatibility | message text only; no `ErrorType` changes. A caller matching message text breaks, and none in the workspace does apart from the test above |
| Performance | none; `segments` gains one counter |
| Recovery | revert per file; the sites are independent |
| Certainty | high. The sweep regex is recorded in the issue; a site it missed stays as today and is found by AC tests only if it is in a tested reader |

## Phase 3: Examples and Tests

### Examples

```text
read_table: record 1, column 'name': null is not allowed (not nullable)        (was: record 0)
from_json: split: data row 1 has 2 cells, but columns declares 3                (was: data row 0)
ipc: the embedded liquers.schema does not describe this file: its field 1 is …  (was: field 0)
':' at column 3 cannot appear unescaped in a parameter; write it as an entity … (was: offset 2)
RecordBatch::row_id: row index 5 out of range (0..3)                            (was: row 5)
```

### Tests

All are new unit tests in the named file's existing `mod tests`, each asserting `contains` on the
message (not the whole text):

- `liquers-records/src/formats/ndjson.rs` — `first_record_is_reported_as_record_1` (AC-1): the
  problem example's NDJSON with a declared non-nullable `name`; also an undeclared key and a
  non-object first item, each `record 1`.
- `liquers-records/src/formats/shapes.rs` — `split_and_values_rows_count_from_1` (AC-1, AC-2): a
  `split` document whose first data row is short (`data row 1`); a `values` document whose first
  row is not an array (`values row 1`); a `records` orient whose first item has a null in a
  non-nullable column (`record 1`).
- `liquers-records/src/batch.rs` — `concat_reports_field_1_and_row_index` (AC-3, AC-5): concat two
  batches whose first fields differ (`field 1`); `row_id(5)` on three rows (`row index 5`).
- `liquers-records/src/sources.rs` — `chunk_field_mismatch_counts_from_1` (AC-3) and the updated
  existing `chunk 0` assertion (`chunk index 0`).
- `liquers-records/src/formats/ipc.rs` — `embedded_schema_mismatch_names_field_1` (AC-3), under
  `#[cfg(feature = "ipc")]` like the module.
- `liquers-records/src/views.rs` — `row_index_messages_keep_the_callers_index` (AC-5): a
  `RowRangeView` `row_number(9)`.
- `liquers-core/src/escape/tests.rs` — `unescaped_character_reports_one_based_column` (AC-4):
  `decode_token("ab:c")` says `at column 3`; `decode_token("aé:")` says `at column 3` (characters,
  not bytes).
- AC-6: the existing CSV (`csv.rs` `CSV line 4 (record 2)`) and Markdown tests, unmodified.

```bash
cargo test -p liquers-records --lib
cargo test -p liquers-records --features ipc --lib formats::ipc
cargo test -p liquers-core --lib escape
cargo test -p liquers-lib --lib records
```

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-records/src/formats/ndjson.rs` `resolve_cell`, `declared_row_values`, `objects_to_batch_ordered` — `row + 1` in the five messages; add `first_record_is_reported_as_record_1` — `cargo test -p liquers-records --lib ndjson`
- [ ] 2. `liquers-records/src/formats/shapes.rs` `from_json_split`, `from_json_values` — 1-based rows, including the literal `"values row 0"`; add `split_and_values_rows_count_from_1` — `cargo test -p liquers-records --lib shapes`
- [ ] 3. `liquers-records/src/formats/ipc.rs`, `sources.rs`, `batch.rs` — schema-field messages `index + 1`, the IPC offsets row `+ 1`; add the AC-3 tests — `cargo test -p liquers-records --lib && cargo test -p liquers-records --features ipc --lib formats::ipc`
- [ ] 4. `liquers-records/src/{batch,views,mutable,column}.rs`, `search/{kernels,rank}.rs`, `sources.rs` `name_chunk` — add `index` to the API-index messages; update the `chunk 0 (` test; add `row_index_messages_keep_the_callers_index` — `cargo test -p liquers-records --lib`
- [ ] 5. `liquers-core/src/escape.rs` `segments` — count characters and report `at column {n}`; add `unescaped_character_reports_one_based_column` — `cargo test -p liquers-core --lib escape`
- [ ] 6. `liquers-lib/src/records/commands.rs` `chunk_id_at` — `chunk index {index}` in both messages — `cargo test -p liquers-lib --lib records`
- [ ] 7. Close `ERROR-MESSAGE-POSITIONS-MIXED-BASE` with a resolution naming the decision taken; record the rule in `specs/reference/RECORD_STREAMS.md` only if it documents error positions (check during this step) — `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-records --all-features --lib --tests`, `cargo test -p liquers-core --lib`,
`cargo test -p liquers-lib --lib --tests`. No `cfg(feature)` or dependency changes, so the build
matrix is not required. No generated file changes (`command_registry.yaml` is untouched: no
signature changes). Final diff review: message strings and tests only.

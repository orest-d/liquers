---
id: POLARS-BRIDGE-VECTOR-COLUMNS
kind: design
title: The RecordBatch <-> DataFrame bridge converts Vector columns as polars Array(Float32, dim)
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [records, lib/polars]
issues: [POLARS-BRIDGE-VECTOR-COLUMNS-REFUSED]
created: 2026-10-21
---
# The RecordBatch <-> DataFrame bridge converts Vector columns as polars Array(Float32, dim)

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented.

## Phase 1: High-Level Design

### Purpose

Let a table with an embedding (`FieldType::Vector`) column cross between records and polars in both
directions, instead of failing the whole conversion because of one column.

### Problem Example

`data/emb.feather` holds `id: Text`, `embedding: Vector` (dim 3). The query
`-R/data/emb.feather/-/ns-rec/to_record/ns-pl/head-5` (validated: Ok) reaches
`try_to_polars_dataframe` (`liquers-lib/src/polars/util.rs`), which calls `record_batch_to_dataframe`
(`liquers-lib/src/records/polars.rs`); `column_to_series` returns
`NotSupported: polars bridge: column 'embedding' is Vector …`. The reverse,
`…/ns-pl/head-5/ns-rec/to_record` (validated: Ok), would refuse an `Array` dtype in
`polars_dtype_to_field_type`. The `polars` dependency (`liquers-lib/Cargo.toml`) does not enable
`dtype-array`, so `DataType::Array` cannot be built. Should: `embedding` becomes a polars
`Array(Float32, 3)` column, null rows null, and comes back as `Vector` with `dim` 3.

### Scope and Acceptance Criteria

- **AC-1** Records to polars
  WHEN a batch with a `Vector` column of `dim` d (with nulls) is converted by `record_batch_to_dataframe`
  THEN the DataFrame has an `Array(Float32, d)` column with the same values and nulls in the same rows
- **AC-2** Polars to records
  WHEN a DataFrame with an `Array(Float32, d)` column is converted by `dataframe_to_record_batch`
  THEN the batch has a `Vector` field of `dim` d with the same values and nulls
- **AC-3** Round trip through a query
  WHEN `-R/data/emb.feather/-/ns-rec/to_record/ns-pl/head-5/ns-rec/to_record` runs
  THEN the result equals the first five rows of the stored table, `Vector` field included
- **AC-4** Other array element types
  WHEN the DataFrame column is `Array(Float64, d)` or an integer `Array`
  THEN it is refused naming the column and the dtype (Q2), as is a variable-length `List`
- **AC-5** Edge widths
  WHEN `dim` is 0, or every row is null
  THEN the conversion succeeds both ways with the row count kept
- **AC-6** Declared schema
  WHEN a Parquet file read through polars with a declared `Vector` field holds a matching `Array` column
  THEN `read_parquet_record_batch` reads it, instead of refusing it as today

Non-goals: Parquet writing of `Vector` (`liquers-records` writer, separate limitation); `List`
columns; a polars command for vectors.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — enables a new feature of a dependency (rule 4).
- **Leading issue:** Q1, whether to add `dtype-array`.
- **Explanation:** the mapping is direct (Arrow `FixedSizeList<Float32>` is polars' `Array`), and
  `ColumnMut` already builds `Vector` columns; only the dependency choice is the maintainer's.
- **Open questions:**
  1. **Open design - dependency:** enable polars `dtype-array` (recommended: a polars-core feature
     gate, no new crate; it is also what lets polars read Arrow/Parquet `FixedSizeList` files) or
     document the refusal as permanent in `RECORD_STREAMS.md`. Implementation step 1 records
     `cargo tree -e features -i polars-core` before and after, so the cost is measured.
  2. **Proposed resolution - element type:** accept `Float32` only and refuse other element types
     naming the dtype (recommended: `Vector` is `f32`; a silent `f64 → f32` narrowing would change
     values). A caller casts in polars first. Alternative: cast `Float64` with a warning.

### Design Dependencies

- `polars-dataframe-ipc` — **overlaps**: with `dtype-array` on, a DataFrame holding an `Array`
  column can also be written as IPC by that design's codec; no order.
- `parquet-read-declared-schema` (complete, E4) — **overlaps**: AC-6 extends what its declared
  read accepts (a present `Vector` column); not extended.
- `ipc-reader-polars-text-columns` — **overlaps** (same area, separate reader); no order.

### Consolidated Findings

- Both directions live in `liquers-lib/src/records/polars.rs`; the existing test
  `bridge_refuses_vector_columns_naming_the_column` is replaced by AC-1/AC-2 tests.
- Null rows: records store `dim` zeros under a clear validity bit; polars' outer validity carries the
  null, inner values are ignored.
- The feature flag is additive: no `#[cfg]` beyond the existing `polars` gate, but the build matrix
  must be rerun because a polars feature changes compile time and size.
- `RECORD_STREAMS.md` bridge row ("`Vector` columns are refused") and the Interop limitation change;
  `POLARS_COMMAND_LIBRARY.md` gains a note that `Array` columns pass through.

## Phase 2: Architecture

### Solution

`liquers-lib/Cargo.toml`: add `"dtype-array"` to the `polars` features (Q1).

`record_batch_to_dataframe` → `column_to_series`, `RecColumn::Vector { validity, dim, data }`: build a
`Float32` series from `data.as_slice()`, reshape to `Array(Float32, dim)` (polars
`Series::reshape_array` with dimensions `[rows, dim]`; for `dim == 0` construct the empty-width array
through `ArrayChunked` directly), then apply the row validity as the outer validity. The exact polars
0.55 constructor is chosen in step 2 against the compiled API; the contract is AC-1.

`dataframe_to_record_batch` → `polars_dtype_to_field_type`: `DataType::Array(inner, d)` with inner
`Float32` → `FieldType::Vector`; any other inner → `Error::not_supported` naming column and dtype.
`append_series_values`'s `FieldType::Vector` arm: iterate rows of `series.array()?`; a null row pushes
`FieldValue::Null`, a present row pushes `FieldValue::Vector` of its `f32` values (`ColumnMut` fixes
`dim` and backfills earlier nulls, which gives AC-5 for leading nulls; an all-null column takes `d`
from the dtype through `ColumnMut::with_capacity` plus an explicit width — added as a private helper
if `ColumnMut` cannot be told the width).
`dataframe_to_declared_columns` reads a present `Array` column through the same arm; its refusal of a
*missing* declared `Vector` field (no width to read nulls as) stays.

Rejected: mapping through `List(Float32)` (no fixed width, needs per-row checks; still needs a feature
for building lists from offsets).

Known-issue preflight: none blocks. Namespaces involved: `pl`, `rec` (behaviour only).

### Changes

No signature changes. Files: `liquers-lib/Cargo.toml`; `liquers-lib/src/records/polars.rs`
(`column_to_series`, `polars_dtype_to_field_type`, `append_series_values`,
tests). Documents as in Consolidated Findings.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| `dtype-array` adds compile time / size | measured in step 1; matrix run | medium |
| polars 0.55 API for building `Array` differs from the sketch | contract fixed by AC-1/AC-2; API chosen in step 2 | medium |
| All-null column width unknown on the records side | width from dtype; AC-5 test | high |
| Behaviour change: a frame that failed now converts | intended; AC-4 keeps refusals explicit | high |

## Phase 3: Examples and Tests

### Examples

`-R/data/emb.feather/-/ns-rec/to_record/ns-pl/head-5` returns a 5-row DataFrame whose `embedding`
dtype is `array[f32, 3]`.

### Tests

`liquers-lib/src/records/polars.rs` `mod tests` (feature `polars`):

- `vector_column_becomes_a_float32_array_series` — AC-1
- `float32_array_series_becomes_a_vector_column` — AC-2
- `float64_array_and_list_columns_are_refused_naming_them` — AC-4
- `zero_width_and_all_null_vectors_cross_both_ways` — AC-5
- `declared_vector_field_reads_an_array_column` — AC-6

`liquers-lib/tests/records_ipc_polars.rs` (gated `records-ipc` + `polars`):
`vector_column_round_trips_through_ns_pl_head` — AC-3 (stores a Liquers-written `emb.feather`).

Run: `cargo test -p liquers-lib --lib records::polars` and
`cargo test -p liquers-lib --test records_ipc_polars`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-lib/Cargo.toml` — `dtype-array`; record `cargo tree -p liquers-lib -e features -i polars-core` before/after in the PR — `cargo build -p liquers-lib`. Rollback: remove the feature.
- [ ] 2. `polars.rs` `column_to_series` Vector arm — AC-1, AC-5 tests — `cargo test -p liquers-lib --lib records::polars`. Depends on 1.
- [ ] 3. `polars.rs` `polars_dtype_to_field_type`, `append_series_values` — AC-2, AC-4, AC-6 tests; remove `bridge_refuses_vector_columns_naming_the_column` — same command.
- [ ] 4. `tests/records_ipc_polars.rs` — AC-3 — `cargo test -p liquers-lib --test records_ipc_polars`.
- [ ] 5. `RECORD_STREAMS.md`, `POLARS_COMMAND_LIBRARY.md`, History, `reviewed:`; close `POLARS-BRIDGE-VECTOR-COLUMNS-REFUSED` — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-lib --lib --tests`;
`cargo test -p liquers-lib --no-default-features --features polars,records --lib --tests`;
`bash scripts/check-build-matrix.sh`; diff review limited to the bridge, the manifest and documents.

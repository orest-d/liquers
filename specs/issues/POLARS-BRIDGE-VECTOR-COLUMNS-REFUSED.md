---
id: POLARS-BRIDGE-VECTOR-COLUMNS-REFUSED
kind: issue
title: The RecordBatch <-> DataFrame bridge cannot convert Vector columns
status: draft
priority: P3
complexity: M
area: [records, lib/polars]
design:
created: 2026-09-27
github:
---

## Problem

`liquers-lib/src/records/polars.rs`'s `record_batch_to_dataframe` and `dataframe_to_record_batch`
(Phase 4 Step 6.2) refuse `Column::Vector` / any dtype that would need one, naming the column. A
`FixedSizeList` (Arrow's `Vector` layout, `dim` floats per row) has no direct match in the `polars`
dependency's enabled feature set — `liquers-lib/Cargo.toml`'s `polars` entry enables
`["lazy", "temporal", "csv", "parquet", "ipc"]`, not `dtype-array` — so building or reading a
`DataType::Array(Float32, dim)` column is not currently possible without adding that feature.

This mirrors the Parquet writer's own `Vector` refusal (`liquers-records/src/formats/parquet.rs`,
"Parquet's `LIST` needs repetition levels this writer does not produce"), but is a separate
limitation: even a `RecordBatch` that never touches Parquet — going only through the in-memory
bridge to a polars `DataFrame` and back — cannot carry an embedding/vector column today.

## Impact

Low: no current caller needs vector columns through this bridge (the embedding/vector-search use
case this crate's `Vector` field type exists for is not yet wired to polars anywhere). But it is a
real gap should a future command need to hand embeddings to polars (e.g. for a similarity join or
export), and the refusal is easy to hit by surprise if a schema happens to include a `Vector` field
alongside ordinary columns — the whole `DataFrame`/`RecordBatch` conversion fails, not just that
column.

## Expected behaviour

Add the `dtype-array` feature to `liquers-lib`'s `polars` dependency and implement the two
directions for `Column::Vector <-> DataType::Array(Float32, dim)`, or, if the feature is judged not
worth the added dependency weight, document the refusal in `specs/design/record-streams/` (or a
reference doc) as a scoped, permanent limitation rather than leaving it explained only in code
comments and this issue.

## Discovery

Noted while implementing the bridge (Phase 4 Step 6.2, `specs/design/record-streams/
phase4-implementation.md`): both `record_batch_to_dataframe` and `dataframe_to_record_batch` refuse
`Vector`/unmapped list dtypes by construction, and a test
(`liquers-lib/src/records/polars.rs`'s `bridge_refuses_vector_columns_naming_the_column`) exists to
pin that refusal rather than a crash. Filed per CLAUDE.md's "record what you find" rule rather than
leaving the gap undocumented outside the code.

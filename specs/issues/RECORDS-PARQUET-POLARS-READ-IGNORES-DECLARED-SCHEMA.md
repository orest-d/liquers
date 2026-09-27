---
id: RECORDS-PARQUET-POLARS-READ-IGNORES-DECLARED-SCHEMA
kind: issue
title: Reading Parquet through the polars bridge ignores a caller-declared schema
status: draft
priority: P2
complexity: M
area: [records, lib/polars]
design:
created: 2026-09-27
github:
---

## Problem

Every other table format's reader (`liquers-records/src/formats/{csv,ndjson,markdown,ipc}.rs`)
takes a `ReadSchema` — `Declared(&RecordSchema)` or `Infer` — and, when a schema is declared, reads
strictly against it (phase2-architecture.md §"Two readers: schema-aware and schema-less": "nothing
is guessed"). Parquet's own reading path,
`liquers-lib/src/records/polars.rs::read_parquet_record_batch`, also takes a `ReadSchema` (to keep
the same signature every caller — `records::read_parquet_record_batch`,
`records::convert::read_table_from_bytes`, `value/mod.rs`'s `deserialize_from_bytes` — already
uses), but silently ignores it: polars' `ParquetReader` infers its own schema from the file, and the
`RecordSchema` a caller declared (e.g. `ns-rec/*` commands' `schema` option,
`ToRecordOptions::schema`) is never checked against, or used to guide, that read.

## Impact

Low-to-moderate: a `ReadSchema::Declared(..)` argument to a Parquet read is accepted but has no
effect, which is surprising given every sibling reader honors it strictly. Concretely: a caller
supplying a schema to coerce column types (e.g. treating a numeric column as `Text`, or supplying
roles/labels/an `Id` field) gets none of that — the schema recovered from a Parquet read is always
whatever `dataframe_to_record_batch`'s built-in dtype mapping produces
(`polars_dtype_to_field_type`), with every field `KeyRole::None` and default `FieldRole`.

## Expected behaviour

One of:
- After `dataframe_to_record_batch` builds its inferred `RecordBatch`, cross-check field count and
  types against a `Declared` schema the way `formats/ipc.rs::effective_schema` does for Arrow IPC,
  refusing on a mismatch and substituting the declared schema's roles/labels/`Id` on success.
- If schema-aware Parquet reading is out of scope for now, say so explicitly in
  `read_parquet_record_batch`'s doc comment (not just "not cross-checked" in passing) and in
  whichever reference document (`specs/reference/`) describes what a `RecordView` reader does with
  `ReadSchema` per format, so a caller does not have to read the source to learn this.

## Discovery

Noted while implementing `liquers-lib/src/records/polars.rs`'s `read_parquet_record_batch` (Phase 4
Step 6.2, `specs/design/record-streams/phase4-implementation.md`): the function accepts `schema:
ReadSchema<'_>` for signature parity with `liquers_records::read_table`, then discards it
(`_schema`) because polars' `ParquetReader` has no schema-aware read path exposed here. Filed per
CLAUDE.md's "record what you find" rule rather than leaving the parameter's dead effect undocumented
outside a one-line code comment.

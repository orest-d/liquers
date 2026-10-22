---
id: RECORDS-PARQUET-POLARS-READ-IGNORES-DECLARED-SCHEMA
kind: issue
title: Reading Parquet through the polars bridge ignores a caller-declared schema
status: closed
priority: P2
complexity: M
area: [records, lib/polars]
design: parquet-read-declared-schema
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

## Problem Example

```rust
// bytes: Parquet written by Liquers, schema id: Int (Id, not null), amount: Float
let wrong = RecordSchema::new(vec![
    FieldSchema::new("id", FieldType::Int), FieldSchema::new("amount", FieldType::Text)])?;
read_parquet_record_batch(&bytes, ReadSchema::Declared(&wrong))   // Ok: `amount` reads as Float
read_parquet_record_batch(&bytes, ReadSchema::Infer)?.schema().id_field()   // None: Id lost
```

Expected: the first is an `Err` naming `amount`, `Float` and `Text` (as the IPC reader does); the
second restores the embedded `liquers.schema`, which `record-streams` Phase 2 promised for
Liquers-written Parquet and which polars exposes as `ParquetReader::get_metadata()?.key_value_metadata`.
The design covers both, since they share the function and the schema rules.

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

## Resolution

Fixed 2026-10-20 by `design/parquet-read-declared-schema/`.
`liquers_lib::records::polars::read_parquet_record_batch` now reads the file's key-value metadata
before the frame. `ReadSchema::Declared` is strict, as in the Arrow IPC reader: columns are matched
by name, an undeclared column or a type mismatch is an error naming it, nothing is cast, and a
missing nullable field reads as nulls. `ReadSchema::Infer` restores the embedded `liquers.schema`
when it still describes the columns, and otherwise derives the schema from the polars dtypes. A
not-null field holding a null is refused on both paths. Evidence: the `parquet_*` and
`to_record_parquet_with_schema_option_is_strict` tests in
`liquers-lib/tests/records_parquet_polars.rs`.

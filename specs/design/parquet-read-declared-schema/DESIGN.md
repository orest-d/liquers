---
id: PARQUET-READ-DECLARED-SCHEMA
kind: design
title: Parquet read through polars honours the declared and embedded schema
form: compact
status: complete
gh_pr: [169]
readiness: ready
autofix: eligible
area: [records, lib/polars]
issues: [RECORDS-PARQUET-POLARS-READ-IGNORES-DECLARED-SCHEMA]
created: 2026-10-19
---
# Parquet read through polars honours the declared and embedded schema

> **Complete on 2026-10-20** (bulk design review): implemented by #169 (commit 6ee35f9), merged; every source issue is closed and no phase is outstanding.

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written; Phase 4 implemented on
2026-10-20 (automatic fix).

## Phase 1: High-Level Design

### Purpose

`ReadSchema::Declared` means "schema-aware and strict, nothing guessed" for every table reader
(`specs/reference/RECORD_STREAMS.md` §Table formats). The Parquet reader in the polars bridge
accepts the argument and then ignores it. It also ignores the `liquers.schema` that Liquers' own
Parquet writer embeds. Make Parquet reads follow the same schema rules as the Arrow IPC reader.

### Problem Example

At HEAD, `liquers-lib/src/records/polars.rs::read_parquet_record_batch(bytes, _schema)` discards
`_schema`:

```rust
// Written by Liquers: `id` is the Id field and `amount` is not null, and `liquers.schema` is embedded.
let schema = RecordSchema::new(vec![
    FieldSchema::new("id", FieldType::Int).with_key(KeyRole::Id).not_null(),
    FieldSchema::new("amount", FieldType::Float),
])?;
let bytes = write_table(&batch, TableFormat::Parquet, &WriteOptions::default())?;

// 1. Declared schema with a wrong type: today Ok(batch), and `amount` reads as Float.
let wrong = RecordSchema::new(vec![
    FieldSchema::new("id", FieldType::Int), FieldSchema::new("amount", FieldType::Text)])?;
read_parquet_record_batch(&bytes, ReadSchema::Declared(&wrong));
// 2. Infer: today the schema has no Id field, and `id` is nullable.
read_parquet_record_batch(&bytes, ReadSchema::Infer)?.schema().id_field();   // None
```

Expected: (1) is an `Err` naming `amount`, `Float` and `Text`, the same rule `formats/ipc.rs::resolve_declared`
applies. (2) returns the embedded schema, so the `Id` and the `not null` are kept. That matches
`specs/design/record-streams/phase2-architecture.md` §"Two readers": "`feather`, `parquet` written by
Liquers | **schema-aware** | the file's own metadata (`liquers.schema`)". Its Tier 3 says roles are
lost "unless polars exposes the key-value metadata". polars 0.55.2 exposes it as
`ParquetReader::get_metadata()?.key_value_metadata`.

### Scope and Acceptance Criteria

- **AC-1** Declared schema is the result's schema
  WHEN a Parquet file is read with `ReadSchema::Declared(s)` and every file column is in `s` with the same `FieldType`
  THEN the batch has exactly `s` (field order, roles, labels, `Id`, nullability), with columns matched **by name**
- **AC-2** Declared type mismatch is refused
  WHEN a declared field's type differs from the type the file's column bridges to
  THEN the read is an `Err` naming the field and both types, and nothing is cast
- **AC-3** Undeclared column is refused
  WHEN the file has a column that `s` does not declare
  THEN the read is an `Err` naming that column
- **AC-4** Declared field missing from the file
  WHEN a nullable declared field is absent from the file
  THEN it reads as an all-null column. If the field is non-nullable, or a `Vector`, the read is an `Err` naming it
- **AC-5** Declared non-null is enforced
  WHEN a field declared not null holds a null in the file
  THEN the read is an `Err` naming the field and the null count
- **AC-6** Embedded schema restored on Infer
  WHEN a Parquet file written by Liquers is read with `ReadSchema::Infer`
  THEN the result's schema equals the written `RecordSchema`, including roles, labels, the `Id` field and `UInt`
- **AC-7** Stale or foreign metadata is ignored
  WHEN `ReadSchema::Infer` reads a file without `liquers.schema`, or with one that does not parse, or one whose names or types do not match the file's columns in order
  THEN the schema is derived from the polars dtypes, as today
- **AC-8** Callers see it through `to_record`
  WHEN `ns-rec/to_record` reads Parquet bytes with a `schema` argument
  THEN AC-1 to AC-5 apply. Deserializing a `RecordView` from Parquet (`value/mod.rs`, `Infer`) gets AC-6

Non-goals: casting to a declared type; a polars-free Parquet reader; Parquet chunks in a manifest
(`read_table` still refuses Parquet); `Vector` columns (`POLARS-BRIDGE-VECTOR-COLUMNS-REFUSED`).

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible. This is a bug fix to documented behaviour, confined to private
  helpers in `liquers-lib/src/records/polars.rs` and tests in `liquers-lib`. No `pub` signature
  changes (only the parameter's name, `_schema` → `schema`), and there is no new type, command,
  dependency or feature.
- **Leading issue:** None
- **Explanation:** both rules already exist (the reference's "strict, nothing guessed"; IPC's
  `resolve_declared` / `check_embedded_schema`), and polars exposes the key-value metadata.
- **Open questions:** None.
- **Resolved from evidence:** a type mismatch is refused, not cast. This matches the IPC reader and
  `RECORD_STREAMS.md` ("strict, nothing guessed"). The embedded `liquers.schema` is in scope: it is
  in the same function, it uses the same comparison, and the origin design already promised it.

### Design Dependencies

- `record-streams`: the **origin** design (complete and frozen, E4). It made the Tier 3 decision that
  Parquet is read through polars, which this design keeps.
- `ipc-reader-polars-text-columns`: **overlaps** (weak; it touches `formats/ipc.rs`, not this file).
  There is no ordering between the two.
- `POLARS-BRIDGE-VECTOR-COLUMNS-REFUSED` (issue): **overlaps**. Both designs touch the same bridge
  file, but in independent functions.

### Consolidated Findings

- The issue is still open at HEAD. `read_parquet_record_batch` has `_schema`, and its doc comment
  and `RECORD_STREAMS.md` §Parquet both say the declared schema and `liquers.schema` are ignored.
- The fix lives entirely in `liquers-lib`. The IPC reader's rules are private to `ipc.rs`, so
  they are re-stated here, not shared (sharing would add `pub` API to `liquers-records`).
- Behaviour change: a mismatching declared schema now fails where it used to be silently ignored,
  as documented. Only `to_record`'s `schema` option passes `Declared` for Parquet. No registry change.

## Phase 2: Architecture

### Solution

In `read_parquet_record_batch`, read the metadata first, then the frame, and then pick the target
schema the way `ipc.rs::effective_schema` does:

1. `let mut reader = ParquetReader::new(Cursor::new(bytes));` then
   `let embedded = embedded_liquers_schema(reader.get_metadata()?)`. This finds the
   `key_value_metadata` entry whose `key == "liquers.schema"` and parses it with
   `serde_json::from_str::<RecordSchema>`. A parse failure gives `None` (AC-7). Then
   `let df = reader.finish()?`.
2. `Declared(s)`: call `dataframe_to_declared_batch(&df, s)`. Every df column must be named in `s`
   (AC-3). For each field of `s` in order, look up `df.column(name)`. If it is present, check
   `polars_dtype_to_field_type(dtype) == field.data_type` (AC-2) and append its values with the
   existing `append_series_values`. If it is absent, push `FieldValue::Null` `height` times into a
   `ColumnMut`. A non-nullable or `Vector` field refuses instead (AC-4). Finally, call
   `RecordBatch::new(Arc::new(s.clone()), …)`.
3. `Infer`: build `dataframe_to_record_batch(&df)` as today. If `embedded` is `Some(e)` and `e`
   describes the inferred batch's fields (same count, and the same name and `FieldType` at every
   position, as in `ipc.rs::check_embedded_schema`), rebuild the batch with schema `e` and the same
   columns (AC-6). Otherwise keep the inferred batch (AC-7). The columns already have the right
   types, so this costs one `RecordBatch::new`.
4. On both paths, after the schema is fixed, every field that is not nullable must satisfy
   `column.null_mask().count_ones() == 0` (AC-5). A file written by Liquers marks such columns
   `REQUIRED`, so a valid file always passes.

Errors: `Error::general_error` for mismatches (IPC's message shapes, prefix `parquet:`),
`Error::not_supported` for the missing `Vector` field, polars errors as today. Sync; no `unwrap`.

Rejected alternatives:

- Making `ipc.rs`'s `resolve_declared` / `check_embedded_schema` `pub`: reuse, but new `pub` API
  in `liquers-records`; worth it once a third reader needs it.
- Casting to the declared type: contradicts "nothing guessed" and the IPC reader.
- `ParquetReader::with_arrow_schema_projection`: projects, does not validate, carries no roles.
- The issue's documentation-only option.

Known-issue preflight: `POLARS-BRIDGE-VECTOR-COLUMNS-REFUSED` (independent); the designs that
touched this file are complete. Nothing blocks.

### Changes

`liquers-lib/src/records/polars.rs`:

- `read_parquet_record_batch(bytes: &[u8], schema: ReadSchema<'_>) -> Result<RecordBatch, Error>`.
  The signature is unchanged. Rename the parameter from `_schema`, and rewrite the doc comment to
  state the rules above.
- New private helpers:
  `fn embedded_liquers_schema(metadata: &FileMetadataRef) -> Option<RecordSchema>`,
  `fn dataframe_to_declared_batch(df: &DataFrame, declared: &RecordSchema) -> Result<RecordBatch, Error>`,
  `fn embedded_schema_describes(schema: &RecordSchema, batch: &RecordBatch) -> bool`,
  `fn check_not_null(batch: &RecordBatch) -> Result<(), Error>`.
  `FileMetadataRef` is in `polars::prelude` (polars-io's `parquet::metadata::*`).
- The module doc comment: replace "Roles are lost on this path" with the embedded-schema rule.

`liquers-lib/src/records/mod.rs::read_parquet_record_batch` doc comment: one sentence pointing at
the rules. There are no changes in `liquers-records`, in commands or in the registry.

Documents: `specs/reference/RECORD_STREAMS.md` §Parquet ("It ignores a declared schema … and
`liquers.schema`" becomes the rule) and its §Table formats row "roles lost when read back"
(restored for Liquers-written files). Add a History row and bump `reviewed:`.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| `get_metadata` then `finish` re-reads the footer | `get_metadata` caches it; `finish` reuses it (polars-io 0.55.2) | high |
| `FileMetadataRef` / `KeyValue` path is not in `polars::prelude` | fall back to `polars::io::parquet::…`; a compile error shows it at step 2 | medium |
| The embedded-schema check rejects a valid Liquers file | `UInt`, `Date`, `Timestamp` already round-trip (existing parquet tests); AC-6's test covers every scalar type | high |
| A caller relies on a mismatched declared schema being ignored | only `to_record`'s `schema` option reaches it; strict refusal is the documented contract | high |
| Feature matrix: code is under `polars`, used by `records-parquet` paths | no new `cfg`; `scripts/check-build-matrix.sh` is run once | high |

Recovery: one file; a revert restores today's behaviour.

## Phase 3: Examples and Tests

### Examples

The Problem Example is tests 2 and 6. Usage:

```rust
let batch = read_parquet_record_batch(&bytes, ReadSchema::Declared(&schema))?;
assert_eq!(batch.schema().as_ref(), &schema);           // roles and Id as declared
```

Query (validated with `liquers-validate`; the `encoded` form is unchanged):
`-R/data/sales.parquet/-/ns-rec/to_record-parquet-t-~X~-R/data/sales_reference.arrow/-/ns-rec/schema~E`
reads `sales.parquet` under the schema of a reference table. A mismatch fails the asset with the
AC-2 message. `-R/data/sales.parquet/-/ns-rec/to_record` restores the embedded schema of a file
written by Liquers.

### Tests

New tests in `liquers-lib/tests/records_parquet_polars.rs` (it is already gated on
`records-parquet` + `polars`). Bytes are written with `write_table(.., TableFormat::Parquet, ..)`.
"polars-written" bytes come from `ParquetWriter` on a `DataFrame`, which has no `liquers.schema`.

- `parquet_declared_schema_is_the_result_schema`: written by polars, declared in a different column
  order with an `Id` and a label. Covers AC-1.
- `parquet_declared_type_mismatch_is_refused`: the Problem Example (1); the message names `amount`,
  `Float` and `Text`. Covers AC-2.
- `parquet_undeclared_column_is_refused`. Covers AC-3.
- `parquet_declared_missing_nullable_field_reads_as_nulls`, plus the non-nullable and `Vector`
  variants, which return `Err`. Covers AC-4.
- `parquet_declared_not_null_with_nulls_is_refused`: written by polars with a null, declared
  `not_null()`. Covers AC-5.
- `parquet_infer_restores_embedded_liquers_schema`: every scalar type, an `Id`, a label and a
  `not_null`. Covers AC-6.
- `parquet_infer_without_liquers_schema_derives_from_dtypes`: written by polars. Also
  `parquet_infer_ignores_stale_liquers_schema`: written by Liquers, then the `DataFrame` read by
  polars has a column renamed and is rewritten with
  `ParquetWriter::with_key_value_metadata` (polars-io 0.55.2), carrying the original
  `liquers.schema`. A second case uses an unparseable value. Covers AC-7.
- `to_record_parquet_with_schema_option_is_strict`: `to_record` through an `Environment` with an
  `AsyncMemoryStore` holding `data/sales.parquet`, and the `schema` argument given as JSON text. It
  checks one success (AC-1) and one mismatch (AC-2). Covers AC-8.

Run with `CARGO_INCREMENTAL=0 cargo test -p liquers-lib --test records_parquet_polars`, then
`cargo test -p liquers-lib --lib --tests`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `liquers-lib/tests/records_parquet_polars.rs`: add the tests from Phase 3. All of them
  except the two "derives / ignores" AC-7 tests fail at HEAD. Proof:
  `cargo test -p liquers-lib --test records_parquet_polars`. Rollback: delete the tests.
- [x] 2. `liquers-lib/src/records/polars.rs`: add `embedded_liquers_schema` and
  `embedded_schema_describes`, and restructure `read_parquet_record_batch` (metadata, then
  `finish`) for the `Infer` path. Proof: `parquet_infer_*` (AC-6, AC-7).
- [x] 3. `polars.rs`: add `dataframe_to_declared_batch` and `check_not_null`, and wire the
  `Declared` path. Proof: `parquet_declared_*` (AC-1 to AC-5) and
  `to_record_parquet_with_schema_option_is_strict` (AC-8).
- [x] 4. Doc comments: the `polars.rs` module comment and `read_parquet_record_batch`, and
  `records/mod.rs::read_parquet_record_batch`. Proof: `cargo doc -p liquers-lib --no-deps` builds
  without warnings for these items.
- [x] 5. `specs/reference/RECORD_STREAMS.md`: update §Parquet and the formats table row, add a
  History row and bump `reviewed:`. Close `RECORDS-PARQUET-POLARS-READ-IGNORES-DECLARED-SCHEMA`
  with a resolution note (`DOCS_STRUCTURE_GUIDE.md` §4.3). Proof:
  `python3 scripts/docs_index.py --check`.

### Validation

Run `cargo test -p liquers-lib --lib --tests`,
`cargo test -p liquers-lib --no-default-features --features records --lib --tests` (Parquet reading
refuses without `polars`, which is unchanged), `bash scripts/check-build-matrix.sh`, and
`python3 scripts/docs_index.py --check`. Review the diff for edits outside `liquers-lib` and the
reference document, and for debug output.

### Implementation notes (2026-10-20)

- `to_record_parquet_with_schema_option_is_strict` links `.txt` schema files: a plain `.yaml` file
  is read as an object and cannot bind to the `String` `schema` argument. Filed as
  `YAML-SCHEMA-FILE-CANNOT-BE-LINKED-AS-SCHEMA-TEXT`.
- A failed `to_record` surfaces as an error state from `AssetRef::get`, so the test accepts either
  an `Err` or an error state, and checks the message.

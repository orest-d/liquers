---
id: POLARS-DATAFRAME-IPC
kind: design
title: A polars DataFrame is written and read as Arrow IPC
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: not-eligible
area: [lib/polars, lib/value]
issues: [POLARS-DATAFRAME-IPC-NOT-SERIALIZABLE]
created: 2026-10-20
---
# A polars DataFrame is written and read as Arrow IPC

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented.

## Phase 1: High-Level Design

### Purpose

Arrow IPC (Feather v2) is the cheapest lossless table interchange with Python, Arrow and DuckDB
consumers. polars is already built with its `ipc` feature, and the format parser already maps the
IPC spellings, but the codec refuses them. Implement both directions and declare the formats, so a
DataFrame can be stored as `.arrow`/`.feather` and read back with its column types.

### Problem Example

```rust
// liquers-lib, features polars (default)
let df = df!["id" => &[1i64, 2], "amount" => &[1.5f64, f64::NAN], "label" => &["a", "b"]]?;
let value = Value::from_polars_dataframe(df);
value.as_bytes("feather");
// today:     Err(SerializationError "Polars serialization for data_format 'feather' is not implemented yet")
// should be: Ok(Arrow IPC file bytes), and deserialize_from_bytes(&bytes, "polars.DataFrame", "feather")
//            returns a frame equal to the original (dtypes i64 / f64 / str, NaN kept)
```

The query `-R/data/sales.feather/-/ns-pl/head-5` fails the same way today: `try_to_polars_dataframe`
(`liquers-lib/src/polars/util.rs`) hands the stored bytes to `deserialize_dataframe_from_reader`
with format `feather`. Even if the codec worked, a keyed `report.feather` recipe would be refused by
the write path, because `polars.DataFrame`'s `TypeInfo` (`liquers-lib/src/value/mod.rs`) declares
only `csv`, `csv:comma`, `csv_comma` and `parquet`. Confirmed at HEAD in
`liquers-lib/src/polars/serde.rs` (`PolarsDataFormat::Ipc` in the refusing arms of
`deserialize_dataframe_from_reader` and `serialize_dataframe_to_writer`) and `liquers-lib/Cargo.toml`
(`polars` features include `ipc`).

### Scope and Acceptance Criteria

- **AC-1** IPC round trip keeps the frame
  WHEN a DataFrame with `i64`, `f64` (including NaN and a null), `str`, `bool` and `date` columns is
  written as `ipc` and read back as `polars.DataFrame` / `ipc`
  THEN the frame read back equals the original, dtypes included
- **AC-2** Every IPC spelling works and is declared
  WHEN `ipc`, `feather`, `arrow_ipc` or `arrow` is used as the data format
  THEN the codec writes and reads it, and `polars.DataFrame`'s `TypeInfo` declares it, so the
  write path accepts a `data.arrow` filename
- **AC-3** A stored IPC file feeds `ns-pl`
  WHEN a stored `.feather` file is the input of `-R/data/sales.feather/-/ns-pl/head-5`
  THEN the command receives the frame and returns its first rows
- **AC-4** Interop with liquers-records
  WHEN IPC bytes written by `liquers-records` (`write_table`, `TableFormat::Ipc`) are read as
  `polars.DataFrame`
  THEN the frame has the records' columns and values

Non-goals: `json`, `ndjson` and `avro` for DataFrames (polars' `json` feature is not enabled;
filed as `POLARS-DATAFRAME-JSON-NOT-SERIALIZABLE`); the IPC *stream* format; compression options
(polars' default writes uncompressed, which `liquers-records` also requires); switching
`liquers-py`'s DataFrame crossing from Parquet to IPC (a `liquers-py` change, enabled by this one);
records reading polars-written string columns (`IPC-READER-CANNOT-READ-ANY-POLARS-STRING-COLUMN`).

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** not-eligible — a new capability rather than a bug fix (rule 2), and the
  registry's declared formats for `polars.DataFrame` grow, which callers and stored artifacts can
  observe (rule 4).
- **Leading issue:** None.
- **Explanation:** two match arms over polars' own `IpcWriter`/`IpcReader` (already exercised in
  `liquers-lib/tests/records_ipc_polars.rs`) and four declared formats.
- **Open questions:**
  1. **Implementation detail - the `arrow` alias:** `parse_polars_data_format` gains `arrow` beside
     `ipc | feather | arrow_ipc`, matching `TableFormat::from_data_format` and `media_type.rs`
     (both treat `arrow` as the IPC file format), so the two table types accept the same spellings.

### Design Dependencies

- `value-serialization-writer` — **overlaps**, weak: its `ExtValue::write_to` override calls
  `serialize_dataframe_to_writer`, so IPC streams through it once both land. No order.
- `data-format-vocabulary` — **overlaps**, weak: the vocabulary will own the IPC alias set that
  this design writes as literals; the vocabulary's "every declared format is known" test must list
  the four spellings. Either order.
- `parquet-read-declared-schema` — **overlaps**, weak: same crate, the records bridge rather than
  the DataFrame codec. No order.

## Phase 2: Architecture

### Solution

`liquers-lib/src/polars/serde.rs`:

- `parse_polars_data_format`: `"ipc" | "feather" | "arrow_ipc" | "arrow" => PolarsDataFormat::Ipc`.
- `deserialize_dataframe_from_reader`, arm `PolarsDataFormat::Ipc`:
  `IpcReader::new(reader).finish()` (the `Cursor<T>` already satisfies `MmapBytesReader`, as for
  `ParquetReader`), errors mapped by the file's `serialization_error` helper ("Failed to deserialize
  IPC DataFrame: …").
- `serialize_dataframe_to_writer`, arm `PolarsDataFormat::Ipc`: clone the frame (columns are
  reference-counted) and `IpcWriter::new(writer).finish(&mut df)`, as the Parquet arm does.
- `PolarsDataFormat::Ipc` leaves the refusing `Json | NdJson | Avro` arms; the match stays
  exhaustive with no `_ =>`.

`liquers-lib/src/value/mod.rs`, `ExtValue::type_descriptions`, `polars.DataFrame`:
`.with_data_formats(["csv", "csv:comma", "csv_comma", "parquet", "ipc", "feather", "arrow_ipc",
"arrow"])`; the comment above it is updated (only `json`/`ndjson` remain unimplemented). Defaults
unchanged (`csv`).

`try_to_polars_dataframe`'s doc comment ("Supported formats: csv aliases and parquet") gains IPC.
Nothing else changes: `ExtValue::as_bytes` / `deserialize_from_bytes` already delegate to the two
functions, `LazyFrame` serialization goes through `serialize_dataframe_to_writer`, and
`liquers-py` calls the functions with `parquet` only.

Rejected: reading IPC through `liquers-records` and the bridge (drops polars-only dtypes and the
string limitation above applies); a new `PolarsDataFormat::Feather` variant (one format, one
variant).

Known-issue preflight: `IPC-READER-CANNOT-READ-ANY-POLARS-STRING-COLUMN` limits only the records
reader, not this codec (non-goal). Nothing blocks. Namespace touched: `pl` (input conversion only).

### Changes

- `liquers-lib/src/polars/serde.rs`: parser alias, two IPC arms, unit tests.
- `liquers-lib/src/value/mod.rs`: the `polars.DataFrame` `TypeInfo` and its comment.
- `liquers-lib/src/polars/util.rs`: doc comment.
- No command signature changes, so `specs/command_registry.yaml` is unchanged. All code is already
  under `#[cfg(feature = "polars")]`; IPC is a feature of the `polars` dependency, not of ours.
- Documents: `specs/reference/POLARS_COMMAND_LIBRARY.md` (the "Supported formats" list of the input
  conversion, and its error text, gain the IPC spellings; History row, `reviewed:`); `specs/reference/VALUE_TYPE_SYSTEM.md` has no DataFrame
  format table, so no change there.

### Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `polars/serde.rs`, `value/mod.rs`, `polars/util.rs`, the polars reference |
| Existing tests | `test_parse_polars_data_format_*` unaffected; `ext_value_type_descriptions_*` (`tests/value_type_system.rs`) compare identifiers, not formats |
| Compatibility / data | Additive: a format that was refused now works. polars' default `CompatLevel` writes `Utf8View` strings, which pyarrow ≥ 15 and polars read; older Arrow readers may not (documented in the reference) |
| Feature matrix | No new `cfg`; `--no-default-features --features polars` builds the same code |
| Recovery | Revert the commit |
| Certainty | High |

## Phase 3: Examples and Tests

### Examples

```rust
let bytes = value.as_bytes("arrow")?;
let back = Value::deserialize_from_bytes(&bytes, "polars.DataFrame", "arrow")?;
assert_eq!(back.as_polars_dataframe()?.schema(), df.schema());   // dtypes kept, unlike csv
```

Query (validated with `liquers-validate`: resource `data/sales.feather`, then action `head-5` in
namespace `pl`): `-R/data/sales.feather/-/ns-pl/head-5`.

### Tests

Unit, `liquers-lib/src/polars/serde.rs` `mod tests`:
- `test_ipc_roundtrip` — `i64`, `f64` with NaN and null, `str`, `bool`, `date`; equal schema,
  `equals_missing` on the non-float columns, the NaN checked with `is_nan`: AC-1
- `test_parse_polars_data_format_ipc_aliases` — the four spellings map to `Ipc`: AC-2

Integration, `liquers-lib/tests/polars_value_serde.rs` (already `#![cfg(feature = "polars")]`):
- `polars_value_ipc_roundtrip_every_alias` — through `liquers_lib::value::Value` for each spelling: AC-1, AC-2
- `polars_type_info_declares_ipc_spellings` — `type_info().supports_data_format` for each: AC-2
- `stored_feather_file_feeds_ns_pl_head` — `DefaultEnvironment<Value>` with an `AsyncMemoryStore`
  holding `data/sales.feather` (IPC bytes, metadata `data_format: feather`, no type identifier, as a
  hand-placed file); evaluate the AC-3 query; 5 rows, same columns: AC-3

Integration, `liquers-lib/tests/records_ipc_polars.rs` (already gated on `records-ipc` and `polars`):
- `ipc_written_by_records_reads_as_polars_value` — `write_table(.., TableFormat::Ipc, ..)` bytes
  read with `Value::deserialize_from_bytes(.., "polars.DataFrame", "ipc")`: AC-4

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-lib --lib polars::serde`,
`cargo test -p liquers-lib --test polars_value_serde --test records_ipc_polars`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-lib/src/polars/serde.rs` — `arrow` alias, IPC arms, the two unit tests — `cargo test -p liquers-lib --lib polars::serde`. Rollback: revert the commit.
- [ ] 2. `liquers-lib/src/value/mod.rs` `ExtValue::type_descriptions` — declare the four spellings, update the comment; `liquers-lib/src/polars/util.rs` doc comment — `cargo test -p liquers-lib --test value_type_system`. Depends on 1.
- [ ] 3. `liquers-lib/tests/polars_value_serde.rs`, `liquers-lib/tests/records_ipc_polars.rs` — the four integration tests — `cargo test -p liquers-lib --test polars_value_serde --test records_ipc_polars`. Depends on 2.
- [ ] 4. `specs/reference/POLARS_COMMAND_LIBRARY.md` "Supported formats" list, History row, `reviewed:`; close `POLARS-DATAFRAME-IPC-NOT-SERIALIZABLE` with a resolution naming the tests — `python3 scripts/docs_index.py --check`.

### Validation

`CARGO_INCREMENTAL=0 cargo test -p liquers-lib --lib --tests`;
`cargo test -p liquers-lib --no-default-features --features polars --lib --tests`; diff review:
only the three source files, two test files, the reference, the issue and the index.

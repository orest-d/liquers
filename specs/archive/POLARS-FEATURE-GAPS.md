# POLARS-FEATURE-GAPS

Status: Partially Implemented

## Summary
Standardize Polars serialization/deserialization in `liquers_lib::polars` with reusable utility functions, explicit format mapping, and integration into `DefaultValueSerializer` for dataframe values.

## Scope
1. Add utility functions for dataframe serialization/deserialization in `liquers_lib::polars`.
2. Utilities should be format-driven (`data_format` string) and writer/reader based.
3. Serialization entry should accept `LazyFrame` to keep API flexible for future streaming/lazy sinks.
4. Integrate these utilities into `DefaultValueSerializer` for `ExtValue::PolarsDataFrame`.
5. Define canonical format names (including CSV separator variants).

## Primary Findings (Polars 0.53, Rust)

### 1. Lazy vs eager tradeoff
1. `DataFrame::lazy()` in Polars is lightweight (builds a lazy logical plan around existing dataframe), so converting eager->lazy is not expected to be a significant penalty by itself.
2. Serialization APIs split into:
   1. eager writer APIs (`CsvWriter`, `ParquetWriter`, `IpcWriter`, `JsonWriter`) that write to `std::io::Write`,
   2. lazy sink APIs (`LazyFrame::sink_csv/sink_parquet/sink_ipc/sink_json`) that target `SinkTarget`.
3. For in-memory writer-centric Liquers serialization, eager writer APIs are currently the most straightforward.

### 2. Writer shape and async-forward compatibility
1. Eager Polars writers use synchronous `std::io::Write` traits.
2. Lazy sinks support `SinkTarget::Dyn` through Polars `DynWriteable` (sync write abstraction).
3. Polars also has async abstractions (`AsyncWriteable`) in IO internals, but writer integration still requires adaptation around sync writer traits in common paths.
4. Conclusion: future Opendal async writer support is feasible via adapters, but not a direct drop-in for current eager writer signatures.

### 3. XLSX in Rust Polars
1. Current Polars Rust crate (0.53) does not expose built-in XLSX reader/writer APIs.
2. Therefore `xlsx` serialization requires:
   1. external crate integration (recommended), or
   2. explicit `Unsupported format` behavior until integration is added.
3. This is a hard design decision for the minimum requirement "serialization: csv, parquet, xlsx".

### 4. `polars_excel_writer` as XLSX backend candidate
1. `polars_excel_writer` provides a dedicated `PolarsExcelWriter` API for writing Polars dataframes to XLSX.
2. It supports:
   1. `write_dataframe(&DataFrame)`,
   2. `save(path)`,
   3. `save_to_buffer()`,
   4. `save_to_writer(...)`.
3. This API shape fits Liquers serializer requirements well, especially writer-centric serialization.
4. Important compatibility note:
   1. `polars_excel_writer` version compatibility must match the exact Polars version in Liquers,
   2. Liquers currently uses `polars 0.53.0`.
5. Resulting tradeoff:
   1. either upgrade Liquers Polars stack to `0.52`,
   2. or pin `polars_excel_writer` to a version compatible with `polars 0.51` (if available),
   3. or temporarily feature-gate xlsx as unsupported until version alignment is resolved.

## Format Matrix (Polars-oriented)

| Format Name | Extension | `data_format` strings (proposed) | Serialize | Deserialize | Polars methods | Notes |
|---|---|---|---|---|---|---|
| Comma-separated values | `csv` | `csv`, `csv_comma` | Yes | Yes | `CsvWriter`, `CsvReadOptions/CsvReader` | separator `,` |
| Tab-separated values | `tsv` | `tsv`, `csv_tab` | Yes | Yes | same CSV APIs | separator `\\t` |
| Semicolon-separated values | `csv` | `csv_semicolon` | Yes | Yes | same CSV APIs | separator `;` |
| Pipe-separated values | `csv` | `csv_pipe` | Yes | Yes | same CSV APIs | separator `|` |
| Apache Parquet | `parquet` | `parquet` | Yes | Yes | `ParquetWriter`, `ParquetReader` | binary columnar |
| Arrow IPC file (Feather) | `ipc`,`feather`,`arrow` | `ipc`, `feather`, `arrow_ipc` | Yes | Yes | `IpcWriter`, `IpcReader` | optional in scope |
| Arrow IPC stream | `arrows` | `ipc_stream` | Limited | Limited | IPC stream APIs | separate from file IPC |
| JSON (array/object rows) | `json` | `json` | Yes | Yes | `JsonWriter`, `JsonReader` | optional in scope |
| NDJSON | `ndjson` | `ndjson`, `jsonl` | Yes (JsonLines) | Yes | `JsonWriter`(json-lines), json readers | optional in scope |
| Apache Avro | `avro` | `avro` | Yes (feature-gated) | Yes (feature-gated) | `AvroWriter`, `AvroReader` | not enabled now |
| Excel workbook | `xlsx` | `xlsx` | Not in Polars core | Not in Polars core | N/A in Polars 0.53 | needs external crate |

## Implementation Status (current)
1. Implemented:
   1. Shared utility module `liquers_lib::polars::serde` with:
      1. `PolarsDataFormat`,
      2. `parse_polars_data_format`,
      3. CSV/Parquet dataframe reader/writer helpers,
      4. lazyframe serialization helper (`collect + eager writer` baseline),
      5. explicit unsupported handling for `xlsx`.
   2. `ExtValue::PolarsDataFrame` `DefaultValueSerializer` integration:
      1. `as_bytes("csv" | aliases | "parquet")`,
      2. `deserialize_from_bytes(..., "polars_dataframe", ...)`.
   3. `CombinedValue` deserialization path updated to allow extended-value deserialization when base deserialization fails.
   4. `polars::util::try_to_polars_dataframe` and `polars::io::{from_csv,to_csv}` now use shared serde utilities.
   5. Test coverage added for:
      1. format parsing aliases,
      2. CSV and Parquet roundtrips,
      3. unsupported XLSX behavior,
      4. value serializer roundtrip for `polars_dataframe`.
2. Not implemented yet:
   1. XLSX backend via `polars_excel_writer`,
   2. IPC/JSON/NDJSON/AVRO read-write implementation,
   3. lazy sink streaming path (`sink_*`) without full materialization.

## Minimum Required Support (this feature request)
1. Deserialization: `csv`, `parquet`.
2. Serialization: `csv`, `parquet`, `xlsx`.
3. Because Rust Polars lacks XLSX core APIs, `xlsx` requires an explicit external integration plan.

## Proposed API Design

### 1. New utility module
Create `liquers_lib::polars::serde` (or `liquers_lib::polars::io_serde`) with:

1. `parse_polars_data_format(data_format: &str) -> Result<PolarsDataFormat, Error>`
2. `deserialize_dataframe_from_reader<R: Read + Seek>(reader: R, data_format: &str) -> Result<DataFrame, Error>`
3. `deserialize_lazyframe_from_reader<R: Read + Seek>(reader: R, data_format: &str) -> Result<LazyFrame, Error>`
4. `serialize_lazyframe_to_writer<W: Write>(lf: LazyFrame, data_format: &str, writer: W) -> Result<(), Error>`
5. `serialize_dataframe_to_writer<W: Write>(df: &DataFrame, data_format: &str, writer: W) -> Result<(), Error>`

Notes:
1. `serialize_lazyframe_to_writer` is the primary API per requirement.
2. `serialize_dataframe_to_writer` is convenience and should delegate to lazy API or shared internal helpers.
3. `deserialize_*` should normalize CSV variants to separator byte and select appropriate reader.

### 2. Format enum
Define:
`enum PolarsDataFormat { Csv { separator: u8 }, Parquet, Ipc, Json, NdJson, Avro, Xlsx }`

Rules:
1. canonical aliases: `csv`, `csv_comma`, `tsv`, `csv_tab`, `csv_semicolon`, `csv_pipe`, `parquet`, `xlsx`.
2. unknown format -> `ErrorType::SerializationError`.

### 3. Lazy serialization strategy
1. Preferred baseline implementation:
   1. `lf.collect()` then write with eager writer APIs.
   2. This keeps writer-based API simple and works for CSV/Parquet immediately.
2. Future optimization path:
   1. use lazy `sink_*` + `SinkTarget::Dyn` adapters for streaming without full materialization.
   2. implement only after benchmark evidence and adapter complexity review.

### 4. XLSX strategy
1. Add explicit abstraction point in utility function for `Xlsx`.
2. Phase A: return clear `Unsupported` error if xlsx backend is not enabled.
3. Phase B: integrate dedicated Rust XLSX writer crate and wire `PolarsDataFormat::Xlsx` to it.
4. Preferred backend candidate: `polars_excel_writer` (subject to Polars version compatibility).
5. Integration target shape:
   1. `PolarsExcelWriter::new().write_dataframe(df)?;`
   2. `save_to_writer(&mut writer)` (preferred) or `save_to_buffer()` fallback.

## Integration Into `DefaultValueSerializer`

Target: `liquers-lib/src/value/mod.rs` (`impl DefaultValueSerializer for ExtValue`).

Plan:
1. For `ExtValue::PolarsDataFrame` in `as_bytes(format)`:
   1. allocate `Vec<u8>`,
   2. wrap in `Cursor<Vec<u8>>`,
   3. call `serialize_dataframe_to_writer(..., format, &mut cursor)`,
   4. return bytes.
2. For `deserialize_from_bytes(...)` when `type_identifier == "polars_dataframe"`:
   1. parse format,
   2. create `Cursor<&[u8]>`,
   3. call `deserialize_dataframe_from_reader`,
   4. return `ExtValue::from_polars_dataframe`.
3. Keep non-polars variants unchanged.

## Complete Implementation Plan

### Phase 1: Format model and utility functions
1. Add `PolarsDataFormat` enum and parser.
2. Implement CSV (with separator variants) and Parquet read/write utilities.
3. Implement `LazyFrame` serialization entry (collect+writer baseline).
4. Add unit tests:
   1. roundtrip CSV comma/tab/semicolon/pipe,
   2. roundtrip Parquet,
   3. unsupported format errors.

### Phase 2: Serializer integration
1. Wire utilities into `ExtValue` `DefaultValueSerializer`.
2. Add tests for:
   1. `as_bytes("csv")` and `as_bytes("parquet")` on `PolarsDataFrame`,
   2. `deserialize_from_bytes` for `polars_dataframe`,
   3. error behavior for unsupported `xlsx` when backend absent.

### Phase 3: Commands alignment
1. Update `liquers_lib::polars::io` command handlers (`from_csv`, `to_csv`, future `to_parquet`, etc.) to use shared utilities.
2. Remove duplicated ad-hoc CSV parsing/writing logic.
3. Ensure command metadata/documentation uses canonical `data_format` names.

### Phase 4: XLSX backend decision and implementation
1. Confirm and adopt `polars_excel_writer` as backend.
2. Resolve version compatibility (`polars 0.51` vs backend-supported Polars version).
3. Add optional feature flag in `liquers-lib` (e.g., `polars_xlsx`).
4. Implement `serialize_*` for `Xlsx` with clear feature-gated errors.
5. Add integration tests for XLSX when enabled.

## Open Decisions / Ambiguities Requiring Confirmation
1. Should we upgrade to Polars `0.52` to align with current `polars_excel_writer`, or find/pin a compatible backend version for Polars `0.51`?
2. Should `xlsx` be mandatory in default build or feature-gated?
3. Should lazy sink streaming (`sink_*`) be introduced now, or deferred until benchmark suite exists?

## Suggested Acceptance Criteria
1. Shared utility module exists and is used by serializer + polars io commands.
2. Minimum required formats are supported as specified:
   1. deserialize: csv, parquet,
   2. serialize: csv, parquet, xlsx (or explicit feature-gated behavior with documented fallback).
3. Data format mapping is canonical and test-covered.
4. No duplicated CSV/Parquet serialization code paths remain in `liquers_lib`.

## References (Primary)
1. Polars lazy frame sink methods (`sink_csv/sink_parquet/sink_ipc/sink_json`) in `polars-lazy` 0.51 source.
2. Polars IO reader/writer trait bounds in `polars-io` 0.53 (`CsvWriter`, `ParquetWriter`, `CsvReader`, `ParquetReader`).
3. Polars sink target abstraction (`SinkTarget::Path` / `SinkTarget::Dyn`) in `polars-plan` 0.51.
4. Polars `Writeable` / `DynWriteable` and async adaptation in `polars-io` 0.51.
5. Polars crate feature list in `polars` 0.51 `Cargo.toml` (csv/parquet/ipc/json/avro, no xlsx feature).
6. `polars_excel_writer` docs and `PolarsExcelWriter` API on docs.rs.

# Phase 3: Examples & Use-cases — A record source's rows served as a byte stream

All queries were checked with `liquers-validate` (registry at HEAD): each plans to
`GetAsset[data/sales/daily.manifest.yaml]`, the `rec/to_record_source` action and a `Filename`.

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | A manifest served as CSV | streamed CSV | AC-1, AC-7 |
| 2 | Example | A non-uniform manifest as NDJSON | NDJSON lines, CSV refusal | AC-2, AC-3 |
| 3 | Unit | `liquers_records::formats::stream::tests::*` | encoder behaviour | AC-1, AC-2, AC-3, AC-5 |
| 4 | Test | `liquers-lib/tests/record_source_stream.rs` | encoder through `get_streamed` | AC-1, AC-3, AC-4, AC-6, AC-7 |
| 5 | Test | `liquers-axum/tests/record_source_stream_routes.rs` | HTTP behaviour | AC-1, AC-4, AC-5, AC-6 |
| 6 | Check | `scripts/check-build-matrix.sh` | feature and wasm builds | AC-8 |

## Example 1: a manifest served as CSV

`data/sales/daily.manifest.yaml` declares `uniform_schema` (`date: date, region: text,
amount: int`) and three explicit chunks.

```text
GET /liquer/q/-R/data/sales/daily.manifest.yaml/-/ns-rec/to_record_source/daily.csv
```

`200`, `Content-Type: text/csv`, no `Content-Length`; body `date,region,amount\n` then the rows of
chunk 1, 2, 3, each sent as its chunk is encoded. Narrowing streams too:
`…/ns-rec/to_record_source/ns-rec/select_columns-region-amount/daily.csv` (validated).
`…/ns-rec/materialize/daily.csv` is unchanged: whole, `Content-Length`, `max_rows`.

## Example 2: a non-uniform manifest as NDJSON

Chunk 1 has `region, amount`; chunk 2 adds `channel`. `…/to_record_source/daily.ndjson` →
`{"region":"EU","amount":1}` … `{"region":"US","amount":5,"channel":"web"}`.
`…/to_record_source/daily.csv` → the error answer naming `ndjson` and `uniform_schema`.

## Edge and Error Cases

| Case | Expected | Test |
|---|---|---|
| first chunk's query fails | error answer, its status | 4, 5 |
| chunk 2 of 3 fails, CSV | client body read fails after chunk 1's rows | 5 |
| chunk 2 of 3 fails, NDJSON | last line `{"$liquers_error":{…}}`, then incomplete transfer | 3, 5 |
| `…/to_record_source/daily.yaml` and `.json` | the manifest, as at HEAD | 4 |
| entry route on `…/daily.csv` | the HEAD refusal naming `ns-rec/materialize` | 5 |
| `.parquet`, `.md`, `.html` on a source | the HEAD refusal (not streamable) | 4 |

## Test Plan

- **Unit, `liquers-records/src/formats/stream.rs` `mod tests`** (fixture `ChunkResolver`s and
  in-memory sources as in `sources.rs` tests):
  `csv_stream_equals_materialized_csv` (AC-1: concatenated chunks equal `write_table` of
  `materialize`, one output chunk per batch); `csv_stream_refuses_undeclared_schema_first` (AC-3);
  `ndjson_stream_mixes_schemas` (AC-2); `ndjson_stream_ends_with_error_line` (AC-5: last `Ok` item
  is the `$liquers_error` line, then `Err`); `csv_stream_error_after_first_batch` (AC-5).
  Command: `cargo test -p liquers-records --lib formats::stream`.
- **Integration, `liquers-lib/tests/record_source_stream.rs`** (`#![cfg(feature = "records")]`;
  `DefaultEnvironment<Value>` with `AsyncMemoryStore::new(&Key::new())` holding the manifest and
  three CSV chunks, commands from `register_all_commands`):
  `get_streamed_streams_a_source_as_csv` (AC-1: `Stream`, ≥3 chunks, body equals the materialize
  route's bytes); `get_streamed_counts_chunk_evaluations_lazily` (AC-1: after pulling the first
  output chunk, chunk 3's asset has not been evaluated — checked through the asset manager);
  `get_streamed_does_not_cache` (AC-7); `manifest_formats_unchanged` (AC-6: `yaml`/`json` bytes
  equal HEAD's `as_bytes`; `get_binary` on `daily.csv` still refuses);
  `first_chunk_error_is_an_err` (AC-4); `undeclared_schema_csv_is_refused` (AC-3).
  Command: `CARGO_INCREMENTAL=0 cargo test -p liquers-lib --test record_source_stream`.
- **Routes, `liquers-axum/tests/record_source_stream_routes.rs`**: `GET` of Example 1 (status,
  type, no `Content-Length`, body); first-chunk failure → error status; chunk-2 failure for CSV
  and NDJSON (client sees an error reading the body; NDJSON prefix ends with the error line);
  the entry route refusal. Command: `cargo test -p liquers-axum --test record_source_stream_routes`.
- **Matrix** (AC-8): `bash scripts/check-build-matrix.sh` (no-`records` rows and wasm32).

## Learning Log

- A CSV served from a source needs `uniform_schema` in the manifest; the guide's §3.3 advice
  becomes a requirement for CSV export, worth stating in §7 Pitfalls.
- A truncated CSV is indistinguishable from a complete one to a client that ignores transfer
  errors; NDJSON is the format to recommend for large exports.

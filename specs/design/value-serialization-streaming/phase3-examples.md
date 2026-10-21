# Phase 3: Examples & Use-cases — A value's byte form as a pull stream

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Serving a DataFrame as CSV | streamed body, first chunk first | AC-2, AC-7 |
| 2 | Example | A small value | unchanged whole path, cached | AC-1, AC-5 |
| 3 | Test | `liquers_core::value::tests::serialize_stream_*` | default identity, `Bytes` sharing | AC-1, AC-8 |
| 4 | Test | `liquers-core/tests/serialized_stream.rs` | asset read and cache rule, adapter | AC-5 |
| 5 | Test | `liquers-axum` `api_core::serving::tests` + `query_api_routes.rs` | HTTP order, errors, timeout, entry | AC-2, AC-3, AC-4, AC-6, AC-9 |
| 6 | Test | `liquers-lib/tests/dataframe_stream.rs` | DataFrame chunks, forwarding | AC-7, AC-8, AC-10 |

## Example 1: Serving a DataFrame as CSV

Query (validated, `encoded` unchanged): `-R/data/sales.csv/-/ns-pl/from_csv/sales.csv` — read the
stored CSV, parse it to a frame, serve it as `csv`. Unkeyed, so no version serialization happens.

```rust
// in serve_query, after evaluation:
let (serialized, metadata) = asset_ref.get_serialized().await?;   // Stream { length: None }
let ready = first_chunk(serialized).await?;                         // pulled inside the timeout
respond(ready, &metadata)                                           // 200, chunked, text/csv
```

The encoder runs in a blocking task; each `write` of polars' `CsvWriter` becomes a chunk; the
bounded channel holds at most 8 chunks, so memory is the frame plus ~512 KiB.

## Example 2: A small value

`-R/data/note.txt` (validated: `GetAsset`) — a `Text` value: `serialize_stream` returns `Whole`,
`get_serialized` caches it as `get_binary` did, and the response has `Content-Length`. A second
request is served from the cache with no encoding.

## Edge and Error Cases

- **Error before the first byte** — a test value whose stream yields `Err` first → the error answer
  (status from the error type, error body), as a failed `get_binary`.
- **Error after some bytes** — `Ok(a)`, `Ok(b)`, `Err(e)` → the client's read of the body fails
  (hyper reports an incomplete body); the received prefix is `a b`.
- **Slow first chunk** — a stream that sleeps longer than the configured timeout before its first
  item → the HEAD timeout answer; one that sleeps after its first item → completes.
- **Client disconnect** — dropping the response body ends the `from_writer` task (its next write
  fails); checked with a counting writer that stops being called.
- **Entry route** — `q/entry` on the streamed test value equals the entry of its collected bytes.
- **Feature matrix** — `PolarsDataFrame` arm is `#[cfg(feature = "polars")]`; wasm32 `from_writer`
  collects (check build only).

## Test Plan

- Unit, `liquers-core/src/value.rs` `mod tests`:
  `serialize_stream_default_equals_as_bytes` (AC-1: every core variant × every accepted format, and
  a refused format gives the same error), `serialize_stream_bytes_shares_buffer` (AC-8),
  `serialized_bytes_collect_and_into_stream` (AC-1 support).
- Integration, new `liquers-core/tests/serialized_stream.rs` (`SimpleEnvironment<Value>`,
  `AsyncMemoryStore`, a registered command returning a core `Value`):
  `stream01_whole_value_is_cached_once` (AC-5), `stream03_from_writer_chunks_and_backpressure`
  (AC-7 support: more than one chunk; a dropped receiver stops the encoder).
- Unit, `liquers-axum/src/api_core/serving.rs` `mod tests` (hand-built
  `SerializedBytes::Stream` values from `futures::stream::iter` / `unfold` with sleeps; responses
  read with `axum::body::Body::into_data_stream`): `streamed01_body_arrives_in_order` (AC-2),
  `streamed02_first_chunk_error_is_status` (AC-3), `streamed03_late_error_aborts_body` (AC-4),
  `streamed04_timeout_bounds_first_chunk` (AC-6, through the `timed_first_chunk` helper that
  `serve_query` uses), `streamed06_whole_value_has_content_length` (AC-5). A custom value type in
  an axum integration test would need a full `ValueInterface`; the serving functions are where the
  behaviour lives.
- Integration, `liquers-axum/tests/query_api_routes.rs` (existing file, core `Value`):
  `streamed05_entry_collects` (AC-9: `q/entry` of a `Bytes` value equals its `data` body) and the
  existing route tests unchanged (whole path).
- Integration, new `liquers-lib/tests/dataframe_stream.rs`, `#![cfg(feature = "polars")]`:
  `dataframe_csv_streams_in_chunks_equal_to_as_bytes` (AC-7, 100 000 rows, `csv` and `parquet`),
  `combined_value_forwards_serialize_stream` (AC-10, a frame and a `SimpleValue::Bytes`, AC-8),
  `stream02_dataframe_asset_stream_is_not_cached` (AC-5: the unkeyed query
  `-R/data/sales.csv/-/ns-pl/from_csv/sales.csv` over a memory store; `get_serialized` twice gives
  two streams and `poll_binary` stays `None`).
- Commands: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests`,
  `cargo test -p liquers-axum`, `cargo test -p liquers-lib --test dataframe_stream`,
  `cargo test -p liquers-lib --lib --tests`, `bash scripts/check-build-matrix.sh`.

## Learning Log

- The keyed persistence path serializes whole for the content version, so streaming pays off only
  for unkeyed, volatile or released encodings — choose test queries accordingly.
- `-R/…` without a header plans to `GetAsset`; `-R-stored/…` to `GetResource` (raw stored bytes).
- `liquers-axum` cannot name `liquers-lib` values, so its streamed behaviour is tested on the
  serving functions with hand-built streams; the end-to-end DataFrame case is a `liquers-lib` test.

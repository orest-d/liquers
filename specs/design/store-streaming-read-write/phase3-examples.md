# Phase 3: Examples & Use-cases — Streaming reads and writes on AsyncStore

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Serve a range from a file store | `get_stream` with a range | AC-2, AC-5 |
| 2 | Example | Store an upload as it arrives | `set_stream` from an HTTP body | AC-3, AC-7 |
| 3 | Test | conformance `stream01`-`stream03` | equivalence and failure rule on every suite's store | AC-1, AC-2, AC-3, AC-4, AC-6 |
| 4 | Test | `liquers_core::store::tests::stream*` | file store and default overrides | AC-1 to AC-6 |
| 5 | Test | `liquers-store` `opendal_stream_*` | native reader/writer, abort | AC-1 to AC-5 |
| 6 | Test | `liquers-axum/tests/store_api_routes.rs` streaming cases | `206`, `416`, streamed `PUT` | AC-7 |

## Example 1: Serve a range from a file store

A caller holding any `Arc<dyn AsyncStore>` reads one kilobyte of a large object:

```rust
let key = parse_key("data/big/video.mp4")?;
let (mut chunks, metadata) = store.get_stream(&key, Some(0..1024)).await?;
let mut first = Vec::new();
while let Some(chunk) = chunks.next().await {
    first.extend_from_slice(&chunk?);
}
assert_eq!(first.len(), 1024);
assert_eq!(metadata.get_media_type(), "video/mp4");
```

On `AsyncFileStore` this opens the file, seeks and reads at most 1024 bytes; on
`AsyncOpenDALStore` it is one ranged backend read; on `AsyncMemoryStore` it is the default slice.

## Example 2: Store an upload as it arrives

```rust
// liquers-axum put_data_handler, after this design
let chunks: ByteStream<'_> = Box::pin(
    body.into_data_stream()
        .map(|chunk| chunk.map_err(|e| Error::from_error(ErrorType::General, e))),
);
store.set_stream(&key, chunks, &metadata).await?;
```

If the client disconnects, the stream yields an error, `set_stream` returns it, and the
key holds no partial file.

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| empty object, `range = Some(0..0)` or `None` | zero chunks or one empty chunk; concatenation `[]` | `stream01` |
| range end beyond size | clipped to size | `stream02` |
| range start beyond size of a non-empty object | `Err` from `get_stream`; HTTP `416` | `stream_range_beyond_size_is_refused`, `store_get_range_out_of_bounds_is_416` |
| input stream fails after two chunks | `Err`; key absent (file, OpenDAL fs) or previous object (memory, atomic services) | `stream03` |
| relative key | `KeyNotAbsolute` before any I/O | `stream_methods_refuse_relative_keys` |
| key with a reserved name | `KeyNotSupported` (same path builders as `set`) | `stream_methods_refuse_reserved_names` |
| concurrent `set_stream` and `set` on one file-store key | serialized by the store lock | `file_store_stream_write_holds_the_lock` |

## Test Plan

- Conformance, `liquers-core/src/store_conformance/rules/streaming.rs`, registered in `RULES`
  and so run by every existing suite (core, store, web Node loop):
  - `stream01` — `set_stream` of three chunks then `get_stream(None)` equals `get_bytes`, and
    metadata `file_size` equals the total; requires `write`, level `CreateOnly` (AC-1, AC-3, AC-6)
  - `stream02` — ranges `0..1`, `1..n+10`, `n-1..n` on a fresh key (AC-2, AC-6)
  - `stream03` — an input stream erroring after one chunk: `Err`, then `get_bytes` is
    `KeyNotFound` on a fresh key; on an existing key either the old bytes or `KeyNotFound`, never
    the first chunk (AC-4, AC-6)
- Unit, `liquers-core/src/store.rs` `mod tests`:
  - `stream_default_collects_through_get_and_set` — a `get`/`set`-only test store (AC-6)
  - `file_store_streams_in_bounded_chunks` — 1 MiB object; largest chunk ≤ 64 KiB both ways, via
    a counting input stream (AC-5)
  - `file_store_failed_stream_leaves_nothing` — data file and sidecar absent (AC-4)
  - `file_store_stream_write_holds_the_lock`, `stream_methods_refuse_relative_keys`,
    `stream_methods_refuse_reserved_names`, `stream_range_beyond_size_is_refused` (edge cases)
  - `router_forwards_streams` — router over memory + file stores (AC-1, AC-6)
- Unit, `liquers-store/src/opendal_store.rs` `mod tests` (memory and fs services):
  `opendal_stream_round_trip` (AC-1, AC-3), `opendal_stream_range` (AC-2),
  `opendal_failed_stream_aborts` (AC-4), `opendal_stream_chunks_are_bounded` (AC-5)
- Integration, `liquers-axum/tests/store_api_routes.rs`: `store_get_range_is_206`,
  `store_get_range_out_of_bounds_is_416`, `store_get_without_range_is_200`,
  `store_put_streams_the_body` (AC-7)
- Commands: `cargo test -p liquers-core --features store-conformance --lib --tests`,
  `cargo test -p liquers-store --features store-conformance --lib --tests`,
  `cargo test -p liquers-axum`.

## Learning Log

- `Reader::into_bytes_stream(range)` and `Writer::{write, abort, close}` are the OpenDAL 0.55
  entry points (read in the vendored source); `close` returns OpenDAL metadata that is not needed.
- No Liquers query is involved (nothing for `liquers-validate`).

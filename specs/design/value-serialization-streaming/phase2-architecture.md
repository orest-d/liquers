# Phase 2: Solution & Architecture — A value's byte form as a pull stream

## Overview

A defaulted `DefaultValueSerializer::serialize_stream` returns `SerializedBytes` — `Whole(Bytes)`
or `Stream { chunks, length }`. The default wraps `as_bytes`, so nothing changes for a type that
does not override it. `State::serialize_stream` applies the value-error gate and the effective
data format; `AssetRef::get_serialized` applies the read gate, returns the cached binary as
`Whole`, caches a fresh `Whole`, and never caches a `Stream`. One `liquers-axum` serving function
turns `SerializedBytes` into a response, pulling the first chunk of a stream before the headers.
A core adapter, `SerializedBytes::from_writer`, turns a synchronous writer-based encoder into a
stream (a blocking task and a bounded channel), which is how `PolarsDataFrame` streams.

Rejected: a stream-only primitive (Phase 1 Q2); `as_bytes` rebuilt on the stream (the async
collect cannot run inside a synchronous `as_bytes`); an `AsyncWrite` push form (the HTTP body pulls;
push is the adapter); an `Environment`-level hook (only one `Environment` impl exists, in core, so
`liquers-lib` could not override it); special-casing record sources in `liquers-axum` (it cannot
name `ExtValue` — `record-streams`' reason for this feature).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `CORE-STORE-OPENBIN-MISSING` (design `store-streaming-read-write`) | accepted | P3 | provides `BoxStream`, `ByteStream`, `bytes`, `StreamResponse` | yes, or step 1 adds them | no | `requires` |
| `SHAREABLE-METADATA-AND-BYTES-ARE-DEEP-COPIED` (design `shared-metadata-and-bytes`) | draft | P3 | cached binary and `Value::Bytes` as `Bytes` | yes (stages B, C) | no | `requires` |
| `VALUE-SERIALIZATION-HAS-NO-INCREMENTAL-WRITER` (design `value-serialization-writer`) | draft | P2 | the same DataFrame encoder, pushed to a caller's writer | no | no | overlap |
| `SERIALIZATION-CANNOT-REPORT-A-LOSSY-WRITE` | draft | P2 | warnings where a `Whole` is cached | no | no | overlap |
| `LIBRARY-CODE-USES-UNWRAP-AND-EXPECT` | draft | P2 | new code adds none | no | no | none |
| `AXUM-QUERY-TIMEOUT-HARDCODED`, `AXUM-ASSETS-API-SERVES-ONLY-BYTES-AND-TEXT` | closed | — | the refactors §1/§3 of the issue asked for are done | — | — | none |

## Interfaces

```rust
// liquers-core/src/value.rs (new items beside DefaultValueSerializer)
pub enum SerializedBytes {
    /// The whole encoding; cacheable, length known.
    Whole(Bytes),
    /// Produced as pulled; never cached. `length` when known in advance (Content-Length).
    Stream { chunks: crate::store::ByteStream<'static>, length: Option<u64> },
}
impl SerializedBytes {
    pub fn length(&self) -> Option<u64>;
    pub async fn collect(self) -> Result<Bytes, Error>;           // Whole: as is; Stream: concatenate
    pub fn into_stream(self) -> crate::store::ByteStream<'static>; // Whole: one chunk
    /// Runs a writer-based encoder in a blocking task, its writes becoming chunks (bounded channel,
    /// 8 chunks of at most 64 KiB). Native only; on wasm32 it runs `encode` into a Vec → Whole.
    pub fn from_writer<F>(encode: F) -> SerializedBytes
    where F: FnOnce(&mut dyn std::io::Write) -> Result<(), Error> + Send + 'static;
}

// DefaultValueSerializer — defaulted, synchronous: a stream does its async work when polled.
fn serialize_stream(self: Arc<Self>, data_format: &str) -> Result<SerializedBytes, Error> {
    Ok(SerializedBytes::Whole(Bytes::from(self.as_bytes(data_format)?)))
}

// liquers-core/src/state.rs
impl<V: ValueInterface> State<V> {
    pub fn serialize_stream(&self) -> Result<SerializedBytes, Error>;   // value_error, effective format
}

// liquers-core/src/assets.rs
impl<E: Environment> AssetRef<E> {
    /// get_binary's gate and wait; cached binary → Whole; fresh Whole → cached; Stream → not cached.
    pub async fn get_serialized(&self) -> Result<(SerializedBytes, Arc<Metadata>), Error>;
}

// liquers-axum/src/api_core/serving.rs (new, pub(crate))
pub(crate) async fn first_chunk(s: SerializedBytes) -> Result<ReadyBody, Error>;  // pulls one chunk
pub(crate) fn respond(body: ReadyBody, metadata: &Metadata) -> Response;          // Binary or Stream
pub(crate) async fn timed_first_chunk<F>(timeout: Duration, fut: F) -> Result<Result<(ReadyBody, Arc<Metadata>), Error>, Elapsed>
where F: Future<Output = Result<(SerializedBytes, Arc<Metadata>), Error>>;   // serve_query's timeout
```

`self: Arc<Self>` is legal on a `Sized` trait and gives the stream ownership (`'static`), which an
HTTP body needs; `State` already holds `Arc<V>`. `from_writer` uses `tokio::task::spawn_blocking`
(core's native tokio has `rt`) and `tokio::sync::mpsc` (`sync`); the writer side calls
`blocking_send`, so a slow client applies back-pressure to the encoder. A dropped receiver makes the
next write fail, which ends the blocking task.

## Integration Points

- `liquers-core/src/value.rs`: `SerializedBytes`, `serialize_stream` default, override for core
  `Value` — `Value::Bytes` in the formats whose `as_bytes` arm returns the bytes unchanged returns
  `Whole(buffer.clone())`; every other case calls the default body through a public helper
  `serialize_stream_via_as_bytes` (an override cannot call the trait default it replaces).
- `liquers-core/src/state.rs`: `State::serialize_stream`.
- `liquers-core/src/assets.rs`: `AssetRef::get_serialized`; `get_binary` becomes
  `get_serialized` + `collect` + cache (unchanged results); the `Whole` caching shares
  `serialize_to_binary`'s write-lock section.
- `liquers-lib/src/value/simple.rs`: `SimpleValue::Bytes` override (as core `Value`);
  `extended.rs`: `CombinedValue` forwards to `Base`/`Ext`, wrapping the half in its own `Arc` (an
  `Arc<CombinedValue>` cannot yield `Arc<B>` without a clone: `Base(b)` → `Arc::new(b.clone())`,
  cheap after `core-value-enum-oversized`; `Ext(e)` clones `ExtValue`, whose payloads are `Arc`s);
  `value/mod.rs` `ExtValue`: `PolarsDataFrame` (feature `polars`) for `csv` and `parquet` →
  `SerializedBytes::from_writer(move |w| serialize_dataframe_to_writer(&df, &fmt, w))` — the
  existing writer-based encoder (`liquers-lib/src/polars/serde.rs`, generic over `W: Write`), with
  the `DataFrame` cloned into the closure (columns are reference-counted);
  every other variant the default (explicit arms, no `_ =>`).
- `liquers-axum`: new `api_core/serving.rs`; `query/handlers.rs` `serve_query` — the timeout wraps
  evaluation, `get_serialized` and `first_chunk`; `assets/common.rs` `asset_bytes` uses the same
  two functions; `asset_entry` and `key_handlers.rs` entry use `get_serialized` + `collect`
  (AC-9); `api_core/response.rs` / `axum_integration.rs` `StreamResponse` (from
  `store-streaming-read-write`): `Body::from_stream`, `Content-Length` when `length` is known,
  `Content-Type` and `X-Liquers-Status` as `BinaryResponse`.
- Not touched: `liquers-web`, `liquers-py` (whole bytes through `get_binary`), the store, the
  persistence path (`prepare_version`, `save_to_store`).

## Error Handling

| Failure | Result |
|---|---|
| unsupported format, value error, gated status | `Err` from `serialize_stream`/`get_serialized`, unchanged constructors; HTTP status as today |
| first stream item `Err` | `first_chunk` returns it; the response is the error answer (AC-3) |
| later item `Err` | the body stream yields it; hyper aborts the chunked body; `eprintln!` names the query and error (AC-4) |
| encoder panics or channel closes in `from_writer` | the stream ends with `Error::general_error("serialization task ended …")` |
| client disconnects | the body is dropped; the receiver drops; the encoder's next write fails and the task ends |

No `unwrap`; `Error::from_error(ErrorType::SerializationError, e)` for I/O errors in the adapter.

## Relevant Commands

None. No command is added or changed; `specs/command_registry.yaml` is unchanged.

## Documentation Architecture

- `specs/reference/VALUE_TYPE_SYSTEM.md`: the serializer's methods (`serialize_stream`,
  byte identity with `as_bytes`). History row, `reviewed:`.
- `specs/reference/ASSETS.md` §Status and reads: `get_serialized` row in the read matrix; streamed
  encodings are not cached. History row, `reviewed:`.
- `specs/reference/WEB_API_SPECIFICATION.md` §2.3 (streamed bodies; first chunk before headers;
  aborted body on a late error; entry routes collect), §7 (the timeout bounds time to first
  chunk), Appendix B (remove the streaming line). History row, `reviewed:`.
- `specs/guides/SERIALIZATION_GUIDE.md` §For implementers; `specs/guides/TYPE_SYSTEM_GUIDE.md`
  step 3 (optional override). History rows, `reviewed:`.
- `affects_docs`: the five above.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | core `value.rs`, `state.rs`, `assets.rs`; lib `value/simple.rs`, `value/extended.rs`, `value/mod.rs`; axum `api_core/{serving,response}.rs`, `axum_integration.rs`, `query/handlers.rs`, `assets/common.rs`, `assets/key_handlers.rs` |
| Crates and workflows affected | core, lib, axum; the Query API and Assets API `data`/`entry` routes |
| Existing tests likely to change | none by signature (`get_binary` unchanged); axum route tests keep passing for whole values |
| New validation | Phase 3: byte identity, streaming order, error-before-headers, aborted body, cache rule, timeout, DataFrame chunks |
| Compatibility / data / concurrency / performance / security | additive trait method; no stored format changes; a streamed response has no `Content-Length` (clients must accept chunked); each streamed request re-encodes (CPU for memory, by Q6); `spawn_blocking` holds a blocking-pool thread per streamed DataFrame response — bounded by tokio's pool; an aborted body is visible to HTTP/1.1 chunked and HTTP/2 clients as an error |
| Recovery | each step reverts alone; removing the overrides returns every value to `Whole` |
| Certainty and open questions | high for the default, the asset read and axum (`Body::from_stream` takes a `Send` stream of `Result<Bytes, E: Into<BoxError>>`; `liquers_core::error::Error` implements `std::error::Error`); medium for `from_writer` back-pressure tuning; Q1, Q2 fix the public shape |

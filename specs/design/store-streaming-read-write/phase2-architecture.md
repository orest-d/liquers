# Phase 2: Solution & Architecture — Streaming reads and writes on AsyncStore

## Overview

Two defaulted methods on `AsyncStore`: `get_stream` returns the object (or a range of it) as a
boxed stream of chunks plus its metadata; `set_stream` consumes a boxed stream and stores it with a
metadata record. The defaults collect through `get` and `set`, so every store is correct at once;
`AsyncFileStore` and `AsyncOpenDALStore` override them with their backend's native streaming, the
router and `SharedStore` forward, and the axum store routes become their first callers.

Rejected, one line each:
- **`open_write` returning a sink** (Q2): a two-step protocol whose close/abort every wrapper must
  forward correctly; a pulled stream needs one call.
- **`openbin(key, mode)` returning a reader/writer handle**: the Python shape; mode strings and
  `AsyncRead`/`AsyncWrite` trait objects are a larger API than either side needs.
- **Range as a separate `get_range` method**: `None` for the whole object keeps one method and one
  override per store.
- **`Vec<u8>` chunks** (Q1): a copy per chunk at OpenDAL and axum boundaries.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE` | draft | P2 | consumer of `set_stream`; shares the stream alias | no | no | record `required-by` |
| `STORE-ABSOLUTE-KEY-NOT-TYPE-ENFORCED` | draft | P3 | the new methods check `key.as_absolute()` first, as the trait doc requires | no | no | none |
| `LIBRARY-CODE-USES-UNWRAP-AND-EXPECT` | draft | P2 | `IntoResponse` for the streamed body must not `unwrap` the builder | no | no | build with `?`-free fallbacks |
| `STORE-OPENDAL-SLASH-HANDLING` (closed) | — | — | path mapping reused through the store's own path builder | — | — | none |

## Interfaces

`liquers-core/src/maybe_send.rs` (per target, as `BoxFuture`):

```rust
#[cfg(not(target_arch = "wasm32"))]
pub type BoxStream<'a, T> = Pin<Box<dyn futures::Stream<Item = T> + Send + 'a>>;
#[cfg(target_arch = "wasm32")]
pub type BoxStream<'a, T> = Pin<Box<dyn futures::Stream<Item = T> + 'a>>;
```

`liquers-core/src/store.rs`:

```rust
/// Chunks of a stored object, in order (Q1: `bytes::Bytes`).
pub type ByteStream<'a> = crate::maybe_send::BoxStream<'a, Result<bytes::Bytes, Error>>;

// AsyncStore, both defaulted (async_trait, ?Send on wasm32 as the rest of the trait):
/// The object at `key`, or bytes `range` of it, as chunks, with its metadata.
async fn get_stream(&self, key: &Key, range: Option<std::ops::Range<u64>>)
    -> Result<(ByteStream<'static>, Metadata), Error>;
/// Stores the chunks of `data` at `key` with `metadata`, as `set` would store their
/// concatenation. On an error of `data`, nothing of it remains visible under `key`.
async fn set_stream(&self, key: &Key, data: ByteStream<'_>, metadata: &Metadata)
    -> Result<(), Error>;
```

- **Default `get_stream`:** `key.as_absolute()?`, `self.get(key)`, slice by `range`
  (`clip_range(n, range)`, a private helper; `start >= n && n > 0` is an error), one chunk.
- **Default `set_stream`:** `key.as_absolute()?`, collect into a `Vec<u8>` (the first error is
  returned and nothing is written), then `self.set(key, &bytes, metadata)`.
- **`AsyncFileStore`:** read — `get_metadata(key)` (unchanged repair behaviour), open the file,
  `seek` to `range.start`, `futures::stream::unfold` over `read` into 64 KiB buffers up to
  `range.end`. Write — `acquire_lock`, `create_dir_all`, sidecar with `Status::Storing` and the
  finalized record (size unknown), write chunks with `write_all`, then the final sidecar with
  `file_size` = total and `seed_filename_from_key`; on a stream or I/O error remove the data file
  and the sidecar and return the error (Q3).
- **`AsyncOpenDALStore`:** read — `stat` + stored metadata as `get` builds it, then
  `op.reader_with(path)` and `Reader::into_bytes_stream(range)`; write — metadata `Storing` as its
  `set` does, `op.writer(path)`, `Writer::write` per chunk, `close()`, final metadata; on error
  `abort()`, then `delete` when the service is not in `ATOMIC_WRITE_SCHEMES`.
- **`AsyncStoreRouter`, `SharedStore`:** forward to the owning member / the inner store.
- **`MetadataPipelineStore`, `ReadOnlyStore`, test decorators, `liquers-web` and `liquers-py`
  stores:** no override (Q5).

**Range errors (implementation detail, resolved):** no new `ErrorType` and no new `Error`
constructor. `liquers-axum` checks the requested range against the key's `file_size`
(`get_asset_info`) before calling `get_stream` and answers `416` itself; the store-level check in
`clip_range` returns `Error::general_error` naming key, range and size, guarding direct callers.
This keeps the exhaustive error matches of `liquers-py`, `liquers-web` and `liquers-axum`
untouched.

## Integration Points

- `liquers-core/Cargo.toml`: `bytes = "1"`; tokio `io-util` added to the native tokio row.
- `liquers-core/src/maybe_send.rs`: `BoxStream`.
- `liquers-core/src/store.rs`: `ByteStream`, the two trait methods and defaults, `clip_range`,
  `AsyncFileStore`, `AsyncStoreRouter` and `SharedStore` overrides; the two `openbin` comments
  and the trait doc's "`openbin`, when it is implemented" sentence replaced.
- `liquers-store/src/opendal_store.rs`: overrides; the `openbin` comment removed.
- `liquers-axum/src/store/handlers.rs`: `get_data_handler` (parse one `Range`, `416` when out of
  bounds, `206` + `Content-Range`, `Body::from_stream`), `put_data_handler` (`Body` →
  `into_data_stream` mapped to `ByteStream`, `set_stream`). `liquers-axum/src/api_core/response.rs`
  and `axum_integration.rs`: a `StreamResponse` beside `BinaryResponse` (headers as today).
- `liquers-core/src/store_conformance/rules/streaming.rs` (new): `stream01`-`stream03`;
  `rules/mod.rs` `RULES`.

## Error Handling

| Failure | Result |
|---|---|
| relative key | `KeyNotAbsolute` (`as_absolute` first) |
| missing key | `KeyNotFound`, before the stream is returned |
| range start beyond the object | `Error::general_error` naming key and size; `416` from axum's pre-check |
| input stream error | that error returned; AC-4 cleanup; a cleanup failure is logged with `eprintln!` and the original error returned |
| backend I/O error mid-read | an `Err` item in the stream (the consumer stops); axum ends the body |

No `unwrap`; typed constructors only.

## Relevant Commands

None. No command namespace is involved; `specs/command_registry.yaml` is unchanged.

## Documentation Architecture

- `specs/reference/STORE_SEMANTICS.md` (reference, internal, `core/store`): new §17 "Streamed
  access" — equivalence with `get`/`set`, range clipping, AC-4 per atomicity, which stores override
  and that adapters collect. If `store-conditional-write` has taken §17, this becomes §18.
- `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` (guide): §1 ("a wrapper is not two methods": forward
  the two methods), §2 (when to override), §8 rows for `stream01`-`stream03`, §9 regenerated.
- `affects_docs`: the two above. No `specs/README.md` link.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-core`: `Cargo.toml`, `maybe_send.rs`, `store.rs`, `store_conformance/rules/{streaming,mod}.rs`; `liquers-store/src/opendal_store.rs`; `liquers-axum/src/store/handlers.rs`, `api_core/response.rs`, `axum_integration.rs` |
| Crates and workflows affected | core, store, axum; the HTTP store API (GET/PUT data) |
| Existing tests likely to change | none by signature; `liquers-axum/tests/store_api_routes.rs` keeps passing (`200` without `Range`) |
| New validation | unit tests per store, three conformance rules run by every suite, axum route tests for `206`/`416` and a large `PUT` |
| Compatibility / data / concurrency / performance / security | additive trait; same on-disk layout and `Storing` protocol; the file store holds its lock for the whole write (as `set`); memory bounded by chunk size; an unbounded `PUT` remains a policy question (no body limit is configured, unchanged) |
| Recovery | revert per step; defaults keep every store correct if an override is reverted |
| Certainty and open questions | high on mechanics (APIs read: `Reader::into_bytes_stream`, `Writer::abort`/`close`, tokio file I/O); Q1 and Q2 fix the signature |

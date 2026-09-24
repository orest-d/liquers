---
id: VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE
kind: feature
title: Serialization is synchronous and whole-value, so a large or lazy value cannot be served or stored as it is produced
status: draft
priority: P2
complexity: L
area: [core/value, core/store, core/assets, axum]
design:
created: 2026-09-24
github:
---
# Serialization is synchronous and whole-value, so a large or lazy value cannot be served or stored as it is produced

## Problem

Every path that turns a value into bytes goes through one synchronous call that returns the whole
encoding:

```rust
// liquers-core/src/value.rs:919-936
pub trait DefaultValueSerializer where Self: Sized {
    fn as_bytes(&self, data_format: &str) -> Result<Vec<u8>, Error>;
    fn deserialize_from_bytes(b: &[u8], type_identifier: &str, data_format: &str) -> Result<Self, Error>;
}
```

and everything downstream is shaped around a complete `Vec<u8>`:

| Layer | Where | Shape |
|---|---|---|
| Value | `DefaultValueSerializer::as_bytes` (`value.rs:926`), `State::as_bytes` (`state.rs:180`) | sync, whole encoding |
| Asset | the cached binary `binary: Option<Arc<Vec<u8>>>` (`assets.rs:530`); `AssetRef::serialize_to_binary` (`:3022`), `get_binary` (`:3416`), `poll_binary` (`:3560`) | whole encoding, cached on the asset |
| Asset → store | `save_to_store` (`assets.rs:2902`), `set_state` steps 7–8 (`:5607`, `store.set` at `:5706`), `set_binary` (`:6717`) | `as_bytes`, then `store.set` |
| Store | `AsyncStore::set(&self, key, data: &[u8], metadata)` (`store.rs:421`); `get`/`get_bytes` return `Vec<u8>` (`:391`, `:394`); `openbin` a `TODO` in every store (`CORE-STORE-OPENBIN-MISSING`) | whole object in, whole object out |
| HTTP | `BinaryResponse { data: Vec<u8> }` (`liquers-axum/src/api_core/response.rs:117`) → `Body::from(self.data)` (`axum_integration.rs:70`) | whole body |

That is fine for the values Liquers has today. It is wrong for a value that is **large and lazy** —
one whose bytes do not exist until they are produced, and whose production has to **await**. The
first such value is the record source of `design/record-streams/`: a `RecordSource` serialized *as
data* (CSV, NDJSON) must open a stream, evaluate chunk queries and pull batches, and none of that can
happen inside a synchronous `as_bytes`. It fails three ways at once:

- **Memory** — the whole encoding is resident before the first byte leaves, which is what the record
  design exists to avoid.
- **Latency** — nothing is sent or written until everything is produced.
- **Possibility** — a synchronous method cannot await, so for a lazy value the encoding cannot be
  produced *at all* through this path. `record-streams` Phase 2 therefore has non-manifest sources
  **refuse** `as_bytes` and be re-derived from their recipe.

`VALUE-SERIALIZATION-HAS-NO-INCREMENTAL-WRITER` records the first of these, for an in-memory value.
This issue is the wider pattern: **asynchronous, incremental serialization**, end to end — value,
asset, store and HTTP — with the store needing a **write-side `openbin`** to receive it.

## Why it surfaced now: the axum "ad-hoc streaming" is not implementable as designed

`record-streams` Phase 2 (§"Streaming a record source over HTTP") plans an ad-hoc branch in
`liquers-axum/src/axum_integration.rs` that recognizes a `RecordSource` and streams it with
`Body::from_stream`. Checking the code for this issue shows that branch **cannot be written where the
design puts it**:

- `liquers-axum` depends on `liquers-core` and `liquers-store` only (`liquers-axum/Cargo.toml:11-12`),
  not `liquers-lib`, so `ExtValue::RecordSource` is not a type it can name.
- Every handler is generic over `E: Environment` (`get_query_handler<E: Environment>`,
  `query/handlers.rs:15`), and reaches values only through the core traits.

The documented dependency flow (`liquers-lib ← liquers-axum`) would *permit* the dependency, but the
branch would then need `E: Environment<Value = liquers_lib::value::Value>`, specializing a router
that is deliberately generic. **A core-level asynchronous serialization hook removes the problem:**
the handler asks any value for a byte stream and never needs to know what a record source is. So the
"after-thought" is the first consumer of a missing general mechanism, and the ad-hoc path should be
built as that mechanism's first use rather than as a special case.

## Expected behaviour

A **pull-based byte stream** as the primitive form of serialization, with the existing forms derived
from it:

```rust
// Sketch only — the design owns the signature.
/// The value's byte form as a stream the consumer pulls. The default wraps `as_bytes` as a
/// single chunk, so every existing value has one and nothing changes until a value overrides it.
fn serialize_stream(self: Arc<Self>, data_format: &str)
    -> Result<SerializedBytes, Error>;

pub struct SerializedBytes {
    pub chunks: BoxStream<'static, Result<Bytes, Error>>,
    /// Known for a whole-value encoding (→ Content-Length, a store's size field); `None` when
    /// the value is produced lazily.
    pub length: Option<u64>,
}
```

- **Pull, not push.** An HTTP body is polled by hyper, and a store writer can be driven by a loop, so
  stream → writer is a loop while writer → stream is a channel and a task. The push form
  (`serialize_to_writer`) is then an adapter, and `as_bytes` a convenience that collects.
  (Established in `VALUE-SERIALIZATION-HAS-NO-INCREMENTAL-WRITER`'s 2026-09-20 update.)
- **In core**, on the value traits, so code generic over `E: Environment` — every axum handler, the
  asset manager — can reach it.
- **A default that routes through `as_bytes`**, so no existing value type changes.
- **A write-side `openbin`** on `AsyncStore`, so a stream can be stored without being collected:
  `set_stream(key, chunks, metadata)` or `openbin(key, Write)` returning an async sink. Its default
  collects and calls `set`, so every store keeps working and only a store that overrides it gains
  anything. OpenDAL's native `Writer` (multipart upload on object stores) is the natural real
  implementation.
- **Deserialization** gets the matching reader form when `openbin`'s read side lands; not required
  for the serving and storing cases.

### Questions the design must answer

1. **Where the environment comes from.** A lazy value may need the environment to produce bytes — a
   record source evaluates chunk queries through a `ChunkResolver`. `DefaultValueSerializer` has no
   environment. Pass an opaque capability into `serialize_stream`, or require the value to have
   captured one when it was built? The first is cleaner and changes the signature; the second leaks
   an environment into a value.
2. **`'static` ownership.** An HTTP body outlives its handler, so the stream must own what it
   serializes — hence `self: Arc<Self>` above (`State` already holds its data in an `Arc`). Confirm
   against `DefaultValueSerializer: Sized`, which permits an `Arc<Self>` receiver.
3. **wasm.** A boxed stream needs a per-target alias in `liquers-core/src/maybe_send.rs`, beside
   `BoxFuture`, because `StreamExt::boxed()` is always `Send`-boxed (E0225 prevents a `MaybeSend`
   trait-object bound). `record-streams` avoided needing one; a core-level pattern does need it.
4. **Mid-stream errors.** Once headers are sent a failure cannot become a status. Pulling the first
   chunk before responding converts most failures; NDJSON can carry an error in band; CSV cannot and
   truncates indistinguishably from success. The pattern should make "first chunk before headers"
   the default rather than each consumer's discovery. The same applies to a store write: a failed
   stream must not leave a partial object visible under the key.
5. **Storing: atomicity and metadata order.** A streamed write is only complete when the stream
   ends, so size, checksum and the final status are known last. The object must not be visible
   before then — temporary name and rename for the file store, multipart completion for object
   stores — and the asset's `Storing` status covers the interval.
6. **Caching.** A streamed encoding is not cached on the asset: `binary: Option<Arc<Vec<u8>>>` stays
   for whole-value encodings, and a streamed value produces none — which is also the answer to
   `SERIALIZED-BINARY-RETAINED-WITH-NO-DISPOSAL-POLICY` for that class. The *input* value stays
   cacheable, so only the encoding repeats per request.
7. **Should a lazy value be stored as data at all?** A record source normally stores its manifest,
   not its rows; `stored: false` exists precisely to avoid duplicating a database on disk. Writing
   a source's *data* into the store is materialization, which may belong to an explicit command
   (`store_record_stream`) rather than to the automatic persistence step.

## What in `liquers-axum` has to be refactored to follow the pattern

The serialization in the HTTP layer is spread over five response shapes and three different
serialization calls. Everything below is at HEAD on 2026-09-24.

### 1. Query execution — `query/handlers.rs`

| Where | Today | Under the pattern |
|---|---|---|
| `get_query_handler` (`:15`) and `post_query_handler` (`:164`) | Two copies of the same ~140-line loop: `env.evaluate`, then poll `asset_ref.poll_binary()` every 10 ms (`:61`, `:216`) until the asset's **cached binary** appears, with a status `match` for the terminal cases | One shared function. Wait for the value (not the binary); take the cached binary when one exists, otherwise ask the value for `serialize_stream`; pull the first chunk before building the response. The status arms stay |
| The response (`:62-64`, `:217-219`) | `BinaryResponse { data: (*data_arc).clone(), .. }` — **copies the whole cached binary on every request**, although it is already in an `Arc` | Hand the `Arc` (or `Bytes`) to the body; never copy |
| The 30 s timeout (`:42`, `:198`) | Bounds the whole wait for the binary (`AXUM-QUERY-TIMEOUT-HARDCODED`) | Must bound time-to-first-chunk; a long transfer is not a timeout |

### 2. The binary response type — `api_core/response.rs`, `axum_integration.rs`

| Where | Today | Under the pattern |
|---|---|---|
| `BinaryResponse { data: Vec<u8>, metadata }` (`response.rs:117`) | Owned bytes only | A body that is either **full** (`Bytes`/`Arc<Vec<u8>>`, with `Content-Length`) or a **stream** (`Body::from_stream`, chunked, no length) |
| `impl IntoResponse for BinaryResponse` (`axum_integration.rs:50-71`) | Sets `Content-Type` from `metadata.get_media_type()` and `X-Liquers-Status`, then `Body::from(self.data)` and `.unwrap()` on the builder | Same headers; body from either form; the `unwrap` removed (`LIBRARY-CODE-USES-UNWRAP-AND-EXPECT`) |

### 3. The assets API — `assets/handlers.rs`

| Where | Today | Under the pattern |
|---|---|---|
| `get_data_handler` (`:23`), serialization at `:74` | `state.data_unchecked().try_into_bytes()` — **not** the format-aware `as_bytes`. `try_into_bytes` accepts only `Value::Bytes` and `Value::Text` (`value.rs:695`) and refuses every extended value (`extended.rs:286`), so a number, a JSON object, a DataFrame or an image cannot be served here. Filed separately as `AXUM-ASSETS-API-SERVES-ONLY-BYTES-AND-TEXT` | The same serving function as the query handler, so `/api/assets/data` and `/q` cannot disagree about what a value's bytes are |
| `get_entry_handler` (`:200`), serialization at `:252` and `:262-291` | The same `try_into_bytes`, then a `DataEntry` envelope (§4) | As above, plus §4's decision |

### 4. The `DataEntry` envelope — `api_core/response.rs:71`, `api_core/format.rs:46`

`DataEntry { metadata: serde_json::Value, data: Vec<u8> }` is encoded **as one document** —
CBOR, bincode, or JSON with the bytes base64-encoded — by `serialize_data_entry`. It is used by:

- `assets/handlers.rs` `get_entry_handler` (`:262-291`),
- `store/handlers.rs` `get_entry_handler` (`:435`, the envelope built after `store.get` at `:455`)
  and `put_entry_handler` (`:506`, `store.set` at `:575`),
- `recipes/handlers.rs` `get_entry_handler` (`:142`, response at `:188-191`),
- `impl IntoResponse for DataEntry` (`axum_integration.rs:76`), which also `unwrap`s.

**An envelope that puts metadata and data in one document cannot stream as it is.** The design must
choose: keep the entry endpoints whole-value and refuse (or size-limit) a streamed value there,
pointing to the data endpoint; or change the entry encoding so the data can follow the metadata —
metadata in headers, `multipart/mixed`, or CBOR's indefinite-length byte strings. The first is
cheap and honest; the second changes a wire format clients already parse.

### 5. The store API — `store/handlers.rs` (the `openbin` sides)

| Where | Today | Under the pattern |
|---|---|---|
| `get_data_handler` (`:17`) | `store.get_bytes(&key)` (`:35`), whole object into memory, then `BinaryResponse` | `openbin` read side, streamed to the body — also what HTTP range requests need (`CORE-STORE-OPENBIN-MISSING`) |
| `put_data_handler` (`:55`) | Takes the whole request as `body: Bytes`, then `store.set` (`:80`) | The request body as a stream (`Body::into_data_stream`) into the write-side `openbin` |
| `upload_handler` (`:637`) | `field.bytes().await` per multipart field, then `store.set` (`:700`) | A multipart field is already a chunk stream; feed it to the write side |
| `put_entry_handler` (`:506`) | Whole `DataEntry` decoded, then `store.set` (`:575`) | Per §4 |

No request-body size limit is configured anywhere in the crate (no `DefaultBodyLimit`), so today an
upload is bounded only by axum's default; a streaming write removes the memory reason for a limit
but not the policy question.

### 6. The planned record-source branch

The ad-hoc branch `record-streams` Phase 2 puts in `axum_integration.rs` becomes **§1's streaming
case, with nothing record-specific in `liquers-axum`**. The parts of that design that carry over
unchanged: eager first batch before headers, the uniform-schema check before a CSV header (which
moves into the record source's own `serialize_stream`), NDJSON's in-band error line, and the
`EnvResolver` (question 1 above is where it enters).

### Out of scope for the refactor

Small, structured payloads stay whole: `ApiResponse<T>` JSON, metadata endpoints, `listdir`, the
recipe YAML at `recipes/handlers.rs:68`, and WebSocket notifications (`assets/websocket.rs`, text
messages only).

## Related

| Record | Relation |
|---|---|
| `VALUE-SERIALIZATION-HAS-NO-INCREMENTAL-WRITER` | The in-memory, push-based part of this. Under this pattern its `serialize_to_writer` is an adapter over the stream form. Should be folded into this one's design |
| `CORE-STORE-OPENBIN-MISSING` | The **read** side of `openbin`. This issue adds the **write** side, and the two should be designed together |
| `SERIALIZED-BINARY-RETAINED-WITH-NO-DISPOSAL-POLICY` | A streamed encoding is never cached, which removes that cost for the values most likely to be large |
| `NO-RECORD-STREAM-ABSTRACTION`, `design/record-streams/` | The first consumer; its axum section should be implemented through this mechanism |
| `AXUM-ASSETS-API-SERVES-ONLY-BYTES-AND-TEXT` | A correctness bug in §3, found while writing this; fixable on its own, before this |
| `AXUM-QUERY-TIMEOUT-HARDCODED` | The timeout's meaning changes for a streamed response |
| `LIBRARY-CODE-USES-UNWRAP-AND-EXPECT` | The response builders refactored here carry several of those `unwrap`s |
| `VALUE-CONVERSION-CAPABILITY` | Content negotiation (`Accept` → data format) is that issue's; this one is about *how* the chosen format is produced |

## Discovery

Raised 2026-09-24 in the `record-streams` Phase 2 review: the HTTP streaming section "is more of an
after-thought, but indicates a possible need for a wider pattern — asynchronous serialization of
large lazy structures, possibly requiring `openbin` when storing". Verified at HEAD while writing
this record: the dependency list of `liquers-axum`, every serialization call site in its handlers,
the whole-`Vec` signatures of `DefaultValueSerializer`, `AssetRef` and `AsyncStore`, and the
`try_into_bytes` defect in the assets API.

## Update 2026-09-24 — the first consumer has an interim path

`record-streams` Phase 2 no longer waits for this. A `RecordSource` serializes **only as its
manifest**; its rows reach bytes through an explicit, asynchronous `ns-rec/materialize` command
that returns a table, which then serializes synchronously through the ordinary path:
`-R/data/sales/daily.manifest.yaml/-/ns-rec/materialize/daily.csv`. That moves the await into
evaluation, where Liquers already has one, and needs no change in core, the store or `liquers-axum`.

What that means here:

- **Question 7 is answered for records:** a lazy value becomes data on disk only when someone writes
  `materialize`; the persistence step never does it on its own.
- **The need narrows to large exports.** The interim path holds the whole table in memory — about
  three times the data at serving time — and is capped by `max_rows`. It also cannot export a
  non-uniform source as NDJSON, which a streaming encoder could.
- **The query form is meant to survive this issue.** `…/ns-rec/materialize/daily.csv` — a
  materialization immediately serialized — is exactly what a streaming encoder can serve without
  building the table, so an implementation of this issue may stream that query without changing it.


# Phase 1: High-Level Design — A value's byte form as a pull stream

## Purpose

Every route from a value to bytes is the synchronous `DefaultValueSerializer::as_bytes`, which
returns the whole encoding, and HTTP sends nothing until it exists. Give values a **pull-based
byte stream** as their primitive byte form — whole for every existing type by default, streamed
for a type that can produce its encoding incrementally — and serve it over the Query and Assets
APIs as it is produced, so a large DataFrame export or (later) a record source's rows are not
held in memory twice before the first byte leaves. This serves the "data service" shape and the
DataFrames-and-records priority of `specs/reference/VISION.md`.

## Problem Example

```text
GET /liquer/q/-R/data/sales.csv/-/ns-pl/from_csv/sales.csv    # a 2 GB frame, unkeyed query
```

Today (HEAD 18732dd): `serve_query` (`liquers-axum/src/query/handlers.rs`) waits for
`AssetRef::get_binary`, which calls `State::as_bytes` → `ExtValue::as_bytes("csv")`
(`liquers-lib/src/value/mod.rs`), which runs polars' writer-based `serialize_dataframe_to_writer`
into a `Vec<u8>`. The whole CSV (several GB) is built and cached on the asset
(`AssetData::binary`), then cloned again into `BinaryResponse { data: (*data).clone(), .. }`. The
client receives nothing for the whole encoding time, and the 30 s query timeout bounds the
encoding as well as the evaluation.

Should be: the value hands out its CSV as a stream of chunks; the handler pulls the first chunk
(within the timeout), answers `200` with the headers, and streams the rest; the CSV is never whole
in memory and is not cached on the asset. A small value (a text, a JSON object) is served exactly
as today, from a cached whole encoding, with `Content-Length`.

## Scope and Acceptance Criteria

- **AC-1** Default is whole and identical
  WHEN `serialize_stream` is called on a type that does not override it, for every format its
  `as_bytes` accepts
  THEN it returns `SerializedBytes::Whole` with exactly the bytes of `as_bytes`, and the same error
  for a format `as_bytes` refuses
- **AC-2** A streamed value is served as it is produced
  WHEN the Query API serves a value whose `serialize_stream` returns a stream of N chunks
  THEN the response is `200` with the value's `Content-Type` and `X-Liquers-Status`, the body is the
  concatenation of the chunks, and the first chunk reaches the client before the last is produced
- **AC-3** An error before the first byte is a status
  WHEN the stream's first item is an error
  THEN the API answers that error's status and error body, exactly as a failed `get_binary` does
- **AC-4** A mid-stream error aborts the body
  WHEN a later item is an error
  THEN the body ends without its terminating chunk (the client sees an incomplete transfer, never a
  complete-looking short body), and the error is logged to stderr
- **AC-5** Streamed encodings are not cached; whole ones are
  WHEN a streamed value is served twice, and a whole value is served twice
  THEN the streamed value's asset has no cached binary and is encoded twice; the whole value is
  encoded once and cached exactly as `get_binary` caches it at HEAD
- **AC-6** The timeout bounds the first chunk only
  WHEN a streamed value's first chunk arrives within `QueryApiConfig::timeout` and its last after it
  THEN the transfer completes; WHEN the first chunk does not arrive in time THEN the timeout answer
  of HEAD is returned
- **AC-7** A DataFrame streams
  WHEN a polars DataFrame of 100 000 rows is served as `csv`
  THEN it is delivered in more than one chunk, none of them the whole encoding, and the body equals
  `as_bytes("csv")`
- **AC-8** Whole bytes are not copied on the way out
  WHEN a `Bytes` value (core `Value::Bytes`, `SimpleValue::Bytes`) is serialized in a format where
  `as_bytes` returns its bytes unchanged
  THEN `serialize_stream` returns `Whole` sharing the value's buffer
- **AC-9** Entry routes stay whole
  WHEN an `entry` route (`q/entry`, `key/entry`) serves a streamed value
  THEN the stream is collected and the `DataEntry` is the same as for its whole encoding
- **AC-10** Combined values forward
  WHEN a `liquers_lib::value::Value` holding a DataFrame or `SimpleValue::Bytes` is serialized
  THEN the override of the half it holds is used (AC-7, AC-8 hold through `CombinedValue`)

Non-goals (slice 1): lazy values that need the environment to produce bytes — record-source rows
(`RECORD-SOURCE-ROWS-CANNOT-BE-STREAMED`); persisting a streamed encoding through `set_stream`
without collecting it (`STREAMED-VALUE-IS-COLLECTED-BEFORE-IT-IS-STORED`); the store API's own
streamed reads, ranges and uploads (`store-streaming-read-write`); streaming `DataEntry` envelopes;
content negotiation (`VALUE-CONVERSION-CAPABILITY`); a streamed reader for deserialization;
`liquers-web`/`liquers-py` serving (they read whole bytes; the default keeps them working).

## Core Interactions

- Value types: `DefaultValueSerializer` gains a defaulted method; core `Value`, `SimpleValue`,
  `CombinedValue`, `ExtValue` override or forward.
- Assets: a stream-aware read beside `get_binary`, with the same read gate and cache rule.
- Web/API: the Query API and the Assets API `data` routes; `entry` routes collect.
- Store: none in this slice (persistence keeps `as_bytes`; the version hash needs whole bytes).

## Crate Placement

`liquers-core` owns the type (`SerializedBytes`), the trait method, `State` and asset reads, and a
push-to-pull adapter for writer-based encoders, so any `E: Environment` code — every axum handler
— reaches it without knowing value types. `liquers-axum` consumes it (depends on core only, as
now). `liquers-lib` overrides for `SimpleValue::Bytes`, `CombinedValue` and `PolarsDataFrame`.

## Documentation Intent

- Reference: extend `specs/reference/VALUE_TYPE_SYSTEM.md` (the serializer's methods),
  `specs/reference/ASSETS.md` §Status and reads (the stream read and its cache rule),
  `specs/reference/WEB_API_SPECIFICATION.md` §2.3 and §7 (streamed bodies, the first-chunk rule,
  the timeout's meaning; remove the Appendix B "streaming of large values" line).
- Guide: extend `specs/guides/SERIALIZATION_GUIDE.md` §For implementers (when to override, the
  byte-identity rule) and `specs/guides/TYPE_SYSTEM_GUIDE.md` step 3 (optional override).
- Other documents: none.
- Documents to update: the five above, History rows and `reviewed:` bumps.

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds a `pub` trait method, a `pub` enum and `pub` methods on
  `State`/`AssetRef` (rules 3, 4), spans `liquers-core`, `liquers-lib`, `liquers-axum` (rule 6), `L`.
- **Leading issue:** **Open design - environment for lazy values** (deferred to the follow-up, with
  a recommendation), then **Open design - primitive shape**.
- **Explanation:** slice 1 needs no environment, because nothing it streams is lazy; the trait
  method is defaulted, so a later environment-aware variant is additive. Every site was read at
  HEAD; the stream alias, `bytes` and the axum stream response come from
  `store-streaming-read-write`. Phases 3-4 assume the recommended answers.
- **Open questions** (the issue's seven, answered):
  1. **Open design - environment (issue Q1):** deferred to `RECORD-SOURCE-ROWS-CANNOT-BE-STREAMED`.
     Recommended there: an additive defaulted method generic over the environment,
     `serialize_stream_in<E: Environment<Value = Self>>(self: Arc<Self>, fmt, envref: &EnvRef<E>)`,
     defaulting to `serialize_stream`, so a record source builds an `EnvResolver` from the
     `EnvRef` the server already holds. Rejected: capturing a resolver in the value (a manifest
     read from a store has none, and an `EnvRef` inside a cached value is a reference cycle).
  2. **Open design - primitive shape:** `serialize_stream(self: Arc<Self>, data_format: &str)
     -> Result<SerializedBytes, Error>`, with `enum SerializedBytes { Whole(Bytes), Stream {
     chunks: ByteStream<'static>, length: Option<u64> } }` (recommended: `Whole` lets callers keep
     today's caching and `Content-Length` without inspecting a stream; `Arc<Self>` makes the stream
     `'static`, as an HTTP body needs — issue Q2; `DefaultValueSerializer: Sized` permits it), or
     always a stream (the issue's sketch; every caller must collect a one-chunk stream to cache it).
  3. **Proposed resolution - wasm (issue Q3):** reuse `maybe_send::BoxStream` and `ByteStream`
     from `store-streaming-read-write`; no second alias.
  4. **Proposed resolution - mid-stream errors (issue Q4):** first chunk pulled before headers by
     the shared serving function (AC-3); a later error aborts the body (AC-4). In-band error lines
     belong to formats that have them (NDJSON) and to the follow-up.
  5. **Proposed resolution - storing (issue Q5, Q7):** out of this slice. Persistence keeps
     `as_bytes`, since the content version (`Version::from_content`) needs the whole encoding; a
     lazy value is materialized only by an explicit command (`ns-rec/materialize`), as
     `record-streams` decided.
  6. **Proposed resolution - caching (issue Q6):** `Whole` is cached as today; `Stream` never is
     (AC-5). That also bounds `SERIALIZED-BINARY-RETAINED-WITH-NO-DISPOSAL-POLICY` for streamed types.
  7. **Proposed resolution - warnings:** stay the separate `serialization_warnings` method of
     `serialization-lossy-write-warnings`, applied where a `Whole` encoding is cached; warnings
     found mid-stream are the follow-up's.
  8. **Proposed resolution - entry routes (issue §4):** stay whole: a streamed value is collected
     (AC-9). Changing the envelope would change a wire format clients parse.

## Design Dependencies

- `store-streaming-read-write` (needs-decision) — **requires**: `maybe_send::BoxStream`,
  `store::ByteStream`, the `bytes` dependency and the axum `StreamResponse`. If it has not landed,
  step 1 here adds those exactly as it specifies. Shared contract (T2) but not merged: both `L`
  (overlap exclusion E2).
- `shared-metadata-and-bytes` (needs-decision) — **requires** its stages B and C: the cached binary
  and `BinaryResponse` as `Bytes` (AC-5) and `Value::Bytes(Bytes)` (AC-8). Not merged (E2).
- `value-serialization-writer` (needs-decision) — **overlaps** (T2 on the serializer trait, weak in
  effect): both wrap polars' `serialize_dataframe_to_writer`; this design adapts it to a pull
  stream directly, so neither orders the other. Its open question 1 ("standalone or folded into
  this feature") is answered: standalone — a synchronous push `write_to` is still the right API for
  a caller holding a file, and this design's `from_writer` can take any such encoder.
- `serialization-lossy-write-warnings` — **overlaps** (T2, weak): its `serialization_warnings`
  must also run where `get_serialized` caches a `Whole`. Either order; the second adds the call.
- `serialized-binary-disposal-policy` — **overlaps**, weak: streamed encodings are never cached.
- `record-streams` (complete, E4) — origin; its HTTP section is realised by the follow-up issue.

## Consolidated Findings

- Since the issue was written, `liquers-axum` merged its query handlers into one `serve_query`, made
  the timeout configurable (`QueryApiConfig`), serves the Assets API through `get_binary`
  (`AXUM-ASSETS-API-SERVES-ONLY-BYTES-AND-TEXT` closed) and builds responses with `build_or_500`.
  What remains is the whole-value shape and the `(*data).clone()` per request.
- Keyed, non-volatile assets serialize whole at finalization to compute their content version
  (`AssetRef::prepare_version`), so they keep a cached `Whole` and gain nothing from streaming
  until a disposal policy or streamed persistence changes that; unkeyed query results and volatile
  assets are where streaming helps now.
- Byte identity between `serialize_stream` and `as_bytes` is the contract (AC-1, AC-7): versions
  hash `as_bytes`.
- Follow-ups filed: `RECORD-SOURCE-ROWS-CANNOT-BE-STREAMED`,
  `STREAMED-VALUE-IS-COLLECTED-BEFORE-IT-IS-STORED`.

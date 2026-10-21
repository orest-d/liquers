# Phase 1: High-Level Design — Streaming reads and writes on AsyncStore

## Purpose

Let a caller read a stored object as a stream of chunks (optionally a byte range) and write one
from a stream, without holding the whole object in memory, so the HTTP store API can serve large
objects and range requests and accept large uploads, and so incremental serialization
(`VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE`) has a store to write into. This replaces the
`openbin` that every store carries as a commented-out Python leftover.

## Problem Example

```
curl -H 'Range: bytes=0-1023' http://localhost:3000/api/store/data/big/video.mp4
curl -T 4GB.parquet http://localhost:3000/api/store/data/big/4GB.parquet
```

Today: `get_data_handler` (`liquers-axum/src/store/handlers.rs`) calls `store.get_bytes(&key)`, so
the first request reads the whole object into memory and returns all of it with `200` (the `Range`
header is ignored); `put_data_handler` takes the body as one `Bytes` and calls `store.set`, so the
second needs 4 GB of RAM. `AsyncStore` has nothing else to call: `// TODO: implement openbin` in
`liquers-core/src/store.rs` (twice) and `liquers-store/src/opendal_store.rs`. Should be: the first
returns `206` with 1024 bytes read from the backend's range, the second streams the body to the
store chunk by chunk, with peak memory bounded by a chunk.

## Scope and Acceptance Criteria

- **AC-1** Streamed read equals `get`
  WHEN `get_stream(key, None)` is read to the end on any store
  THEN the concatenated chunks equal `get_bytes(key)` and the metadata equals `get_metadata(key)`
- **AC-2** Range read
  WHEN `get_stream(key, Some(a..b))` is read on a key of size `n`
  THEN the chunks are bytes `a..min(b, n)`; `a >= n` (with `n > 0`) is an error, and the store
  API answers such a request with `416`
- **AC-3** Streamed write equals `set`
  WHEN `set_stream(key, chunks, metadata)` completes
  THEN `get` returns the concatenated chunks and a record whose `file_size` is their total, exactly
  as `set` with the same bytes would
- **AC-4** A failed write leaves no partial object
  WHEN the input stream yields an error part-way
  THEN `set_stream` returns that error, and the key afterwards holds either its previous object
  (a store with `atomic_write`) or nothing — never a prefix of the new bytes
- **AC-5** Bounded memory where the backend allows it
  WHEN `AsyncFileStore` or `AsyncOpenDALStore` streams an object in either direction
  THEN no buffer larger than one chunk (64 KiB default) is held per call (checked by a store that
  counts the largest chunk it received)
- **AC-6** Every store keeps working
  WHEN a store or wrapper does not override the methods (memory store, pipeline, read-only
  adapter, web and Python-backed stores)
  THEN the defaults give AC-1 to AC-4 by collecting, through `get`/`set`
- **AC-7** HTTP uses it
  WHEN the store API serves `GET` with a single `Range: bytes=a-b` or accepts `PUT`
  THEN it answers `206` with `Content-Range` for the range (`200` without one) and streams the
  `PUT` body into `set_stream`

Non-goals: value serialization as a stream (the value-layer issue); multi-range requests;
`upload_handler` multipart streaming and `DataEntry` envelopes (follow-up); streaming through the
metadata pipeline and `ReadOnlyStore` (they keep the collecting default; follow-up); a Python or
JavaScript streaming API.

## Core Interactions

- **Store:** two defaulted `AsyncStore` methods, overridden by the file, OpenDAL and router stores.
- **Web/API:** the `liquers-axum` store data routes. **Bindings:** inherit the defaults.

## Crate Placement

`liquers-core` (trait, core stores, stream alias), `liquers-store` (OpenDAL native reader and
writer), `liquers-axum` (consumers). The dependency flow is unchanged.

## Documentation Intent

- Reference: extend `specs/reference/STORE_SEMANTICS.md` with a section on streamed access
  (equivalence with `get`/`set`, range rule, failure rule per atomicity).
- Guide: extend `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` §1-§2 (what a store may override and
  why the default is correct) and the rules table.
- Other documents: none.
- Documents to update: the two above; the issue's resolution note.

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds `pub` methods to `AsyncStore` and a `pub` type alias,
  and a tokio feature (rules 3 and 4); `L`.
- **Leading issue:** **Open design - chunk type:** `bytes::Bytes` (new `liquers-core` dependency)
  or `Vec<u8>`.
- **Explanation:** both backends have native streaming (`tokio::fs::File`; OpenDAL `Reader` /
  `Writer`), the defaults make every other store correct unedited, and the HTTP routes are the only
  consumers needed now. The open choices fix the trait signature, so they are decisions; Phases
  3-4 assume the recommendations.
- **Open questions:**
  1. **Open design - chunk type:** `bytes::Bytes` (recommended: zero-copy from OpenDAL's `Buffer`
     and to and from axum bodies; the same type the value-layer issue sketches, so the two
     designs share one stream type) or `Vec<u8>` (no new dependency; a copy per chunk at both ends).
  2. **Open design - write shape:** the store pulls a stream, `set_stream(key, stream, metadata)`
     (recommended: one call, and the value-layer issue settled on pull), or the caller pushes into
     a sink returned by `open_write` (two-step, needs a close/abort protocol in the trait).
  3. **Proposed resolution - failure rule:** AC-4 as stated; `AsyncFileStore` writes in place under
     its lock and the `Storing` marker exactly as `set` does, and on a stream error removes the
     data object and its sidecar (its previous object is already gone, as with a failed `set`).
  4. **Proposed resolution - names:** `get_stream` / `set_stream`; the `openbin` comments go.
  5. **Proposed resolution - adapters:** the pipeline and `ReadOnlyStore` keep the collecting
     default in this design; streaming through them needs data-only stream methods, recorded as a
     follow-up issue when this design lands (Phase 4).

## Design Dependencies

- `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE` (feature, no design) — **required-by**: its
  write-side store API is `set_stream`; the stream alias and chunk type are shared (Q1). Not
  merged: both are `L`, the merge would be `XL` (overlap exclusion E2).
- `value-serialization-writer` — **overlaps**, weak: a push writer for values; non-goal there.
- `store-conditional-write` — **overlaps**, weak: both add defaulted `AsyncStore` methods and
  touch the same wrappers; independent methods, either order.
- `store-command-namespace` — **overlaps**, weak: large `save`/`get` values; no shared interface.

## Consolidated Findings

- Additive: two defaulted trait methods; no implementor must change; adapters collect (Q5).
- Needs tokio `io-util` in `liquers-core` and a per-target `BoxStream` alias (E0225).
- OpenDAL fs is not `atomic_write`: AC-4 there is `Writer::abort` plus `delete`.
- Three conformance rules change `STORE_IMPLEMENTATION_GUIDE.md` §9 counts.

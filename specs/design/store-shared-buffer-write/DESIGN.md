---
id: STORE-SHARED-BUFFER-WRITE
kind: design
title: A whole write of a shared buffer reaches the store without a copy
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/store, store/backends]
issues: [STORE-WHOLE-WRITE-COPIES-CALLER-BYTES]
created: 2026-10-26
---
# A whole write of a shared buffer reaches the store without a copy

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented. The source was filed `L`; Phase 2 found a solution with no public
signature change, in `liquers-core` plus a check in `liquers-store`, so it was re-sized to `M` and
the compact form is used. Requires `store-streaming-read-write` and `shared-metadata-and-bytes`
stage B (both unfinished).

## Phase 1: High-Level Design

### Purpose

Writing a value the caller already holds in a shareable buffer (`bytes::Bytes`) should not copy
it: a keyed asset's cached binary, or a freshly serialized `Vec<u8>`, can be handed to the memory
store and to OpenDAL as is. Today `AsyncStore::set(&self, key, data: &[u8], metadata)` borrows, so
every store that keeps or forwards the bytes copies them (VISION: data service — memory in
proportion to the data once).

### Problem Example

```rust
// save_to_store, after shared-metadata-and-bytes stage B: binary is a 200 MB `Bytes`
store.set(key, &binary, &metadata).await?;
```

At HEAD (cd6d5da) `AsyncMemoryStore` keeps `Arc::from(data)` (copy) and `AsyncOpenDALStore::set`
builds `Buffer::from_iter(data.iter().copied())` (`liquers-store/src/opendal_store.rs`, a
byte-by-byte copy; stage B reduces it to one `memcpy`). Should be: the store receives the `Bytes`
handle; the memory store keeps it and OpenDAL passes it to its writer, so 200 MB are not
duplicated.

### Scope and Acceptance Criteria

- **AC-1** The memory store keeps the caller's buffer
  WHEN a `Bytes` is written to `AsyncMemoryStore` as one whole write and read back with `get`
  THEN the bytes read share the written buffer (same pointer and length)
- **AC-2** OpenDAL writes without copying the payload
  WHEN a `Bytes` is written as one whole write to an OpenDAL `memory` / `fs` store
  THEN the payload reaches the operator as that `Bytes` (no `Buffer::from_iter`, no `to_vec`), and
  the stored object and record equal what `set` produces for the same bytes
- **AC-3** Keyed persistence uses it
  WHEN a keyed asset with a cached binary is persisted by `save_to_store`, and when
  `AssetManager::set_state` persists a serialized value
  THEN the store receives the buffer without a copy on the memory store (AC-1 observable through
  `get` sharing the asset's binary)
- **AC-4** Every other store is unchanged
  WHEN a store that does not override the streamed write (pipeline, read-only, web, Python,
  test decorators) receives such a write
  THEN it is stored exactly as through `set`, with at most the copy `set` makes today
- **AC-5** No public signature changes
  WHEN the change is complete
  THEN `set`, `set_data` and every `AsyncStore` implementor outside `liquers-core` and
  `liquers-store` compile unchanged

Non-goals: changing `set` / `set_data` to take `Bytes`; the HTTP upload handlers (`put_data` is
streamed by `store-streaming-read-write`; `entry` and multipart uploads stay); data-only writes
(`set_data`); `liquers-web` and `liquers-py` stores.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — a performance change, not a fix restoring documented
  behaviour (rule 2); readiness is not `ready` (rule 5).
- **Leading issue:** **Open design - the write path for a shared buffer.**
- **Explanation:** a one-chunk `set_stream` already says "store these bytes as `set` would", so the
  stores that can keep a buffer only need to keep a single chunk; no trait method is added and no
  caller outside the asset layer changes. Phases 3-4 assume the recommendation.
- **Open questions:**
  1. **Open design - write path:** (a) a one-chunk `set_stream` (recommended: no new trait method;
     the router and `SharedStore` already forward `set_stream` after `store-streaming-read-write`;
     the default gains a single-chunk fast path so it is never worse than `set`); (b) a defaulted
     `set_bytes(key, Bytes, metadata)` beside `set` (explicit, but a third write method every
     forwarding wrapper must remember, and a missed forward silently copies); (c) `set` takes
     `Bytes` (about 285 `set` and 42 `set_data` call sites, mostly tests, and every implementor
     including `liquers-web` and `liquers-py`); (d) leave whole writes copying.
  2. **Proposed resolution - OpenDAL:** rely on `store-streaming-read-write`'s `set_stream`
     override, which hands each chunk to `Writer::write` (`Bytes` → `Buffer` without a copy), and
     verify it with AC-2; no `liquers-store` code change unless it collects.
  3. **Implementation detail (resolved):** the memory store's `set` and its new `set_stream`
     override share one private `set_owned(key, Bytes, metadata)`; `set` calls it with
     `Bytes::copy_from_slice(data)` (the copy it makes today).

### Design Dependencies

- `store-streaming-read-write` (needs-decision) — **requires**: `set_stream`, `ByteStream`, its
  default and the OpenDAL override this design relies on.
- `shared-metadata-and-bytes` (needs-decision) — **requires** stage B: the memory store holding
  `Bytes` (its AC-6) and the asset's cached binary as `Bytes`. This issue is its open question 3's
  follow-up.
- `streamed-keyed-persistence` (this batch) — **overlaps**, weak: it adds a separate streamed write
  before finalization; this design changes the whole-value write in `save_to_store`. The
  same-change-site test was considered and does not hold. Either order.
- `serialized-binary-disposal-policy` — **overlaps**, weak: sharing the cached binary with the
  memory store means releasing it on the asset no longer frees it while the store holds it; correct,
  and expected for a memory store.

## Phase 2: Architecture

### Solution

Whole writes from the asset layer go through `set_stream` with one chunk. The memory store
overrides `set_stream` to keep a single chunk as its stored buffer (several chunks are
concatenated once). The trait default of `set_stream` (from `store-streaming-read-write`) gets a
single-chunk fast path: when the stream yields exactly one chunk it calls
`self.set(key, &chunk, metadata)` without collecting into a new `Vec`, so a store that does not
override is never worse than `set`. OpenDAL is covered by its existing override. Rejected: options
1 (b)-(d) above.

### Changes

- `liquers-core/src/store.rs`
  - `AsyncStore::set_stream` default: pull the first item; if the stream then ends, `set(key,
    &first, metadata)`; otherwise collect as specified there. Errors as specified there.
  - `AsyncMemoryStore`: private `set_owned(&self, key, data: Bytes, metadata)` holding today's
    `set` logic (record finalization, sibling rule, `file_size`); `set` → `set_owned(key,
    Bytes::copy_from_slice(data), md)`; new `set_stream` override: one chunk → `set_owned(key,
    chunk, md)`, several → concatenate then `set_owned`.
- `liquers-core/src/assets.rs`
  - `save_to_store`: `store.set(key, &data, &metadata)` → `store.set_stream(key,
    one_chunk(data.clone()), &metadata)` where `data: Bytes` (stage B) and `one_chunk` is a private
    helper `fn one_chunk(b: Bytes) -> ByteStream<'static>` (`futures::stream::once`, boxed).
  - `AssetManager::set_state` (`DefaultAssetManager` and `ImmediateAssetManager`, the
    `store.set(key, &binary, …)` after `state.as_bytes()`): `set_stream(key,
    one_chunk(Bytes::from(binary)), …)` (the `Vec` is moved, not copied).
  - `set_binary` keeps `set`: its `binary: &[u8]` is a borrowed public parameter.
- `liquers-store/src/opendal_store.rs`: none if its `set_stream` override passes chunks to
  `Writer::write`; otherwise make it do so (AC-2).
- No `pub` signature change; no command, format or binding change. Errors are those of
  `set` / `set_stream`.
- Documents: `specs/reference/STORE_SEMANTICS.md` streamed-access section (a one-chunk
  `set_stream` is the zero-copy whole write; the default's fast path) and
  `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` (override `set_stream` to keep a single chunk when
  the store can). History rows, `reviewed:` bumps.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A store's `set_stream` override differs from its `set` (metadata, `file_size`, sibling rule) | `store-streaming-read-write`'s conformance rules (stream01-03) run on every suite; AC-2 compares records | high |
| OpenDAL's writer path behaves differently from `op.write` for one chunk on some service (multipart thresholds) | AC-2 on `memory` and `fs`; services with multipart thresholds write one part below the threshold | medium |
| Sharing the asset's binary with the memory store keeps it alive after the asset releases it | intended for a memory store; documented | high |
| Existing tests assert `set` was called (counting wrappers in `tests/fixtures`) | the counting wrappers forward `set_stream` to `set` through the default fast path, so counts stay | medium |

Recovery: revert the two asset call sites to `set`; the store-side changes are inert without them.

## Phase 3: Examples and Tests

### Examples

The Problem Example, after the change: `save_to_store` calls `set_stream(key, one_chunk(binary),
&metadata)`; on `AsyncMemoryStore`, `store.get(key)` returns a `Bytes` with
`as_ptr() == binary.as_ptr()`.

### Tests

- `liquers-core/src/store.rs` `mod tests`:
  - `memory_store_one_chunk_write_shares_buffer` (AC-1): `set_stream` one chunk, `get` shares the
    pointer.
  - `memory_store_set_still_copies_and_matches` (AC-4): `set` and one-chunk `set_stream` of the
    same bytes give equal data and records.
  - `default_set_stream_single_chunk_calls_set_once` (AC-4): a recording store with no override sees
    one `set` with the chunk's bytes and no other call.
- `liquers-core/tests/store_conformance_CONF.rs`: the memory suite already runs the stream rules
  of `store-streaming-read-write`; no new rule (AC-4).
- `liquers-core/tests/shared_buffer_write.rs` (new):
  - `keyed_persist_shares_cached_binary_with_memory_store` (AC-3): keyed recipe `data/x.txt`
    (`counted/x.txt`, validates with `--command counted`), evaluate, wait `Persisted`; the store's
    bytes share the asset's `poll_binary()` buffer (run before `serialized-binary-disposal-policy`
    releases it, or compare with a recording store that captures the chunk).
  - `set_state_moves_serialized_vec_to_store` (AC-3): `set_state` with a text value; the recording
    store sees one chunk via `set_stream`.
- `liquers-store/src/opendal_store.rs` `mod tests`: `opendal_one_chunk_write_equals_set` (AC-2): `memory` and
  `fs` schemes; data and record equal to `set`'s.
- AC-5: `cargo test -p liquers-lib --lib --tests` and `cargo check -p liquers-axum` compile
  unchanged.

Run: `cargo test -p liquers-core --lib store::tests::`, `CARGO_INCREMENTAL=0 cargo test -p
liquers-core --test shared_buffer_write`, `cargo test -p liquers-store`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. Entry check — `store-streaming-read-write` (`set_stream`, default, OpenDAL override) and
  `shared-metadata-and-bytes` stage B have landed; if not, stop. Proof: `grep -n 'fn set_stream'
  liquers-core/src/store.rs`.
- [ ] 2. `liquers-core/src/store.rs` default `set_stream` single-chunk fast path —
  `default_set_stream_single_chunk_calls_set_once` — `cargo test -p liquers-core --lib store::`.
  Rollback: revert.
- [ ] 3. `AsyncMemoryStore::set_owned`, `set` through it, `set_stream` override —
  `memory_store_one_chunk_write_shares_buffer`, `memory_store_set_still_copies_and_matches`, the
  memory conformance suite — `cargo test -p liquers-core --lib --tests`. Rollback: revert.
- [ ] 4. `liquers-store/src/opendal_store.rs` — confirm or make the `set_stream` override pass
  chunks to `Writer::write` without copying; `opendal_one_chunk_write_equals_set` —
  `cargo test -p liquers-store`. Rollback: revert.
- [ ] 5. `liquers-core/src/assets.rs` `one_chunk`, `save_to_store`, both managers' `set_state` —
  `tests/shared_buffer_write.rs` — `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests`.
  Rollback: revert the call sites.
- [ ] 6. Docs: `STORE_SEMANTICS.md`, `STORE_IMPLEMENTATION_GUIDE.md` (History, `reviewed:`); close
  `STORE-WHOLE-WRITE-COPIES-CALLER-BYTES` with a resolution note — `python3 scripts/docs_index.py
  --check`.

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-store`; `cargo test -p
liquers-lib --lib --tests`; `bash scripts/check-build-matrix.sh` (wasm32: `ByteStream` is `?Send`
there); after `cargo clean`, the `liquers-web` Node loop (its stores inherit the default, AC-4);
diff review: no `pub` signature change (AC-5), no debug output.

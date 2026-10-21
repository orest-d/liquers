# Phase 1: High-Level Design — Share metadata records and stored bytes

## Purpose

Two immutable payloads are copied where they could be shared: a metadata record (a 624-byte
struct with a growing log) is cloned on every asset read, and a stored object's bytes are copied
at the store, the value and the HTTP boundary. Holding the record behind an `Arc` inside
`Metadata` (copy-on-write) and using `bytes::Bytes` as core's byte type makes those boundaries
reference-count increments, so serving large binaries and reading assets costs memory in
proportion to the data once — the "data service" shape of `VISION.md`.

## Problem Example

```text
GET /liquer/q/-R/data/image.png        # a 50 MB object in an OpenDAL store
```

Today, at HEAD 18732dd:

1. `AsyncOpenDALStore::get` (`liquers-store/src/opendal_store.rs`) reads an `opendal::Buffer`
   and returns `buf.to_vec()` — copy 1 (the memory store copies too: `data.as_ref().to_vec()`);
2. `Step::GetResource` (`liquers-core/src/interpreter.rs`) moves the `Vec` into `Value::Bytes`;
3. `AssetRef::get_binary` → `State::as_bytes` → `Value::as_bytes("b")` clones the `Vec` — copy 2 —
   and caches it as `Arc<Vec<u8>>` (`AssetData::binary`);
4. `serve_query` (`liquers-axum/src/query/handlers.rs`) builds
   `BinaryResponse { data: (*data).clone(), metadata: (*metadata).clone() }` — copy 3, plus a
   deep copy of the metadata record.

Every `AssetRef::get()` on the same asset also deep-copies its metadata: `exposed_value_state`
(`liquers-core/src/assets.rs`) does `self.metadata.clone()` and then writes two type fields.

Should be: one buffer, shared from the store to the asset (steps 1, 2 and 4 copy nothing), and
one metadata record shared by the asset, every state read from it and a `Value::Metadata` made
from it. Step 3's encoding copy is removed by `value-serialization-streaming` (a `Bytes` value
hands its buffer out as its encoding); this design makes it possible.

## Scope and Acceptance Criteria

- **AC-1** Metadata clones share the record
  WHEN a `Metadata::MetadataRecord` is cloned
  THEN both clones point at one record (`Arc::ptr_eq` on `record_arc()`)
- **AC-2** Copy on write
  WHEN one of two clones is changed (`add_log_entry`, `set_status`, `record_mut`)
  THEN the other is unchanged, and the changed clone no longer shares the record
- **AC-3** Asset reads do not copy metadata
  WHEN `AssetRef::get()` is called twice on a `Ready` asset with nothing changing in between
  THEN the two states' records are `Arc::ptr_eq`, and equal to `AssetRef::get_metadata()`'s
- **AC-4** A metadata value shares the state's record
  WHEN `to_metadata` runs on a state
  THEN the resulting `Value::Metadata` holds the state's record (`Arc::ptr_eq`)
- **AC-5** Wire formats unchanged
  WHEN a `Metadata`, a core `Value::Bytes` and a `SimpleValue::Bytes` are written as JSON and YAML,
  and stored metadata written at HEAD is read back
  THEN the output is byte-identical to HEAD and the read succeeds
- **AC-6** A store read hands out the store's buffer
  WHEN `AsyncMemoryStore::get` reads one key twice
  THEN both `Bytes` point at the same buffer, and `AsyncOpenDALStore`, `AsyncFileStore` and every
  conformance suite return the same content as at HEAD
- **AC-7** A resource value shares the store's buffer
  WHEN `-R/data/x.bin` is evaluated against a memory store holding `x.bin`
  THEN the `Value::Bytes` of the result points at the buffer a second `store.get` returns
- **AC-8** The cached binary is served without a copy
  WHEN `AssetRef::get_binary` is called twice, and the Query API serves the same asset
  THEN both reads return one shared buffer, and the HTTP body equals it

Non-goals: store **writes** taking `Bytes` (`set`/`set_data` keep `&[u8]`; large writes go through
`set_stream` in `store-streaming-read-write`; the remaining copy is filed as
`STORE-WHOLE-WRITE-COPIES-CALLER-BYTES`); `as_bytes` returning `Bytes`, and removing step 3's
encoding copy (`value-serialization-streaming`); sharing `LegacyMetadata`; `try_into_bytes`
(an owned-copy API by contract); `liquers-records`' `ChunkResolver::read_resource` (keeps
`Vec<u8>`; converts at its boundary); making `State::metadata` a plain `Metadata`.

## Core Interactions

- Value types: `Metadata`, `MetadataRecord` access; core `Value::Bytes`, `Value::Metadata`;
  `SimpleValue::Bytes`; `ValueInterface::from_metadata`, `from_bytes`.
- Store: `AsyncStore::get`, `get_bytes`, `get_data` return `Bytes`; every implementor.
- Assets: `AssetData::binary`, `metadata`; the binary read family; `AssetManager::get_binary_any_status`.
- Web/API: `liquers-axum` `BinaryResponse`, the Query and Assets data routes.
- Commands: `to_metadata`, `to_assetinfo` (no signature change; registry unchanged).

## Crate Placement

`liquers-core` owns the types (`Metadata`, `Value`, `AsyncStore`, assets) and gains the `bytes`
dependency (already in the tree through tokio; `liquers-store`, `liquers-lib`, `liquers-axum` use
1.12). Implementors follow in `liquers-store`, `liquers-lib`, `liquers-web`, `liquers-axum` and the
test stores of `liquers-records`. Nothing moves between crates.

## Documentation Intent

- Reference: extend `specs/reference/VALUE_TYPE_SYSTEM.md` §What a value variant may hold (`Bytes`
  is O(1) to clone; `Metadata` shares its record), `specs/reference/ASSETS.md` §AssetData
  (`binary: Bytes`), `specs/reference/STORE_SEMANTICS.md` §11 (reads return `Bytes`).
- Guide: extend `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` (return the backend's buffer, do
  not copy into a `Vec`).
- Other documents: none.
- Documents to update: the four above, each with a History row and `reviewed:` bump.

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — changes `pub` signatures of `AsyncStore`, `ValueInterface`,
  `AssetRef`, `Metadata` and the payloads of `pub` enums (rule 4), adds a dependency (rule 4),
  spans six crates (rule 6), `L`.
- **Leading issue:** **Open design - byte type:** `bytes::Bytes` throughout.
- **Explanation:** every site was inventoried at HEAD (Phase 2); the change is mechanical behind
  three type swaps, compile errors find every site, and wire formats are pinned by AC-5. Phases
  3-4 assume the recommended answers; the maintainer prefers no backward compatibility, which the
  signature changes rely on.
- **Open questions:**
  1. **Open design - byte type:** `bytes::Bytes` (recommended: zero-copy from `opendal::Buffer`
     when contiguous, consumed by axum and hyper directly, and the chunk type
     `store-streaming-read-write` proposes, so whole and streamed bytes are one type), `Arc<[u8]>`
     (no dependency, but a copy at both OpenDAL and axum), or `opendal::Buffer` (ties core to
     OpenDAL).
  2. **Open design - sharing in the value trait:** `ValueInterface::from_metadata` takes
     `Arc<MetadataRecord>` and `from_bytes` takes `Bytes`; `Metadata::metadata_record()` returns
     `Option<Arc<MetadataRecord>>`. Recommended: change them (three implementors each, about ten
     callers). `core-value-enum-oversized` kept them owned on 2026-10-12 because no caller could
     avoid a copy yet; after this design every caller can.
  3. **Open design - writes:** keep `set(&[u8])` (recommended: whole writes of large objects are
     `set_stream`'s job; changing `set` touches ~280 call sites for one copy) or take `Bytes`.
  4. **Proposed resolution - where copy-on-write lives:** inside `Metadata`
     (`MetadataRecord(Arc<MetadataRecord>)`, `Arc::make_mut` in its `&mut self` methods), so every
     holder — asset, state, store, value — shares without knowing; `State::metadata` stays
     `Arc<Metadata>` (a second pointer, but no churn across hundreds of readers).
  5. **Proposed resolution - wire format:** `Value::Bytes` and `SimpleValue::Bytes` serialize
     through a `serde(with)` helper that writes a sequence of integers, as `Vec<u8>` does, rather
     than the `bytes` crate's `serde` feature (byte strings in CBOR).

## Design Dependencies

- `core-value-enum-oversized` (ready) — **requires**: it puts `Value::Metadata` (and
  `SimpleValue::Metadata`) behind `Arc<MetadataRecord>`; AC-4 shares into that variant. Land it
  first.
- `store-streaming-read-write` (needs-decision) — **overlaps** (T2 on the byte type, weak
  otherwise): both add `bytes` to `liquers-core` and should use `Bytes`; whichever lands first adds
  the dependency. Different `AsyncStore` methods; either order.
- `value-serialization-streaming` — **required-by**: it hands a `Bytes` value's buffer out as its
  encoding and serves `Bytes` bodies; it needs stage B (cached binary as `Bytes`). Not merged:
  both `L`, merged `XL` (overlap exclusion E2).
- `serialized-binary-disposal-policy` (needs-decision) — **overlaps**, T3: both edit
  `AssetData::binary`; its `Arc::ptr_eq` becomes a pointer-and-length comparison of `Bytes`.
  Either order; the second rebases.
- `serialization-lossy-write-warnings`, `metadata-application-attributes` — **overlaps**, weak:
  they add `Metadata` mutators, which after stage A go through `record_mut`. Either order.
- `combined-value-shared-data-node` — **overlaps**, weak: same goal (share, do not copy) for data
  nodes; no shared site.
- `state-lock-marker-cleanup` — **overlaps**, weak: documents `State`'s handles; kept apart by E1.

## Consolidated Findings

- Three independent stages, each one PR: **A** metadata (`liquers-core` and three callers),
  **B** `Bytes` for store reads, asset binaries and HTTP, **C** `Value::Bytes(Bytes)`. B before C;
  A independent of both.
- The per-read metadata copy is the larger cost in practice: every `get()` copies the record and
  its log. It disappears only if the type-field sync on read becomes a comparison (Phase 2), since
  any write through a shared `Arc` copies.
- The `Arc::make_mut` sites are found by the compiler: assignment through `&mut Arc<T>` does not
  compile, so no mutation can silently skip copy-on-write.
- Wire formats are kept by construction (manual `Serialize` for `Metadata`; a `serde(with)` helper
  for bytes) and checked by AC-5; stored metadata is untouched.
- OpenDAL writes copy byte by byte today (`Buffer::from_iter(data.iter().copied())`); stage B
  replaces it with one `memcpy` while the write signature stays.
- Follow-up filed: `STORE-WHOLE-WRITE-COPIES-CALLER-BYTES`.

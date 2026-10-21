---
id: SHAREABLE-METADATA-AND-BYTES-ARE-DEEP-COPIED
kind: issue
title: Metadata and stored bytes are deep-copied where they could be shared
status: draft
priority: P3
complexity: L
area: [core/value, core/store, core/assets]
design: shared-metadata-and-bytes
created: 2026-10-12
github:
---
## Problem

Two kinds of immutable payload are copied where they could be shared.

**Metadata.** `State::metadata` is `Arc<Metadata>` (`liquers-core/src/state.rs`), but `Metadata`
is an enum over `MetadataRecord` and legacy JSON, so the record inside cannot be shared on its
own. Every consumer that needs a `MetadataRecord` clones it, a 624-byte struct with nested
`String`s and `Vec`s (measured 2026-10-12). Examples: the `to_metadata` command
(`liquers-lib/src/commands.rs`) and `interpreter.rs` `Step::GetResourceMetadata`, which build
`Value::Metadata` from a cloned record; after `core-value-enum-oversized` that variant holds
`Arc<MetadataRecord>`, but it cannot reuse the state's record.

**Bytes from a store.** The byte path copies at every boundary:

- `AsyncStore::get` / `get_bytes` return `Vec<u8>` (`liquers-core/src/store.rs`); the OpenDAL
  store reads an `opendal::Buffer`, which is already reference-counted, and copies it with
  `buf.to_vec()` (`liquers-store/src/opendal_store.rs` `get_bytes`);
- the asset holds `Option<Arc<Vec<u8>>>` (`AssetData::binary`), a pointer to a pointer;
- `Value::Bytes(Vec<u8>)` copies again when a value is made from it, and `AsyncStore::set` takes
  `&[u8]`, so a store that keeps the bytes (the memory store) copies once more.

## Problem Example

```text
GET -R/data/image.png   (a 50 MB file in an OpenDAL store)
today:    opendal::Buffer -> Vec<u8> (copy) -> Arc<Vec<u8>> in the asset -> Vec<u8> in
          Value::Bytes (copy) -> response body (copy, depending on the axum handler)
expected: one shared, reference-counted buffer from the store to the response, with no copy
```

## Impact

Memory and time proportional to payload size on every read; for large binaries several copies
coexist. No correctness problem; no workaround.

## Expected behaviour

A refactoring of the core data structures toward shareable references, in two parts:

1. **Metadata:** `Metadata::MetadataRecord(Arc<MetadataRecord>)` (or an equivalent) so a state,
   an asset and a `Value::Metadata` share one record, with copy-on-write (`Arc::make_mut`) where
   metadata is updated.
2. **Bytes:** one general, shareable byte type from the store to the consumer. Prior art:
   - [`bytes::Bytes`](https://docs.rs/bytes) — the de facto standard (tokio, hyper, axum): a
     cheaply cloneable, sliceable, reference-counted contiguous buffer; `liquers-axum`,
     `liquers-lib` and `liquers-store` already depend on `bytes`.
   - [`opendal::Buffer`](https://opendal.apache.org/docs/rust/opendal/struct.Buffer.html) —
     OpenDAL's read type, either one contiguous `Bytes` or a non-contiguous sequence of `Bytes`
     chunks, cheap to clone; it converts to `Bytes` without a copy when contiguous.
   - [sled](https://github.com/spacejam/sled)'s `IVec` — the key-value store's value type: short
     values inline, longer ones behind an `Arc<[u8]>`, so a read hands out a shared reference to
     the stored bytes. This is most likely the store the maintainer remembered.
   - [fjall](https://docs.rs/fjall)'s `Slice` — the same idea in an LSM-tree key-value store: a
     cheaply cloneable, reference-counted byte slice returned by reads.

   Recommended direction (to be decided in the design): `bytes::Bytes` as the core type, since
   every other one converts into it cheaply and axum consumes it directly; `AsyncStore::get`
   returning it; `AssetData::binary: Option<Bytes>`; `Value::Bytes(Bytes)` (serde through
   `serde_bytes` or the `bytes` crate's `serde` feature). It touches every store implementation
   (including `liquers-web`'s JavaScript store and `liquers-py`), so it is `L`.

## Discovery

Raised by Orest on 2026-10-12 while deciding `core-value-enum-oversized` (its question 3): keep the
`ValueInterface` signatures owned for now, but "MetadataRecord can benefit from sharing", together
with "a general way how to handle the bytes coming from the store". The store-side facts were
checked at HEAD; the prior-art descriptions come from each project's documentation and were not
checked against the current versions of sled and fjall. Weak overlap with
`VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE` (streaming a value as it is produced): a
shared byte type is a likely building block for it, but neither covers the other.

## Update 2026-10-20: re-checked at HEAD, designed

Still reproduces at HEAD 18732dd: `Metadata::MetadataRecord(MetadataRecord)` (`metadata.rs`),
`AssetData::binary: Option<Arc<Vec<u8>>>`, `AsyncStore::get` returning `Vec<u8>`, OpenDAL
`buf.to_vec()` in `get_bytes`/`get_data`, the memory store's `data.as_ref().to_vec()`,
`Value::Bytes(Vec<u8>)`, and `(*data).clone()` in `liquers-axum` `serve_query` and `asset_bytes`.
Two further copies were found:

- every `AssetRef::get()` deep-copies the asset's metadata record and log
  (`AssetData::exposed_value_state` clones `self.metadata` and writes the type fields);
- both managers' `set_state` deep-clone the value they install
  (`Arc::new(state.data_unchecked().as_ref().clone())`).

The problem example above is a `GetAsset` (`-R/data/image.png` loads the keyed asset by the fast
track): the read bytes become the cached binary, so the axum clone is the second whole copy; the
`Step::GetResource` path is `-R-stored/…`.

Designed in [`shared-metadata-and-bytes`](../design/shared-metadata-and-bytes/phase1-high-level-design.md)
in three stages (metadata; `Bytes` for store reads, asset binaries and HTTP; `Value::Bytes(Bytes)`).
Store writes taking `Bytes` were scoped out and filed as `STORE-WHOLE-WRITE-COPIES-CALLER-BYTES`.

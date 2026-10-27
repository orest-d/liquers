---
id: STORE-WHOLE-WRITE-COPIES-CALLER-BYTES
kind: issue
title: A whole-object store write copies bytes the caller already holds in a shareable buffer
status: draft
priority: P3
complexity: M
area: [core/store, store/backends]
design: store-shared-buffer-write
created: 2026-10-21
github:
---
## Problem

`AsyncStore::set(&self, key, data: &[u8], metadata)` and `set_data(&self, key, data: &[u8])`
(`liquers-core/src/store.rs`) borrow the bytes, so a store that keeps or hands them on must copy:
`AsyncMemoryStore` stores `Arc::from(data)`; `AsyncOpenDALStore::set` builds
`Buffer::from_iter(data.iter().copied())` (`liquers-store/src/opendal_store.rs`), a byte-by-byte copy.
The asset layer writes its cached binary, which after `design/shared-metadata-and-bytes/` stage B
(not landed at HEAD cd6d5da) is a `bytes::Bytes` that OpenDAL and the memory store could take
without a copy; `AssetManager::set_state` writes a freshly serialized `Vec<u8>` the same way.

## Problem Example

```rust
// a keyed asset's 200 MB cached binary, persisted by save_to_store
store.set(&key, &binary, &metadata).await?;   // binary: Bytes; the store copies 200 MB
```

Today: one full copy per write (two for OpenDAL counting the iterator copy, which
`shared-metadata-and-bytes` reduces to one `memcpy`). Should be: the store takes a `Bytes` handle
and shares it, as `Operator::write(path, Bytes)` accepts.

## Impact

Memory and time proportional to the object on every whole write; no correctness problem. Large
objects can avoid it through the streamed write `set_stream` (`design/store-streaming-read-write/`).

## Expected behaviour

Options, not chosen here: `set`/`set_data` take `Bytes` (about 280 call sites, mostly tests, and
every implementor); or a defaulted `set_shared(key, Bytes, metadata)` beside `set`, overridden by
the stores that can keep the buffer; or leave whole writes copying and rely on `set_stream` for
large objects.

## Discovery

Scoped out of `design/shared-metadata-and-bytes/` (its open question 3) during the 2026-10-20 bulk
design, while inventorying the byte path of `SHAREABLE-METADATA-AND-BYTES-ARE-DEEP-COPIED`.

Designed 2026-10-26 in `design/store-shared-buffer-write/` (bulk design). Re-sized `L` → `M`: the
recommended route (a one-chunk `set_stream`, kept by the stores that can) changes no public
signature and sits in `liquers-core` plus a check in `liquers-store`.

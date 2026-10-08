---
id: STORES-DISAGREE-ON-SEEDING-THE-DATA-FORMAT-FROM-THE-KEY
kind: issue
title: Some stores derive a file's data format from its key's extension and others do not
status: draft
priority: P3
complexity: S
area: [core/store, web]
design: store-key-format-seeding
created: 2026-10-07
github:
---

## Problem

The effective media type (`MetadataRecord::get_media_type`) derives from the data format, and the
data format is seeded from the *filename* (`MetadataRecord::set_filename` / `with_filename`). Whether
the key's own extension counts depends on the store:

- liquers-web's HTTP store `infer_metadata` sets `data_format = csv` for `input.csv`.
- `AsyncMemoryStore::set(&parse_key("data/input.csv")?, bytes, &Metadata::new())` stores the key but
  no filename and no format, so `get_asset_info` reports `application/octet-stream`.
  `finalize_metadata` (`liquers-core/src/store.rs`) sets the key, size and status, not the filename.

`reference/STORE_SEMANTICS.md` does not say which is right.

## Impact

The same bytes written under the same key report different media types depending on the backend.
A page calling `store.set("input.csv", bytes, {})` and then `getAssetInfo` gets `text/csv` over HTTP
and `application/octet-stream` over a memory or local-storage store.

## Expected behaviour

Decide in `STORE_SEMANTICS.md` whether a store seeds the filename (and so the format) from the key
when the written metadata has none, and make the stores agree, with a conformance test.

## Discovery

Found 2026-10-07 implementing `design/js-store-effective-media-type/`: its Phase 3 example E1 writes
`{}` and expects `text/csv`, which the memory-store test could not reproduce.

---
id: WEB-JS-STORE-BYTES-WITHOUT-METADATA-READ-AS-BINARY
kind: issue
title: A page store returning data without metadata may read a .txt key as bytes, not text
status: draft
priority: P3
complexity: S
area: [web, core/store]
design:
created: 2026-10-31
github:
---

## Problem

**Suspected, not yet reproduced.** When a page-implemented store's `get` returns `{data}` with no
`metadata`, `metadata_from_js_value` (`liquers-web/src/store/mod.rs`) builds a `MetadataRecord`
with only the key set. It does not seed the filename or data format from the key, as
`AsyncStore::default_metadata` / `finalize_metadata` do for core's stores (`FileStore::get`). With no
`data_format`, deserialization falls back to `bin` (`liquers-core/src/assets.rs`
`deserialize_stored_value`), so `-R/notes/a.txt` would yield bytes rather than a string.

## Impact

A minimal page store gives results whose type depends on whether it supplied metadata, unlike every
Rust store. Workaround: return `metadata` with a `filename` or `data_format`.

## Expected behaviour

Missing metadata is filled from the key exactly as for a Rust store without metadata. A test
evaluates a `.txt` key through a store returning `{data}` alone and expects a string.

## Discovery

Codebase review of the `liquers-py-store` Phase 1 (2026-10-31), which specifies the same seeding for
the Python adapter. Reasoned from the code; verify on a wasm build before fixing.

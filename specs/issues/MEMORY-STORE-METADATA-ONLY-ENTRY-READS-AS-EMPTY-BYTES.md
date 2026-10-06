---
id: MEMORY-STORE-METADATA-ONLY-ENTRY-READS-AS-EMPTY-BYTES
kind: issue
title: AsyncMemoryStore answers a metadata-only entry with empty bytes, indistinguishable from empty content
status: draft
priority: P2
complexity: S
area: [core/store]
design: memory-store-metadata-only-entry
created: 2026-10-02
github:
---
## Problem

`AsyncMemoryStore::set_metadata` on a key with no data inserts an empty data object
(`liquers-core/src/store.rs`, `set_metadata`: `Arc::<[u8]>::from(Vec::<u8>::new())`), and
`get` / `get_bytes` then return `Ok(vec![])`. `AsyncFileStore` keeps the two apart: a sidecar
without a data file answers `get_bytes` with `KeyNotFound`. So on a memory store "the store holds
metadata but no data object" (a value that could not be serialized, stored metadata-only by
`set_state`; or bytes deleted with metadata kept) cannot be told from "the value is the empty byte
string".

## Impact

Part G of `dependency-audit-and-expiry-provenance` re-hashes stored bytes and must *skip* a key
with no bytes but *check* an empty data object. On a memory store it cannot tell which it has.
Step 9 works around it in `assets.rs` (`no_bytes_by_design`): empty bytes under a *timestamp*
version are taken for a metadata-only entry, because Liquers records a timestamp version only when
it stored no bytes. That is a heuristic. A legacy (unflagged) hash that happens to read as a
timestamp, on a value emptied by hand, is skipped instead of detected. The same ambiguity feeds
`METADATA-ONLY-ENTRY-RELOADS-AS-CORRUPTED`, where the fast track deserializes the empty bytes.

## Expected behaviour

A store reports "no data object" the same way on every backend — for example `get_bytes`
returning `KeyNotFound` for a metadata-only memory entry — with a conformance rule that pins it.
Alternatively, metadata-only entries carry an explicit marker (the option
`METADATA-ONLY-ENTRY-RELOADS-AS-CORRUPTED` lists). Either would let `no_bytes_by_design` be
removed.

## Discovery

Found 2026-10-02 while implementing Step 9 (`verify_stored_versions`, the `missing_bytes_are_skipped`
and `empty_data_object_is_checked_not_skipped` tests), when a metadata-only memory entry with a
`new_unique` version would have been reported as changed outside Liquers and converted to
`Override`.

## Update 2026-10-06 — it serves a wrong value, not only a misleading log

Reproduced while fixing `FAST-TRACK-FAILS-ON-METADATA-ONLY-FILE-STORE-ENTRY`: on a memory store, a
metadata-only `Text` entry stored as `txt` (status `Ready`) is **served as the empty string**. The
fast track reads the empty data object, `deserialize_stored_value` accepts empty `txt` bytes as
`""`, and the recipe never runs. The corruption log described in
`METADATA-ONLY-ENTRY-RELOADS-AS-CORRUPTED` appears only for formats that reject empty input. Priority
raised from P3 to P2 because this is a wrong value. The fast-track fix that makes a `KeyNotFound`
answer safe has landed, so this store change can now be made on its own (design
`memory-store-metadata-only-entry`).


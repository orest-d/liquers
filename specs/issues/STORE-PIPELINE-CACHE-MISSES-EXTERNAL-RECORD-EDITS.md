---
id: STORE-PIPELINE-CACHE-MISSES-EXTERNAL-RECORD-EDITS
kind: issue
title: The metadata pipeline's cache ignores a record object edited outside the process
status: draft
priority: P3
complexity: S
area: [core/store]
design:
created: 2026-10-15
github:
---
## Problem

`MetadataPipelineStore` caches each key's record, valid while the key's *data* facts are unchanged
(`liquers-core/src/store/pipeline.rs`, `read_record`, `CacheEntry`). A sidecar, folder record or
grouped file edited by another program while the data stays the same is not noticed until the
data changes or the process restarts. Phase 2 of `design/store-adapters/` described a per-group
cache; the code caches per key.

## Impact

Two processes sharing a store with `per_entry` or `grouped` storage can serve each other stale
titles and other record fields. Data versions are unaffected.

## Expected behaviour

The cache entry also records the facts of the record object(s) it was read from, and a read
compares both, or the limitation is chosen deliberately and documented as such.

## Discovery

Found while documenting `design/store-adapters/` (Phase 5).

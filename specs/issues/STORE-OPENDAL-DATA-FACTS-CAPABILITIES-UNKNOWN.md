---
id: STORE-OPENDAL-DATA-FACTS-CAPABILITIES-UNKNOWN
kind: issue
title: An OpenDAL store under-reports its data capabilities because OpenDAL does not say what stat returns
status: closed_not_planned
priority: P3
complexity: S
area: [store/backends]
design:
created: 2026-10-15
github:
---
## Problem

`AsyncOpenDALStore::data_capabilities` reports `modified_time: false` and `etag: false` for every
service (`liquers-store/src/opendal_store.rs`), because OpenDAL 0.55's capability set has no
field saying whether `stat` returns them. `data_facts` still reports both when a service returns
them. `atomic_write` comes from a fixed list of object-store schemes (`ATOMIC_WRITE_SCHEMES`)
rather than from OpenDAL.

## Impact

Small today: no write strategy or handler checks `modified_time` or `etag`, and `data_facts` passes
real values through. But the advertised capabilities misdescribe S3, GCS and similar services,
so any future build-time check on them would refuse stores that work, and the fixed scheme list
for `atomic_write` (consumed by `liquers-core/src/store/writes.rs`) can go stale as services are
added.

## Expected behaviour

Derive the three capabilities per service: from OpenDAL if a later version exposes them, else
from a maintained per-scheme table checked by a test against a live `stat`.

## Discovery

Found implementing step 13 of `design/store-adapters/`.

## Resolution (2026-10-16)

Accepted as a known limitation (the user's decision): OpenDAL does not say what `stat` returns, so `AsyncOpenDALStore` keeps reporting `modified_time` and `etag` as false while passing real values through `data_facts`. Added as a fallback: when a backend reports no modification time, the pipeline records the time of the write as Liquers saw it as `created` (`MetadataPipelineStore::record_backend_facts`, test `created_falls_back_to_the_write_time`); `updated` is already the current time. Documented in `STORE_SEMANTICS.md` §13.

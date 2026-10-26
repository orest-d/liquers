---
id: VALUE-HAS-NO-MEMORY-SIZE-ESTIMATE
kind: feature
title: A value cannot report its memory size, so the asset cache cannot be bounded in bytes
status: draft
priority: P3
complexity: L
area: [core/value, lib/value, core/assets]
design: value-memory-size-budget
created: 2026-10-21
github:
---

## Problem

`ValueInterface` (`liquers-core/src/value.rs`) and `ExtValueInterface` (`liquers-lib`) have no
method that estimates how much memory a value holds. The asset manager therefore cannot enforce a
byte budget on its cache. On 2026-10-10 the maintainer asked for one in `CORE-ASSET-GC`: an
internet-facing service bounds its in-memory cache in bytes, separately for recipes and for ad-hoc
queries. `asset-garbage-collection` bounds entries by count and idle time only, for this reason.

## Problem Example

A server with `assets: { max_cached_queries: 1000 }` (the count limit of
`asset-garbage-collection`) answers ad-hoc `-R/data/big<i>.csv/-/ns-pl/from_csv` queries, each
holding a 400 MB frame. Ten of them fit the count limit and take 4 GB, and there is no setting that
says "at most 1 GiB", because no value can report its size.

## Impact

A service can bound how many assets it caches, but not how much memory they take. A few large
DataFrames can exceed the memory a count limit was chosen for. Workaround: a conservative count
limit.

## Expected behaviour

A provided `ValueInterface` method such as `fn estimated_size(&self) -> Option<usize>` (default
`None`), implemented for the core variants and for each `ExtValue` variant (Polars frames, images,
record views, UI elements). Then a `max_cached_bytes` option (per map) beside the count limits of
`asset-garbage-collection`, counting the value estimate plus any cached binary
(`AssetData::binary`). Open: whether a value whose size is unknown counts as zero or as a fixed
default.

## Discovery

Split off on 2026-10-21 from `CORE-ASSET-GC` during bulk design (`asset-garbage-collection`
Phase 1, open question 2).

Designed 2026-10-26 in `design/value-memory-size-budget/` (bulk design). The estimate (part A)
can land before `asset-garbage-collection`; the byte budget (part B) requires it.

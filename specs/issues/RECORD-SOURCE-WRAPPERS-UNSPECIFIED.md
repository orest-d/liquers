---
id: RECORD-SOURCE-WRAPPERS-UNSPECIFIED
kind: issue
title: Phase 2 names filtering/mapping RecordSource wrappers but gives no signature
status: draft
priority: P3
complexity: M
area: [records]
design: record-streams
created: 2026-09-26
github:
---

## Problem

`specs/design/record-streams/phase2-architecture.md`'s Trait Implementations table (§"Trait
Implementations") names, alongside `ManifestSource` and `InMemorySource`, "wrappers — a filtering
source and an asynchronously mapping one" as further `RecordSource` reference implementations.
Phase 4's implementation plan (§Step 4.2) repeats "`InMemorySource` and the wrapping sources" as
something that step implements.

Neither §"The types" nor §"Function Signatures" — the two sections that pin every other type this
crate implements (`ManifestSource`, `InMemorySource`, `ChunkNaming`, `ChunkTemplate`, `ChunkOrigin`,
...) — gives either wrapper a struct name, a field list, or a constructor. Searching all of
`phase2-architecture.md`, `phase3-tests.md` and `phase3-examples.md` turns up no concrete signature,
no test, and no example query that would exercise one. There is nothing declared to implement.

## Impact

Low today: `liquers-records/src/sources.rs` (Step 4.2) implements `ManifestSource` and
`InMemorySource` per their concrete signatures, and no test in Phase 3 or Phase 4 depends on a
wrapping source existing. But the gap will resurface the first time a command author needs "filter
a `RecordSource` without materializing it" or "map each chunk asynchronously" — `RecordView` already
has `filter`/`select_columns`/`take` (Step 2.4), but nothing analogous composes over a *source*
(async, chunked) rather than a view (sync, in memory).

## Expected behaviour

Before implementing either wrapper, a short design note should settle:

- **A filtering source**: does it filter chunks (skip whole chunks a predicate rejects, using
  `ChunkList`/`describe_chunk`) or rows within each chunk (apply `RecordView::filter` to every
  batch a wrapped source's stream yields)? These have different costs and different answers to
  "does `chunks()` still enumerate the same set".
- **An asynchronously mapping source**: what does the mapping function see and return per chunk —
  `Arc<dyn RecordView> -> BoxFuture<Result<Arc<dyn RecordView>, Error>>`, most likely, mirroring
  `futures::StreamExt::then` — and does the wrapped source's own `schema()`/`manifest()` still
  apply, or does the wrapper's mapping invalidate them (a schema-changing map needs a new declared
  schema, which nothing currently gives it a place to declare)?
- Whether either wrapper needs a name at all, or whether `RecordSource::stream`'s caller composing
  `futures::StreamExt` combinators directly over the returned `BoxRecordStream` (as
  `ManifestSource::stream`'s own body already does) already covers the use case Phase 2 had in
  mind, in which case the table entry should be corrected rather than the code written.

## Discovery

While implementing `liquers-records/src/sources.rs` (Phase 4, Step 4.2), which names
`ManifestSource`, `InMemorySource` and "the wrapping sources" as its three deliverables. The first
two have concrete signatures in §"The types"/§"Function Signatures"; grepping both phase documents
for a signature, field list or test for the third found nothing, so Step 4.2 implements only what
is declared and records this gap instead of guessing at one.

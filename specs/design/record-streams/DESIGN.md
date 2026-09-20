---
id: RECORD-STREAMS
kind: design
title: Record streams — a lightweight, chunked, Arrow-interoperable tabular abstraction
workflow: liquers-project
status: draft
phase: architecture
area: [lib/value, lib/commands, web, axum]
gh_pr: []
issues: [NO-RECORD-STREAM-ABSTRACTION, NO-RELATIONAL-DATABASE-ACCESS-LAYER, NO-RECIPE-PROVIDER-CHAIN, RECIPE-CONTAINS-DEFAULT-ASSUMES-ENUMERABILITY]
affects_docs: [reference/RECORD_STREAMS.md, guides/RECORD_STREAM_GUIDE.md, reference/VALUE_TYPE_SYSTEM.md, guides/TYPE_SYSTEM_GUIDE.md, guides/COMMAND_REGISTRATION_GUIDE.md]
created: 2026-09-19
superseded_by:
---
# Record streams — design tracking

**Created:** 2026-09-19

## Phase Status

- [x] Phase 1: High-Level Design — approved 2026-09-20
- [ ] Phase 2: Solution & Architecture — in review
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Origin: split out of `store-and-asset-search`

This design was **extracted from [`store-and-asset-search`](../store-and-asset-search/)** on
2026-09-19. That design set out to make stores and assets searchable, and six Phase 2 revisions
established that the interesting half was not the search at all: it was the **record stream** the
search filters. Records serve four consumers of which search is one — search, external engines, SQL
and serialization — and they carry requirements search never raises, notably multi-gigabyte lazy
processing and per-chunk provenance.

So records are stabilized **first**, and search is rebuilt on top. The search design keeps its
predicate, syntax, parser and the `get_asset_info` repair, and depends on this one for everything
tabular.

The reasoning that produced the model is preserved rather than re-derived:

- [`record-model.md`](./record-model.md) — records, streams, chunks, batches and schema; identity as
  a pair; why the refresh unit is the chunk and the memory unit the batch. Moved here from the search
  design, where it was written.
- The search design's Phase 2 revisions 3–4 established the columnar, Arrow-laid-out form and the
  schema-owns-the-roles model; both are carried into this design's Phase 1 and Phase 2 rather than
  left in a document about search.

## What this design owns

Defining, storing, manipulating and serializing a **stream of records**, with:

1. **Interoperability** with GlueSQL, polars, pandas, Arrow, Qdrant, Tantivy, Lucene and external
   relational databases — via an Arrow-compatible memory layout, **without binding a heavy Arrow
   library**.
2. **A `Value` variant**, so a record stream is an ordinary Liquers value: addressable, cacheable,
   serializable, and produced by commands.
3. **A lightweight DataFrame-like abstraction** that does not rely on polars and therefore works in
   `liquers-web`, which is wasm32 and cannot bundle it.
4. **Lazy, chunked processing of datasets that do not fit in memory** — multi-gigabyte tables, with
   one chunk resident at a time.
5. **Metadata, provenance and validity per chunk**, flyweighted down to the record level, so a record
   knows where it came from and whether it is still current without paying per-record storage.

## What it does not own

Search — the predicate, its syntax, its parser and the matching contract — stays in
`store-and-asset-search`. Indexation policy and the interoperability *layer* (feeding and
reconciling an external engine) also stay there, because they are about keeping a **view** fresh
rather than about what a record is.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Record model](./record-model.md)
- [Chunking and resumability](./chunking-and-resumability.md) — unknown chunk counts, generated chunk queries, store-backed resumption
- [Engine survey](./engine-survey.md) — is the field-role model right, and can records front a relational database?
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

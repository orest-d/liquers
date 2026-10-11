---
id: EXTERNAL-INDEX-SYNC
kind: design
title: Keeping external indexes and engines in sync with record sources
workflow: liquers-project
status: draft
phase: high-level
area: [records, core/assets, lib/commands]
issues: [ASSET-EXPIRATION-EVENTS-CANNOT-BE-OBSERVED-EXCEPT-PER-ASSET]
affects_docs: []
created: 2026-10-10
---
# Keeping external indexes and engines in sync with record sources — design tracking

**Created:** 2026-10-10, split out of [`store-and-asset-search`](../store-and-asset-search/) when
that design was refocused on in-tree, record-based search commands (its revision 8).

## Phase Status

- [ ] Phase 1: High-Level Design — not started; the analysis below is its input
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## What this design is for

Some systems answer questions that an in-tree scan cannot answer well. These are a full-text
search engine (Tantivy, Elasticsearch, Meilisearch), a vector store (Qdrant, pgvector), a RAG
pipeline, and an external SQL mirror. Each one is a **materialized view of a record source**. This
design is how such a view is fed and kept correct. It also covers in-tree derived indexes, such as
per-document word filters, that let search stop being a full scan.

The search commands in `store-and-asset-search` are the in-tree baseline. They scan a record
source on every query and keep nothing between queries. This design is where anything kept between
queries belongs.

## Inherited analysis

These two documents were written for `store-and-asset-search` (rounds three and seven of its
Phase 2) and moved here unchanged on 2026-10-10:

- [`interoperability-layer.md`](./interoperability-layer.md) covers one layer for plugging in a
  search engine, vector store, RAG pipeline or SQL mirror: the partition, the reconciliation diff,
  the staleness declaration, and the Exact/Inexact/Unsupported report.
- [`indexation-policy.md`](./indexation-policy.md) covers which documents are indexed, whether
  content is read or produced, the four document classes, and why volatile assets need a second
  refresh regime. Two of its rules stay with the search baseline as well:
  - §1, "indexation may evaluate; search may not";
  - the type rule in §9, not to extract text from non-text media types.

Background that stays in the search design folder:
- [`research-questions.md`](../store-and-asset-search/research-questions.md): §3 (Tantivy, Whoosh),
  §4 (GlueSQL), §8 (embeddings), §9 (semantic search), §10 (one interoperability layer) and §11
  (tinysearch) are this design's input.
- [`options-analysis.md`](../store-and-asset-search/options-analysis.md): axes E and I, and the
  external-engine rows of §5.

Per-engine `FieldRole` mappings are in
[`record-streams/engine-survey.md`](../record-streams/engine-survey.md). The search design's
milestones M4 and M7 are in the archived roadmap
([`archive/2026-10-10-store-and-asset-search-rev7-roadmap.md`](../../archive/2026-10-10-store-and-asset-search-rev7-roadmap.md)).

## Conclusions carried over

These are numbered as in the search design's revision-7 `DESIGN.md`, which is archived at
[`archive/2026-10-10-store-and-asset-search-rev7-DESIGN.md`](../../archive/2026-10-10-store-and-asset-search-rev7-DESIGN.md).

- **5. Avoid the Whoosh prototype's mechanism.** Indexing rode along inside a metadata hook, so
  correctness depended on every notification being delivered. A missed delivery is permanent and
  goes unnoticed.
- **6. Correctness comes from reconciliation; push only reduces latency.**
  - A diff of `(id, version)` streams tells an external view what it is missing, deletions
    included.
  - This one mechanism serves search engines, vector stores, RAG pipelines and SQL mirrors. They
    differ only in what they answer.
- **7. Almost no new vocabulary is needed.**
  - Existing pieces already cover most of it: `Version`, `DependencyRecord`,
    `register_version` → `expire_stale_dependents`, `Expires` and `ExpirationMonitor`.
  - The one new concept is an identity for the *projection rule*. Without it, changing a tokenizer
    or an embedding model would leave an index looking fresh when it is stale.
- **16. Four classes of document**, separated by whether a version is knowable without producing
  the document:
  - *stored*: a content hash;
  - *transient*: a version derived from its dependencies;
  - *ad-hoc query*: a version derived from the plan and the command versions;
  - *volatile*: no version at all.
- **17. The partition is also how non-keyed assets are enumerated.** The set of possible ad-hoc
  queries is infinite, so the ones to index must be *declared*.
- **18. Volatile assets need time-based refresh.** They never register a version, so the
  reconciliation diff has nothing to compare.
- **20. Use tinysearch's data structure, not tinysearch itself.**
  - A per-document word filter is a derived asset of one document, so it has no dependency fan-out.
    That makes it the natural first in-tree index (filter, then verify).
  - In-tree indexes ride the asset layer because they *are* assets.

## What has changed since the analysis

- **Records exist** (`liquers-records`), so the analysis's "Level 1" now exists in code:
  - `RecordSource::chunks()` is the partition, and `describe_chunk` gives each chunk's provenance.
  - `FieldRole` with `IndexKind`/`Analyzer` is the schema an engine maps to its own.
- **The in-tree baseline does relevance ranking itself.** `ns-search/search` with `rank=true`
  scores with BM25. An external engine is therefore needed only for scale, persistence, or
  semantic and vector queries, not just to get ranked results.
- **The search predicate is a parseable string.** An engine receives the same syntax and parses it
  with the same parser in `liquers-records`. This was path (b) of the search design's revision 7.

## Open questions (from the search design's Phase 1, renumbered)

1. How does a projection rule get an identity that takes part in the reconciliation diff? Does it
   belong on the record, the sink, or both? (was OQ 8)
2. Should the reconciliation contract ship with a test double before any real engine exists? It
   should: a trait with only one implementation is a guess. (was OQ 9)
3. Does `produce` exist in the first indexing version? (was OQ 18)
4. What does an index entry record for a volatile document? (was OQ 19)
5. How is a declared set of ad-hoc queries bounded? Are two spellings of the same computation
   deduplicated? (was OQ 20)
6. How is "why is this document not in my results?" answered for one key? (was OQ 21)
7. Is the first deliverable the per-document filter index (milestone M4, in-tree) or a first
   external sink (milestone M7)?

## Related issues

- `ASSET-EXPIRATION-EVENTS-CANNOT-BE-OBSERVED-EXCEPT-PER-ASSET` (P2). This is the missing push
  path. It reduces latency only; reconciliation is still what guarantees correctness.
- `NO-SQL-QUERY-CAPABILITY-OVER-STORED-AND-DERIVED-DATA` and `NO-RELATIONAL-DATABASE-ACCESS-LAYER`.
  An external SQL mirror is one of this design's consumers.
- `STORE-NO-CONTENT-OR-METADATA-SEARCH` (closed, not planned). A store that keeps metadata in a
  queryable backend could later evaluate a search predicate itself. That would be an engine in this
  design's sense, fed from the store rather than reconciled against it.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

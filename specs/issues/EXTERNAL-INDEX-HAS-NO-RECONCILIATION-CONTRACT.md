---
id: EXTERNAL-INDEX-HAS-NO-RECONCILIATION-CONTRACT
kind: feature
title: An external index, vector store or SQL mirror cannot be kept in sync with a record source
status: draft
priority: P2
complexity: XL
area: [records, core/assets, lib/commands]
design: external-index-reconciliation
created: 2026-10-21
github:
---

## Problem

Systems that answer questions an in-tree scan answers badly (a full-text engine such as Tantivy or
Meilisearch, a vector store, a RAG pipeline, an external SQL mirror) are materialized views of a
record source. Liquers has no contract for feeding such a view and keeping it correct: no feed of
`(id, version)` pairs, no reconciliation diff, no identity for the projection rule (tokenizer,
embedding model), no sink trait with a test double, and no indexation policy. The in-tree search
commands (`ns-search`) scan on every query and keep nothing between queries.

## Problem Example

A manifest `docs/corpus.manifest.yaml` lists 100 keyed chunks fed to a search engine. One chunk's
file changes and another chunk is removed from the manifest. Today (HEAD cd6d5da) nothing says
which chunk changed or that one disappeared: the only safe option is re-reading all 100 chunks and
rebuilding the engine, and a removed chunk stays indexed until that rebuild. Should be: a
reconciliation run reads the 100 chunk descriptors (`RecordSource::describe_chunk`, metadata
only for stored keyed chunks), fetches the one changed chunk, deletes the removed one, and reports
98 unchanged; a second run does nothing.

## Impact

Search over a large corpus, semantic search and SQL over derived data must be built outside
Liquers, without its version and dependency machinery. That machinery is exactly what knows what
to re-index when a document changes. Workaround: a full re-index on a schedule.

## Expected behaviour

The analysis exists in `specs/design/external-index-sync/`: `interoperability-layer.md` (feed,
reconciliation, staleness declared with `Expires`, the exact/inexact/unsupported report) and
`indexation-policy.md` (inclusion, content policy, four document classes, volatile refresh). It
needs its own design. The push path it relies on for promptness is designed separately in
`external-index-sync` (asset expiration events). Open questions moved from that folder:

1. How does a projection rule get an identity that takes part in the reconciliation diff? On the
   record, the sink, or both?
2. Should the reconciliation contract ship with a test double before any real engine exists?
   (Recommended: yes.)
3. Does `produce` exist in the first indexing version?
4. What does an index entry record for a volatile document?
5. How is a declared set of ad-hoc queries bounded? Are two spellings of the same computation
   deduplicated?
6. How is "why is this document not in my results?" answered for one key?
7. Is the first deliverable the per-document filter index (in-tree) or a first external sink?

## Discovery

Split off on 2026-10-21 during bulk design, when `external-index-sync` was narrowed to its only
source issue (`ASSET-EXPIRATION-EVENTS-CANNOT-BE-OBSERVED-EXCEPT-PER-ASSET`). The folder was created
on 2026-10-10 from `store-and-asset-search`. Related: `NO-SQL-QUERY-CAPABILITY-OVER-STORED-AND-DERIVED-DATA`,
`NO-RELATIONAL-DATABASE-ACCESS-LAYER`.

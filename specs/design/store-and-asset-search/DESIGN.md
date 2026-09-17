---
id: STORE-AND-ASSET-SEARCH
kind: design
title: Search over stores and assets
workflow: liquers-project
status: in_review
phase: high-level
area: [core/store, core/assets, core/commands, lib/commands, docs]
gh_pr: []
issues: [STORE-NO-CONTENT-OR-METADATA-SEARCH]
affects_docs: []
created: 2026-09-17
superseded_by:
---
# Search over stores and assets — design tracking

**Created:** 2026-09-17

## Phase Status

- [ ] Phase 1: High-Level Design
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Notes

Started 2026-09-17 from a loose brief: make the store and assets searchable, over metadata and
possibly over data, for both people and agents. The brief asked for the task to be **delimited**
first and the **options analysed** second; a second round widened it to a broad use-case survey and
nine research questions, seeking the design sweet spot — the simplest common denominator covering as
many use cases as possible. The folder therefore carries three documents beside Phase 1:

- `use-cases.md` — the survey, with every use case classified **essential**, **optional** or **not
  search**. The two essentials named by the brief are excellent agent-memory support and a simple
  user search field.
- `research-questions.md` — the nine questions answered with evidence: store/asset methods versus
  commands, what search must do, third-party engines and why the Whoosh prototype feels wrong,
  query languages and GlueSQL, command discovery, agent and MCP needs, search syntax standards,
  embeddings in metadata, and semantic search.
- `options-analysis.md` — ground truth at HEAD, the unifying model, nine decision axes, the
  recommended combination, the invariants, and the questions Phase 2 must answer. It names no types
  or signatures; that is Phase 2's job.
- `interoperability-layer.md` — the layer for plugging in an external search engine, vector store,
  RAG pipeline or SQL mirror without tying the design to any of them. Added in the third round,
  when the brief asked whether one hook system could serve them all and handle updates and
  expiration.
- `record-model.md` — what a record, a record stream, a chunk, a batch and a schema are. Added in
  the fourth round, when partial refresh turned out to need a unit smaller than the stream, and
  extended in the fifth when memory turned out to need a second, smaller one.
- `roadmap.md` — which decisions are foundational and which are additive, the milestones, and what
  the agent memory MVP actually needs. Added in the sixth round against a fair objection: making
  Level 1 "specialized commands" appears to put the whole specification's burden on Level 0.

### The sweet spot

Every essential use case is one operation: *select records from a set, by a predicate over their
fields and their text, and return enough of each record to judge it and to address it.* They differ
only in where records come from and which clause is used. Fix those two contracts and everything
else is an extension point — commands become a record source rather than a search feature, SQL
becomes an engine over the same records, RAG becomes a record source plus a clause, and a
third-party engine becomes an implementation of selection.

The split that makes it small: **projection is a command** (it varies with the value type, an open
set) and **selection is a trait method** (it varies with the backend, and the store is the only
component that sees every write, so it is the only one that can hold an index).

### Load-bearing conclusions

1. **A search must never evaluate.** The asset layer can produce a value for a key only a recipe
   declares; a content search reaching through to that could recompute an entire corpus from one
   query.
2. **The store is not the whole corpus.** `AssetManager::get_asset_info` already resolves a key as
   live asset → store → recipe provider. A search seeing only the store answers a question nobody
   asked.
3. **Selection belongs below the consumer.** `STORE-NO-CONTENT-OR-METADATA-SEARCH` is on file
   precisely because filtering in the caller is O(corpus) and forecloses any backend that could do
   better. A command-only implementation would ship faster and have to be undone.
4. **wasm decides the engine question before anything else does.** `liquers-web` is wasm32-only and
   browser search is essential, while the mature Rust full-text engines are server-oriented —
   Tantivy's own wasm RFC says so. The baseline must run everywhere; an engine can only ever be an
   optional override.
5. **The Whoosh prototype's mechanism is the thing to avoid, not the goal.** Indexing rode along
   inside a metadata-transformation hook, with a write side and no read side. The load-bearing
   defect generalizes: *a push hook makes correctness depend on delivery*, and every missed delivery
   is permanent and undetectable — which is why `reindex_store()` had to exist and why it can only
   answer "rebuild everything?" rather than "is it right?".
6. **Correctness comes from reconciliation; push is only a latency optimization.** An external
   system is a materialized view, and a set-diff of `(id, version)` streams tells it what it is
   missing — including deletions, the half push hooks usually get wrong. This is what makes one
   layer serve a search engine, a vector store, a RAG pipeline and an external SQL database alike:
   they differ only in what they *answer*.
7. **The layer needs almost no new vocabulary.** `Version` is already a content hash on every stored
   record; `DependencyRecord { key, version }` is already the shape of "what I observed";
   `register_version` → `expire_stale_dependents` is already the push cascade; `Expires` already
   declares how stale a view may be; `ExpirationMonitor` is already a worker of that shape. Exactly
   one new concept is required: an identity for the *projection rule*, so that changing a tokenizer
   or an embedding model invalidates an index rather than leaving it looking fresh.
8. **Three scales, deliberately distinct: a chunk is the unit of refresh, a batch the unit of
   memory, a record the unit of retrieval.** A record is derived and has no independent existence,
   so versioning one costs a full read of its source; a chunk is the smallest unit whose staleness is
   decidable from metadata alone. But the unit that is natural for dependencies — one parquet file —
   may be far too large to hold, so a chunk is *delivered* as batches. Partitioning by dependency and
   partitioning by size are different questions; one mechanism answers both, and batches never appear
   in a reconciliation diff.
9. **Identity is a pair, and the expensive half is derived.** A record is identified by its asset and
   an asset-dependent record id — a row number, a line, a pointer, or nothing when the asset *is* the
   record. Holding an evaluable query per row costs a multiple of the data it describes, so the asset
   is carried by the chunk and the locator query is constructed on demand, through `ActionRequest`
   rather than string templating. `Value::Object` is the right representation for the fields and the
   wrong one for the whole record: it cannot guarantee identity, cannot distinguish text from
   exact-match fields, and mixes provenance into what SQL would project.
10. **The record model has four consumers, of which search is one** — search, external sinks, SQL and
   serialization, since a chunk is a table and a stream is a table in parts. The search MVP needs
   only Level 0 (one record per asset, no ids, no batching), which must be the *degenerate case* of
   Level 1 rather than a second type. Whether Level 1 graduates to its own design is a decision to
   take deliberately.
11. **Key-prefix filtering carries more weight than metadata filtering.** Measured on this
   repository's 321 tracked documents, "what does this project know about expiration" gives 72 text
   hits; narrowing to `specs/issues/` — a key prefix, free, no metadata — gives 42; narrowing to
   still-open gives 23. Genre is mostly the folder and `area` overlaps what the text already found,
   so **lifecycle state is the only filter that genuinely needs metadata**, which narrows the
   `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` dependency to one filter rather than three. Text search
   is the primary act; filtering is a precision aid over its result.
12. **`status` means two unrelated things** — the asset lifecycle in `MetadataRecord`, the
   document's lifecycle in `specs/` front-matter — and a bare `status:draft` would resolve silently
   to either. Field resolution needs a namespace or a documented precedence.
13. **The level is cardinality; field provenance is a separate axis.** An earlier draft defined
   Level 0 as "one record per asset, fields from metadata", fusing two independent things. Extracting
   front-matter from Markdown yields one record per asset — Level 0 by cardinality — while taking its
   fields from content. The non-evaluation invariant then decides the shape: running a projection per
   candidate during a search would let one query recompute a corpus, so **a search reads projections
   that already exist and never creates one**. Materializing them is a recipe's job. This repository
   already works that way by hand — `specs/index.csv` is a committed, regenerated projection of every
   document's front-matter — so the practice preceded the principle.
14. **Only seven decisions are foundational.** The test is whether a thing can be added later
   without changing what Level 0 shipped. Seven cannot — the record's shape with the id field present
   but unused, identity as a pair, fields as a named `Value::Object`, a bounded opaque result rather
   than a `Vec`, the ordering promise, the text/field distinction, and non-exhaustive enums. Schema,
   chunks, batches, locator rules, streaming, projection identity and reconciliation are all
   additive. Milestones M0–M3 are the deliverable and are exactly what the agent memory MVP needs.
15. **Borrow tinysearch's data structure, not tinysearch.** It builds its index at build time and
   emits a compiled wasm module, which does not fit a corpus mutated at runtime. But a per-document
   word filter is a derived asset of *one* document, so it has no dependency fan-out — which is the
   problem that makes a monolithic derived index unattractive. In-tree indexes ride the asset layer
   because they *are* assets; external systems need reconciliation precisely because they cannot be.

## Relationship to `agent-memory-mvp`

That design's Phase 1 open question 3 asks how far its MVP goes on search, noting a subtree scan is
"honest at ~300 documents and wrong at 100×". This design answers it: search is a capability with
a query surface, not a command private to `ns-mem`. The two designs share the non-evaluation
invariant and the "no identity, no ACL" exclusion (`CORE-SESSION-AND-KEY-ACL`).

## Scope boundaries

**Optional** — reachable later, not built now: ranked full text, semantic search and RAG, a
maintained index, any actual external sink, tags, facets and ranges, router fan-out, a dedicated
HTTP endpoint. `options-analysis.md` §5 records why each is excluded and what keeps it cheap.

**A separate task** — SQL. Considered here only where it intersects: an external SQL database is fed
and kept fresh by the same interoperability layer as any other external system
(`interoperability-layer.md` §6). The single requirement it places on this design is that records
carry named, typed fields, so that a SQL column and a search predicate's field are the same thing.

**Excluded outright** — identity and access control, consistent with `CORE-SESSION-AND-KEY-ACL`.

**Not search** — link traversal, dependency-graph queries and DataFrame filtering. Each has, or
deserves, its own mechanism.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Use cases](./use-cases.md)
- [Research questions](./research-questions.md)
- [Options analysis](./options-analysis.md)
- [Record model](./record-model.md)
- [Interoperability layer](./interoperability-layer.md)
- [Roadmap](./roadmap.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

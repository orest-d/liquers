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
- `options-analysis.md` — ground truth at HEAD, the unifying model, eight decision axes, the
  recommended combination, the invariants, and the questions Phase 2 must answer. It names no types
  or signatures; that is Phase 2's job.

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
   inside a metadata-transformation hook, with a write side and no read side, freshness maintained
   imperatively in a framework whose thesis is that derived things recompute themselves. Keep the
   pluggable engine; move the ownership into a store decorator that can enforce the invariant.

## Relationship to `agent-memory-mvp`

That design's Phase 1 open question 3 asks how far its MVP goes on search, noting a subtree scan is
"honest at ~300 documents and wrong at 100×". This design answers it: search is a capability with
a query surface, not a command private to `ns-mem`. The two designs share the non-evaluation
invariant and the "no identity, no ACL" exclusion (`CORE-SESSION-AND-KEY-ACL`).

## Out of scope

Ranked full text, semantic search and RAG, SQL over records, a maintained index, a third-party
engine, tags, facets and ranges, router fan-out and a dedicated HTTP endpoint are all **optional**:
reachable later, not built now. `options-analysis.md` §5 records why each is excluded and what keeps
it cheap. Identity and access control are excluded outright, consistent with `CORE-SESSION-AND-KEY-ACL`.

Link traversal, dependency-graph queries and DataFrame filtering are **not search** and stay out
permanently; each has, or deserves, its own mechanism.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Use cases](./use-cases.md)
- [Research questions](./research-questions.md)
- [Options analysis](./options-analysis.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

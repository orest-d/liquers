---
id: STORE-AND-ASSET-SEARCH
kind: design
title: Search over stores and assets
workflow: liquers-project
status: in_review
phase: high-level
area: [core/store, core/assets, lib/commands, docs]
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
first and the **options analysed** second, so this folder carries an extra document:

- `phase1-high-level-design.md` is the delimitation — scope, what is deliberately excluded, the
  invariants, and the documentation intent.
- `options-analysis.md` is the analysis — eight independent decisions, their alternatives, what
  each costs, the recommended combination, and the questions Phase 2 must answer. It is long by
  design and deliberately names no types or signatures; that is Phase 2's job.

Three things shaped the delimitation and are load-bearing:

1. **A search must never evaluate.** The asset layer can produce a value for a key that only a
   recipe declares. If a content search reached through to that, one query could recompute an
   entire corpus. Search reads what already exists.
2. **The store is not the whole corpus.** `AssetManager::get_asset_info` already resolves a key as
   live asset → store → recipe provider. A search that saw only the store would answer a question
   nobody asked.
3. **Selection belongs below the consumer.** `STORE-NO-CONTENT-OR-METADATA-SEARCH` is on file
   precisely because filtering in the caller is O(corpus) and forecloses any backend that could do
   better. A command-only implementation would ship faster and have to be undone.

## Relationship to `agent-memory-mvp`

That design's Phase 1 open question 3 asks how far its MVP goes on search, noting a subtree scan is
"honest at ~300 documents and wrong at 100×". This design answers it: search is a capability with
a query surface, not a command private to `ns-mem`. The two designs share the non-evaluation
invariant and the "no identity, no ACL" exclusion (`CORE-SESSION-AND-KEY-ACL`).

## Out of scope

Ranked full-text and relevance scoring, semantic/embedding search, a dedicated index engine or
crate, a new HTTP endpoint, and identity- or ACL-filtered results. `options-analysis.md` §4 records
why each is excluded now and what keeps it cheap to add later.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Options analysis](./options-analysis.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

---
id: STORE-AND-ASSET-SEARCH
kind: design
title: Search over stores and assets
workflow: liquers-project
status: in_review
phase: high-level
area: [records, core/assets, lib/commands, docs]
gh_pr: []
issues: [STORE-NO-CONTENT-OR-METADATA-SEARCH, RECORD-SOURCE-WRAPPERS-UNSPECIFIED]
affects_docs: [reference/SEARCH.md, reference/RECORD_STREAMS.md]
created: 2026-09-17
superseded_by:
---
# Search over stores and assets — design tracking

**Created:** 2026-09-17 · **Revision 8:** 2026-10-10

## Phase Status

- [ ] Phase 1: High-Level Design — revision 8 **in review**. Revision 7 was approved on 2026-09-18
  and is superseded.
- [ ] Phase 2: Solution & Architecture — revision 8 drafted alongside Phase 1; reviewed at its own
  gate
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## What this design builds

Three commands in a new `search` namespace, over records (`liquers-records`):

| Command | What it does |
|---|---|
| `catalog` | A folder, taken from the input state, becomes records: one row per entry, with key, filename, extension, status, title, description and optionally content. Recursive or folder-only. The root key is used when there is no input |
| `commands` | The command registry, whole or one namespace, becomes records whose columns match search's default fields |
| `search` | Filters any records with a short syntax: terms, phrases, `-`, `\|`, parentheses and `field:value`. Optionally ranks them with BM25. It is case-sensitive on request, and matches the field list given at the end of its arguments, or a default list |

`search` accepts whatever can be made into records:
- a view or a source;
- a folder, through `catalog`;
- a key or query, which is evaluated;
- a stored CSV or other table format.

With `rank=false` over a source, it filters lazily and never materializes.

## Revision 8 — refocused on records, 2026-10-10

Revisions 1–7 ran from 2026-09-17 to 2026-09-25 and built an analysis. Its record half became
[`record-streams`](../record-streams/), which is now complete and built as `liquers-records`. With
records in place, the user brief of 2026-10-10 reduced search to the three commands above.
Revision 8 merges that brief with the earlier analysis.

**Kept from revisions 1–7:**
- the non-evaluation invariant (conclusions 1 and 14 below);
- the key-prefix observation (conclusion 11);
- the `status` collision (conclusion 12);
- the syntax grammar, its literal-term and empty-expression rules, and the escaping notes;
- evidence as appended columns (`score` and `excerpt`), not a wrapper type;
- the ordering promise;
- the predicate as a syntax string, and the conditions under which `Value::Predicate` would earn
  its place;
- commands as a record source.

**Dropped as obsolete:**
- `AsyncStore::select`, `AssetManager::select`, the capability flag, conformance rules, router
  fan-out and push-down. Search is a command over records, so `STORE-NO-CONTENT-OR-METADATA-SEARCH`
  is closed as superseded. A store that later keeps metadata in a queryable backend could evaluate
  a predicate itself; that is an engine in `external-index-sync`'s sense.
- `Hit`, `ClauseMatch`, `RecordSet`, `Diagnostics`, the evidence bitmask and its 64-node cap.
- The roadmap's foundational decisions F1–F7, the Level 0/Level 1 discussion, and milestones M0,
  M5 and M6. `record-streams` delivered all of these.
- "Ranked full text forces an external engine". `search` ranks in-tree.
- The `get_asset_info` repair, which is already done (`DESCRIBING-AN-ASSET-CAN-TRIGGER-ITS-EVALUATION`
  is closed).

**Moved to [`external-index-sync`](../external-index-sync/):**
- `interoperability-layer.md` and `indexation-policy.md`, whole;
- milestones M4 (per-document filters) and M7 (sinks);
- conclusions 5, 6, 7, 16, 17, 18 and 20;
- the revision-7 Phase 1 open questions 8, 9 and 18–21.

That design keeps anything held between queries: indexes, engines, vector stores, RAG and SQL
mirrors.

**Archived, so nothing is lost:** the revision-7 `DESIGN.md`, Phase 1, Phase 2 and `roadmap.md`
are at `specs/archive/2026-10-10-store-and-asset-search-rev7-*.md`.

## Conclusions that still hold

Numbers are those of revision 7.

1. **A search never evaluates.**
   - `catalog` describes entries without starting them, and reads content only from stored bytes.
   - The only evaluation is of a key or query the caller hands to `search` explicitly.
2. **The store is not the whole corpus.** `catalog` lists through the asset manager, so keys that
   only a recipe declares are found too.
4. **wasm decides the engine question.** The search kernels live in `liquers-records`, which builds
   for wasm, so the browser build can search.
11. **Key-prefix filtering carries more weight than metadata filtering.** This was measured on this
    repository: 72 text hits, 42 within `specs/issues/`, 23 still open. Hence `key` with globs, and
    a folder-scoped `catalog`.
12. **`status` means two things.** The catalog's `status` column is the asset lifecycle.
    Front-matter fields will get an `attr.` prefix.
14. **The danger is starting an asset, not projecting one.** `get_asset_info` is the safe describe.

## Relationship to other work

- **`agent-memory-mvp`.** This design answers its open question 3 (how far an MVP goes on search):
  search is a set of commands, not something private to `ns-mem`.
- **`QUERY-API-ARGUMENTS-ONLY-IN-QUERY-PATH`**, filed 2026-10-10. Over HTTP a search expression has
  to be escaped into the path. Passing arguments by name is that issue, not this design.
- **`ns-rec/file_records`** overlaps with `catalog` (Phase 1, open question 2).

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
- Background: [use cases](./use-cases.md), [research questions](./research-questions.md),
  [options analysis](./options-analysis.md)
- [`external-index-sync`](../external-index-sync/) and [`record-streams`](../record-streams/)

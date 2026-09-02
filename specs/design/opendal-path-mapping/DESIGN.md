---
id: OPENDAL-PATH-MAPPING
kind: design
title: One path mapping for the OpenDAL store, with a key round-trip property
workflow: liquers-project
status: in_review
phase: architecture
area: [store/backends]
gh_pr: []
issues: [STORE-OPENDAL-SLASH-HANDLING, OPENDAL-LOCALFS-TEST-SILENT-ON-WRONG-VALUE-TYPE, STORE-OPENDAL-WITHOUT-ASYNC-STORE-BROKEN]
created: 2026-08-29
superseded_by:
---
# OpenDAL path mapping

Design tracking for `STORE-OPENDAL-SLASH-HANDLING` (**P0** since 2026-09-02). Begun under
[`guides/autonomous_issue_fixing.md`](../../guides/autonomous_issue_fixing.md), and **migrated to
the `liquers-project` five-phase contract on 2026-09-02**, explicitly adopted by the user at the
architecture gate: the issue is `P0`/`M`, and that guide's §1 confines its procedure to `S`/`M` at
`P2`/`P3`. All five phases are therefore required, Phase 5 documentation included, with a per-phase
approval gate.

## Phase status

- [x] Phase 1: High-Level Design — [`phase1-high-level-design.md`](./phase1-high-level-design.md)
      *(rewritten 2026-09-02 after a second reproduction; restructured to the template the same day)*
- [x] Phase 2: Solution & Architecture — [`phase2-architecture.md`](./phase2-architecture.md)
      *(rewritten and restructured 2026-09-02; gate decisions folded in)*
- [ ] **Phase 2 approval gate — awaiting `proceed`.** Q1-Q4 and the workflow question are answered;
      the revised document has not yet been approved.
- [ ] Phase 3: Examples & Use-cases — `phase3-examples.md`
- [ ] Phase 4: Implementation Plan — `phase4-implementation.md`
- [ ] Phase 5: Documentation — `phase5-documentation.md` *(mandatory under `workflow: liquers-project`)*

## Why this folder exists

The issue as filed says keys containing `/` are "not reliably addressable through an OpenDAL-backed
store". A first reproduction on 2026-08-29 did not support that headline — on the filesystem
backend a nested key works end to end — and restated the problem as three defects, none of them
about slashes. A **second reproduction on 2026-09-02, probing sibling directories whose names share
a prefix, found that the headline is true after all**, for a reason the first pass could not see
with one key in isolation: three call sites address a directory without a trailing `/`, so OpenDAL
treats the path as a prefix. One of them is `removedir`, which therefore deletes sibling
directories.

Six defects are now in scope. The folder exists because the problem statement had to be corrected
twice, and both corrections are evidence a future reader needs.

## Decisions taken at the architecture gate, 2026-09-02

| | Decision |
|---|---|
| **Workflow** | Adopt `liquers-project`. Phase 1 and Phase 2 restructured to its templates the same day; Phases 3-5 follow it, Phase 5 mandatory. |
| **Q1 — directory-key gap in scope?** | Yes. |
| **Q2 — `key_prefix()` fix here or split out?** | Fix here, in its own commit, with a router test. |
| **Q3 — the 200-line commented-out synchronous `OpenDALStore`?** | **Delete it in this change**, so the issue closes with all four of its `//TODO: create_dir` citations resolved rather than two left inside dead text. |
| **Q4 — the P1 -> P0 raise?** | Keep P0. Data loss reachable over HTTP is the guide's own §4.4 criterion. |

## Notes

- The `// FIXME: … some bug with handling '/'` at `opendal_store.rs:340` is **stale as written**.
  Re-enabling the line it guards produces correct output on the filesystem backend at all directory
  depths. The second half of that comment — that the call may be too expensive — is still true and
  is the reason to leave it disabled.
- **`make_sub_dirs` has never worked.** `create_dir` without a trailing slash is rejected by
  OpenDAL unconditionally and the error is discarded. The 2026-08-29 note that it satisfies the
  `//TODO: create_dir` markers is withdrawn; Phase 1 records the evidence and Phase 2 §5 proposes
  deleting the function.
- **2026-09-02, from [`design/store-factories-in-core/`](../store-factories-in-core/):** that
  design has **merged** (`status: complete`, PR #46). `store_builder.rs` no longer exists;
  `create_opendal_store` is now `OpendalStoreFactory::create` in
  `liquers-store/src/store_factory.rs`. Every reference in Phase 2 has been re-resolved. There is
  no merge conflict: that design does not touch `opendal_store.rs`. Its `opendal03` test carries a
  comment deferring the `key_prefix()` assertion to this design.
- `AsyncMemoryStore` (`liquers-core/src/store.rs:810-830`) synthesizes directory existence from a
  key index rather than asking a backend. That is the precedent for the directory-key gap described
  in Phase 1 defect 4, and the reason the fix is not speculative.

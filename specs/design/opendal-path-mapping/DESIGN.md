---
id: OPENDAL-PATH-MAPPING
kind: design
title: One path mapping for the OpenDAL store, and shared directory support in core
workflow: liquers-project
status: in_review
phase: examples
area: [core/store, store/backends, web]
gh_pr: []
issues: [STORE-OPENDAL-SLASH-HANDLING, CORE-DIRECTORY-INDEX-NOT-SHARED, CORE-ASYNC-MEMORY-STORE-MAKEDIR-DOES-NOTHING, OPENDAL-LOCALFS-TEST-SILENT-ON-WRONG-VALUE-TYPE, STORE-OPENDAL-WITHOUT-ASYNC-STORE-BROKEN]
created: 2026-08-29
superseded_by:
---
# OpenDAL path mapping

Design tracking for `STORE-OPENDAL-SLASH-HANDLING` (**P0**) and `CORE-DIRECTORY-INDEX-NOT-SHARED`
(P1, `L`, filed at the architecture gate on 2026-09-02 and covered here). Begun under
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
- [x] Phase 2 approval gate — approved 2026-09-02.
- [x] Phase 3: Examples & Use-cases — [`phase3-examples.md`](./phase3-examples.md)
      *(two findings carried back into Phase 2; see Notes)*
- [ ] **Phase 3 approval gate — awaiting `proceed`.**
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
| **The store contract in `specs/reference/`** | Desirable, and **Phase 5 work** — written against what shipped. `specs/reference/STORE_SEMANTICS.md`. |
| **Where the directory fallback lives** | **`liquers-core`**, not private to the OpenDAL store: `liquers-web`'s HTTP-backed stores have or will have the same problem. A shared `DirectoryIndex` (the `AsyncMemoryStore` mechanism, extracted and generalized) plus the `AsyncStore` semantics that follow from `is_dir`. Filed as `CORE-DIRECTORY-INDEX-NOT-SHARED`; the work becomes cross-crate and therefore `L`. |

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
- **Four stores already derive directory structure from a flat key set, no two alike**, which is
  what turned "give the OpenDAL store a fallback" into "put the fallback in core":
  `AsyncMemoryStore` (`store.rs:580`, refcounted `scc` index maintained on write), the sync
  `MemoryStore` (`:1607`, no index — an O(n) scan per call), `FetchStore`
  (`liquers-web/src/store/fetch.rs:130`, an immutable `BTreeMap` built from a configured key set),
  and `LocalStorageStore` (`local_storage.rs:353`, a mutable map **plus** an `explicit_dirs` set for
  empty directories `makedir` created). `AsyncOpenDALStore` has none. `explicit` is a field of the
  core type because `LocalStorageStore` proved it necessary.
- **Sequencing.** The P0 (commits 1-2: the trailing slash and `key_prefix()`) touches
  `liquers-store` only and depends on nothing in `liquers-core`, so it can ship and revert ahead of
  the core work if that needs another round. Phase 4 keeps that freedom.
- **Phase 3 corrected Phase 2 twice, and both corrections are recorded in Phase 2 rather than only
  in Phase 3.** (1) "`AsyncMemoryStore`'s existing tests prove the extraction faithful" was not
  evidence: there is **one** behavioural test, covering a single key and never checking `is_dir`
  after a removal. Characterization tests are now written against `HEAD` and committed *before* the
  extraction. (2) `AsyncMemoryStore::makedir` (`store.rs:888`) is a silent no-op, so adopting
  `DirectoryIndex::explicit` would change its behaviour; filed as
  `CORE-ASYNC-MEMORY-STORE-MAKEDIR-DOES-NOTHING` (P0/S — a documented endpoint that does nothing)
  and sequenced as its own commit after the extraction.

---
id: OPENDAL-PATH-MAPPING
kind: design
title: One path mapping for the OpenDAL store, with a key round-trip property
status: in_review
phase: architecture
area: [store/backends]
gh_pr: []
issues: [STORE-OPENDAL-SLASH-HANDLING, OPENDAL-LOCALFS-TEST-SILENT-ON-WRONG-VALUE-TYPE, STORE-OPENDAL-WITHOUT-ASYNC-STORE-BROKEN]
created: 2026-08-29
superseded_by:
---
# OpenDAL path mapping

Design tracking for `STORE-OPENDAL-SLASH-HANDLING` (**P0** since 2026-09-02), begun under
[`guides/autonomous_issue_fixing.md`](../../guides/autonomous_issue_fixing.md). No `workflow:`
marker: this is a simplified transitional design whose required phases are the two written here
plus whatever the approval gate authorizes. It is **not** opted into the `liquers-project` artifact
and approval contract — see "Workflow" below, which is a question for the gate.

## Phase status

- [x] Phase 1: High-level design — [`phase1-high-level-design.md`](./phase1-high-level-design.md)
      *(rewritten 2026-09-02 after a second reproduction)*
- [x] Phase 2: Solution and architecture — [`phase2-architecture.md`](./phase2-architecture.md)
      *(rewritten 2026-09-02)*
- [ ] Approval gate — **awaiting a decision on Q2, Q3, Q4 and the workflow question**
- [ ] Phase 3: Examples, reproduction and tests
- [ ] Phase 4: Implementation plan and execution
- [ ] Phase 5: Documentation

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

## Workflow

The issue is `P0` / `M`. `guides/autonomous_issue_fixing.md` §1 confines that procedure to `S`/`M`
at `P2`/`P3`, so this design has outgrown the procedure it was begun under. Two ways forward, for
the gate to choose:

- **Adopt the `liquers-project` five-phase contract** — add `workflow: liquers-project` to this
  front-matter, and produce Phase 3, 4 and 5 documents under that skill's templates and approval
  gates. `DOCS_STRUCTURE_GUIDE.md` §5.2 requires the user to adopt that contract explicitly; it is
  not added retroactively on an agent's judgement.
- **Keep the simplified contract** and carry on with the phases this folder already uses, with the
  gate authorizing Phases 3-5 in one decision.

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

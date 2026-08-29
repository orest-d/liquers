---
id: OPENDAL-PATH-MAPPING
kind: design
title: One path mapping for the OpenDAL store, with a key round-trip property
status: in_review
phase: architecture
area: [store/backends]
gh_pr: []
issues: [STORE-OPENDAL-SLASH-HANDLING]
created: 2026-08-29
superseded_by:
---
# OpenDAL path mapping

Design tracking for `STORE-OPENDAL-SLASH-HANDLING`, prepared under
[`guides/autonomous_issue_fixing.md`](../../guides/autonomous_issue_fixing.md). No `workflow:`
marker: this is a simplified transitional design whose required phases are the two written here
plus whatever the approval gate authorizes. It is **not** opted into the `liquers-project` artifact
and approval contract.

## Phase status

- [x] Phase 1: High-level design — [`phase1-high-level-design.md`](./phase1-high-level-design.md)
- [x] Phase 2: Solution and architecture — [`phase2-architecture.md`](./phase2-architecture.md)
- [ ] Approval gate (§5 of the autonomous procedure) — **awaiting a decision**
- [ ] Phase 3: Examples, reproduction and tests
- [ ] Phase 4: Implementation plan and execution
- [ ] Phase 5: Documentation

## Why this folder exists

The issue as filed says keys containing `/` are "not reliably addressable through an OpenDAL-backed
store". Reproduction at `HEAD` (recorded in Phase 1) does not support that headline: on the
filesystem backend, `sub/deeper/foo.txt` works end to end. What reproduction *did* find is three
separate defects the headline hides, one of which (`key_prefix()` returning the wrong value) affects
routing rather than paths. Restating the problem is therefore the first deliverable, and it needs a
durable home rather than a chat message.

## Notes

- The `// FIXME: … some bug with handling '/'` at `liquers-store/src/opendal_store.rs:335` is
  **stale**. Re-enabling the line it guards produces correct output on the filesystem backend at
  all directory depths. The second half of that comment — that the call may be too expensive — is
  still true and is the reason to leave it disabled.
- `AsyncMemoryStore` (`liquers-core/src/store.rs:1619`) synthesizes directory existence from stored
  keys rather than asking a backend. That is the precedent for the directory-key gap described in
  Phase 2, and the reason the fix is not speculative.

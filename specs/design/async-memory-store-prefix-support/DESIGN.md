---
id: ASYNC-MEMORY-STORE-PREFIX-SUPPORT
kind: design
title: Memory-store support predicates respect prefixes
workflow: liquers-project
status: in_review
phase: implementation
area: [core/store]
gh_pr: []
issues: [CORE-ASYNC-MEMORY-STORE-IS-SUPPORTED-IGNORES-PREFIX]
affects_docs: [specs/reference/STORE_SEMANTICS.md, specs/issues/CORE-ASYNC-MEMORY-STORE-IS-SUPPORTED-IGNORES-PREFIX.md]
created: 2026-09-02
superseded_by:
---
# Memory-store Prefix Support Design Tracking

**Created:** 2026-09-02

## Phase Status

- [x] Phase 1: High-Level Design - approved 2026-09-02
- [x] Phase 2: Solution & Architecture - approved 2026-09-02
- [x] Phase 3: Examples & Testing - approved 2026-09-02
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Notes

- Phase 1 distinguishes the existing absolute-key invariant from prefix membership: retain
  `!key.is_relative()` and add the missing segment-wise prefix predicate.
- Phase 2 resolves the helper question in favor of two explicit predicates matching the existing
  file and OpenDAL stores; no API, data structure, error, or command change is needed.
- Phase 3 uses runnable inline unit-test templates. Six named cases share one helper so prefix
  boundaries fail independently while sync and async memory-store results remain paired.
- Phase 4 keeps the implementation in one source module: two predicate edits, public trait
  rustdoc repair, and six direct regression tests followed by current-state documentation.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

---
id: ASYNC-MEMORY-STORE-PREFIX-SUPPORT
kind: design
title: Memory-store support predicates respect prefixes
workflow: liquers-project
status: complete
area: [core/store]
gh_pr: []
issues: [CORE-ASYNC-MEMORY-STORE-IS-SUPPORTED-IGNORES-PREFIX]
affects_docs: [specs/reference/STORE_SEMANTICS.md, specs/issues/CORE-ASYNC-MEMORY-STORE-IS-SUPPORTED-IGNORES-PREFIX.md, specs/issues/STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE.md]
created: 2026-09-02
superseded_by:
---
# Memory-store Prefix Support Design Tracking

**Created:** 2026-09-02

## Phase Status

- [x] Phase 1: High-Level Design - approved 2026-09-02
- [x] Phase 2: Solution & Architecture - approved 2026-09-02
- [x] Phase 3: Examples & Testing - approved 2026-09-02
- [x] Phase 4: Implementation Plan - approved 2026-09-02
- [x] Phase 5: Documentation - approved after user correction 2026-09-02
- [x] Implementation Complete

## Notes

- User feedback clarified the cumulative contract: absolute key, configured-prefix membership,
  then optional store-specific exclusions.
- Phase 2 changes both memory-store predicates to the same segment-aware minimum used by other
  prefix-bearing stores.
- Phase 3 directly tests six boundaries, including an outside key and `data` versus `database`.
- Phase 4 updates trait documentation with the empty-prefix single-file overlay example, which
  demonstrates why `is_supported` can be narrower than its prefix.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

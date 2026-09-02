---
id: EVALUATE-PATH-CONSOLIDATION
kind: design
title: One evaluation path for every entry point
workflow: liquers-project
status: draft
phase: high-level
area: [core/assets, core/plan]
gh_pr: []
issues: [CORE-EVALUATE-PATH-CONSOLIDATION, ASSETS-FIX1]
affects_docs: []
created: 2026-09-02
superseded_by:
---
# evaluate-path-consolidation Design Tracking

**Created:** 2026-09-02

## Phase Status

- [x] Phase 1: High-Level Design (awaiting approval)
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Notes

Designs the fix for `CORE-EVALUATE-PATH-CONSOLIDATION` (P1, L): `AssetRef::evaluate_and_store` and
`AssetRef::evaluate_immediately` are two independent evaluation bodies reached through four run
harnesses and six manager entry points, diverging on delegation, payload admission, status
finalization, persistence and dependency recording. Target: one body plus policy, entry points as
thin wrappers.

Phase 1 open questions carried into Phase 2: policy axes, `Context::apply`'s dependency contract,
DM registration for ad-hoc assets, the payload boundary, harness collapse, and whether
`INLINE-PATH-LACKS-EXECUTE-ONCE` is a prerequisite.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

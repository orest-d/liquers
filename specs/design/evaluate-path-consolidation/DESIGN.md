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

Discussion resolved most of Phase 1's questions before Phase 2. The "policy axes" framing was
rejected: dependency recording, status finalization, key-owner delegation and the payload
precondition are invariants of the one body; persistence is *derived* from a reproducibility
predicate (no payload, no supplied initial state, not volatile) rather than switchable; only
queued-vs-inline and queue characteristics are genuine manager policy, the latter out of scope.
The same predicate governs map reuse, loadable persistence, and eligibility to be a dependency —
which is why `Context::apply` records no edge. The unified body therefore takes only the asset.

Three questions carried into Phase 2: store-target ownership for a query asset with a `filename`,
whether the four run harnesses collapse to two given the wasm `cfg` split, and whether
`INLINE-PATH-LACKS-EXECUTE-ONCE` is a prerequisite.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

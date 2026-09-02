---
id: STORE-CONFORMANCE-SUITE
kind: design
title: A shared behavioural conformance suite for AsyncStore
workflow: liquers-project
status: draft
phase: high-level
area: [core/store, store/backends, web, docs]
gh_pr: []
issues: [STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE, CORE-STORE-KEYS-MEANS-TWO-DIFFERENT-THINGS]
affects_docs: [STORE_SEMANTICS, STORE_CONFORMANCE_GUIDE]
created: 2026-09-02
superseded_by:
---
# A shared behavioural conformance suite for `AsyncStore`

**Created:** 2026-09-02

## Phase Status

- [x] Phase 1: High-Level Design — awaiting approval
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Notes

Fixes `STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE` (P1, L). Two deliverables in order:
complete `specs/reference/STORE_SEMANTICS.md` (three rows are still marked unsettled), then build
the parameterized suite every `AsyncStore` implementation runs — natively and under `wasm32`.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

---
id: STORE-CONFORMANCE-SUITE
kind: design
title: An implemented conformance suite, a completed contract, and a store implementation guide
workflow: liquers-project
status: draft
phase: high-level
area: [core/store, store/backends, web, docs]
gh_pr: []
issues: [STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE, CORE-STORE-KEYS-MEANS-TWO-DIFFERENT-THINGS]
affects_docs: [STORE_SEMANTICS, STORE_IMPLEMENTATION_GUIDE, LANGUAGE-INTEGRATION_GUIDE, STORE_FACTORY_GUIDE, STORE_CONFIG_FSD]
created: 2026-09-02
superseded_by:
---
# An implemented conformance suite, a completed contract, and a store implementation guide

**Created:** 2026-09-02

## Phase Status

- [x] Phase 1: High-Level Design — awaiting approval
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Notes

Fixes `STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE` (P1, L). Three deliverables:

1. Complete `specs/reference/STORE_SEMANTICS.md` — the contract. Three rows are still ⚠.
2. Implement `liquers_core::store_conformance` — the suite, runtime-agnostic so it runs natively
   and under `wasm32`, and applied to all seven in-tree implementations.
3. Write `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` — the operational counterpart, modelled on
   `LANGUAGE-INTEGRATION_GUIDE.md` but with the suite *implemented* rather than fixed as
   appendix pseudocode.

Contract, guide and suite stay synchronized through shared rule IDs, asserted by a test.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

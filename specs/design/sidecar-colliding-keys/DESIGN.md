---
id: SIDECAR-COLLIDING-KEYS
kind: design
title: Sidecar-colliding keys refused by the path builders
workflow: liquers-project
status: in_review
phase: high-level
area: [core/store, docs]
gh_pr: []
issues: [CORE-FILE-STORE-WRITES-METADATA-COLLIDING-KEYS]
affects_docs: [STORE_SEMANTICS, STORE_IMPLEMENTATION_GUIDE]
created: 2026-09-03
superseded_by:
---
# Sidecar-colliding keys refused by the path builders

**Created:** 2026-09-03

## Phase Status

- [ ] Phase 1: High-Level Design — awaiting approval
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Notes

Fixes `CORE-FILE-STORE-WRITES-METADATA-COLLIDING-KEYS` (P1, M), found by conformance rule
`sidecar03` against fixture `C2` and recorded there as an allowed failure.

`AsyncFileStore` refuses a sidecar-colliding key in `is_supported` — a routing hint — and accepts it
everywhere else, so `set("collide.__metadata__")` overwrites the metadata of `collide`. The fix
moves the refusal into the path builders, as `STORE_SEMANTICS.md` §8 already promises and
`AsyncOpenDALStore` already implements.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

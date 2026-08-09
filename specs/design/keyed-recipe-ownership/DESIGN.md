---
id: KEYED-RECIPE-OWNERSHIP
kind: design
title: Non-evaluating ownership test for keyed recipes
status: draft
phase: high-level
area: [core/assets, web]
gh_pr: []
issues: [CORE-IMMEDIATE-MANAGER-KEYED-RECURSION, VOLATILE-KEYED-RECIPE-SELF-DELEGATION]
created: 2026-08-09
superseded_by:
---
# keyed-recipe-ownership Design Tracking

**Created:** 2026-08-09

## Phase Status

- [x] Phase 1: High-Level Design (awaiting approval)
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Implementation Complete

## Notes

Fixes two P1 issues on the same line of `AssetRef::evaluate_recipe`
(`liquers-core/src/assets.rs:1833`): the wasm stack-exhaustion recursion under
`ImmediateAssetManager`, and the spurious dependency cycle for volatile keyed recipes. The
regression guard is five `test.fixme` cases in `liquers-web/tests/e2e/store.spec.ts` plus a new
wasm keyed-evaluation test.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)

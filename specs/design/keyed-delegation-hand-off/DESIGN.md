---
id: KEYED-DELEGATION-HAND-OFF
kind: design
title: Keyed delegation is a hand-off, not a dependency
workflow: liquers-project
status: complete
area: [core/assets]
gh_pr: []
issues: [ASSET-KEYED-DELEGATION-ALWAYS-CYCLES]
affects_docs: [specs/reference/DEPENDENCIES_STATUS.md]
created: 2026-08-12
superseded_by:
---
# keyed-delegation-hand-off Design Tracking

**Created:** 2026-08-12

## Phase Status

- [x] Phase 1: High-Level Design
- [x] Phase 2: Solution & Architecture
- [x] Phase 3: Examples & Testing
- [x] Phase 4: Implementation Plan
- [x] Phase 5: Documentation
- [x] Implementation Complete

## Notes

Fixes `ASSET-KEYED-DELEGATION-ALWAYS-CYCLES` (P0): the keyed-delegation branch of
`AssetRef::evaluate_recipe` could never succeed, because the delegate is always registered under
the caller's own key and `record_dependency_on_asset` therefore always saw a self-edge.

The rule added is that **two assets resolving to the same key are one node of the dependency
graph**, so waiting on one is a hand-off with no edge to record.
`AssetRef::record_dependency_on_asset` exempts that case — no metadata `DependencyRecord`, no
`DependencyManager` edge — and delegation reaches `AssetManager::wait_for_dependency` for the first
time. `would_create_cycle` is unchanged; it was answering correctly and simply should not have been
asked.

## Implementation outcome

Landed as planned in Phase 4, all six code/doc steps, with no deviation from the approved
architecture. The verification tests
(`manager_parametric.rs::keyed_delegation_{default,immediate}`) were inverted per the instructions
their previous author left in them; two unit tests were added in `liquers-core/src/assets.rs`.
`cargo test -p liquers-core --lib --tests` and `cargo test -p liquers-lib --lib --tests` are green.

One thing was found and deliberately left out of scope: a delegating asset re-persists the owner's
value to the store under the same key, because `evaluate_and_store` cannot tell a delegated state
from one it computed. Idempotent but wasteful, and unreachable before this fix. Filed as
`DELEGATED-VALUE-REPERSISTED` (P3).

See `phase5-documentation.md` for the full outcome, learning points and documentation changes.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

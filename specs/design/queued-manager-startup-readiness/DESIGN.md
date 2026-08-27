---
id: QUEUED-MANAGER-STARTUP-READINESS
kind: design
title: Environment builder
workflow: liquers-project
status: draft
phase: high-level
area: [core/assets, core/context]
gh_pr: []
issues: [QUEUED-MANAGER-STARTUP-READINESS, ENVIRONMENT-MANAGER-REFERENCE-CYCLE]
affects_docs: []
created: 2026-08-27
superseded_by:
---
# queued-manager-startup-readiness Design Tracking

**Created:** 2026-08-27

## Phase Status

- [x] Phase 1: High-Level Design (awaiting approval)
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Notes

Resolves issue `QUEUED-MANAGER-STARTUP-READINESS` (P1; complexity to be reclassified M -> L).

Race confirmed empirically: immediately after `to_ref()` the dependency manager holds no command
versions, and `register_plan_dependencies` therefore silently registers zero edges for a plan
evaluated in that window.

Root cause is the Environment/AssetManager construction cycle, broken today by back-filling
`set_envref` after `EnvRef` is already shareable. Scope was widened at the user's direction from a
readiness barrier to an environment builder that owns that cycle.

Phase 1 decision (user, option A): `build()` and asset-manager startup are **sync**. The async in
today's `start()` comes only from `scc`'s `entry_async`; startup does uncontended vacant inserts into
an in-memory map and never touches the store.

Cleanup to fold into Phase 2 rather than file separately: `DefaultAssetManager::with_capacity` has an
unconditional `eprintln!("Spawned job queue")` that fires on every manager construction
(`liquers-core/src/assets.rs`). Stray debug output in code this design rewrites.

Also filed during Phase 1: `ENVIRONMENT-MANAGER-REFERENCE-CYCLE` (P2) — the manager's back-reference
is a strong `Arc`, so every environment leaks (`Arc::strong_count(&envref.0) == 2` after `to_ref`).
Reassessed: there are **two** cycles. `AssetData<E>` also holds a strong `EnvRef<E>`, and the
manager's `assets` / `query_assets` maps hold those assets, so every cached asset closes a second
cycle. Weakening only the manager's back-reference does not fix the leak; recommendation is to keep
that issue outside this project's committed scope.

Phase 1 decisions (user): `EnvRef::new` is deprecated (one in-tree caller) and `to_ref` is withdrawn
from the public surface — literal privacy is unavailable for a defaulted method on a public trait, so
Phase 2 picks between removing it from the trait and deprecating it in place. 336 `.to_ref()` call
sites migrate. Environment consolidation is a Phase 2 research task, constrained by two firm
requirements: the caller specifies the `Value` type, and a caller can implement their own
`Environment` for custom global services — so the builder must be generic over `E: Environment`.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

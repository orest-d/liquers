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

Also filed during Phase 1: `ENVIRONMENT-MANAGER-REFERENCE-CYCLE` (P2) — the manager's back-reference
is a strong `Arc`, so every environment leaks (`Arc::strong_count(&envref.0) == 2` after `to_ref`).

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

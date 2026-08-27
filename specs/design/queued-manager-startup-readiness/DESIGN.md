---
id: QUEUED-MANAGER-STARTUP-READINESS
kind: design
title: Asset manager startup readiness
workflow: liquers-project
status: draft
phase: high-level
area: [core/assets, core/context]
gh_pr: []
issues: [QUEUED-MANAGER-STARTUP-READINESS]
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

Resolves issue `QUEUED-MANAGER-STARTUP-READINESS` (P1, complexity M).

Race confirmed empirically: immediately after `to_ref()` the dependency manager holds no command
versions, and `register_plan_dependencies` therefore silently registers zero edges for a plan
evaluated in that window.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

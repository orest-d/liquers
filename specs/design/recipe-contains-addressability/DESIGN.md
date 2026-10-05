---
id: RECIPE-CONTAINS-ADDRESSABILITY
kind: design
title: Separate listed (contains) and producible (can_make) questions on recipe providers
status: in_review
phase: implementation
readiness: ready
area: [core/assets]
issues: [RECIPE-CONTAINS-DEFAULT-ASSUMES-ENUMERABILITY]
created: 2026-10-04
---
# recipe-contains-addressability Design Tracking

Autonomous bulk design (`guides/autonomous_bulk_design.md`) for
`RECIPE-CONTAINS-DEFAULT-ASSUMES-ENUMERABILITY`. The issue was previously linked to the
`record-streams` project, which resolved its concrete case (`ManifestRecipeProvider` overrides
`contains`) but explicitly left the trait-level change to the issue; this design owns that change.

## Phase Status

- [x] Phase 1: High-Level Design
- [x] Phase 2: Solution & Architecture
- [x] Phase 3: Examples and Tests
- [x] Phase 4: Implementation Plan

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)

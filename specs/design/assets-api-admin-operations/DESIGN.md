---
id: ASSETS-API-ADMIN-OPERATIONS-DESIGN
kind: design
title: Assets API manager status and a guarded removal of computed values
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [axum, core/assets, docs]
issues: [ASSETS-API-ADMIN-OPERATIONS]
affects_docs: [specs/reference/WEB_API_SPECIFICATION.md, specs/guides/WEB_API_GUIDE.md, specs/reference/ASSETS.md]
created: 2026-10-20
---
# Assets API manager status and a guarded removal of computed values

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented. Source deferred by `axum-assets-endpoints` (implemented, PR #73). Re-sized from `M` to
`L`: the guarded removal needs a new `AssetManager` method in `liquers-core` (Phase 2).

## Phase Status

- [x] Phase 1: High-Level Design
- [x] Phase 2: Solution & Architecture
- [x] Phase 3: Examples & Tests
- [x] Phase 4: Implementation Plan
- Phase 5: none — four-phase bulk design

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)

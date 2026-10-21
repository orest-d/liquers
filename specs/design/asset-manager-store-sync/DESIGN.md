---
id: ASSET-MANAGER-STORE-SYNC
kind: design
title: One call reconciles a running asset manager with its store
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/assets, core/store, records]
issues: [ASSET-MANAGER-CANNOT-BE-SYNCHRONIZED-WITH-THE-STORE]
affects_docs: [guides/DEPENDENCY_CONSISTENCY_GUIDE.md, reference/ASSETS.md, reference/DEPENDENCIES_STATUS.md]
created: 2026-10-20
---
# One call reconciles a running asset manager with its store

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-21: Phases 1-4 written, not
approved, nothing implemented. The operation composes checks that already exist, adds one new
step (reconciling live registrations) and adds a cache-invalidation hook on recipe providers.

## Phase Status

- [x] Phase 1: High-Level Design (written, not approved)
- [x] Phase 2: Architecture (written, not approved)
- [x] Phase 3: Examples and Tests (written, not approved)
- [x] Phase 4: Implementation Plan (written, not approved)

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)

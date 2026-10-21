---
id: ASSET-GARBAGE-COLLECTION
kind: design
title: Bounded asset cache with reachability-based collection
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/assets, core/store]
issues: [CORE-ASSET-GC]
affects_docs: [reference/ASSETS.md, reference/ENVIRONMENT_CONFIG.md, guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md, guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md]
created: 2026-10-20
---
# Bounded asset cache with reachability-based collection

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20: Phases 1-4 written, not
approved, nothing implemented. The first deliverable bounds the asset manager's in-memory maps by
entry count and idle time. A byte budget and pruning of stored values are follow-up issues.

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

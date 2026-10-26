---
id: VALUE-MEMORY-SIZE-BUDGET
kind: design
title: Values estimate their memory size, and the asset cache can be bounded in bytes
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/value, lib/value, core/assets]
issues: [VALUE-HAS-NO-MEMORY-SIZE-ESTIMATE]
affects_docs: [reference/VALUE_TYPE_SYSTEM.md, reference/ASSETS.md, reference/ENVIRONMENT_CONFIG.md, guides/TYPE_SYSTEM_GUIDE.md, guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md]
created: 2026-10-26
---
# Values estimate their memory size, and the asset cache can be bounded in bytes

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented. Two parts: a provided `estimated_memory_size` on the value traits,
implemented for every core and `liquers-lib` variant (independent), and per-map byte budgets
`max_cached_key_bytes` / `max_cached_query_bytes` in the collector of `asset-garbage-collection`
(requires that design). Split off from `asset-garbage-collection` (its open question 2).

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

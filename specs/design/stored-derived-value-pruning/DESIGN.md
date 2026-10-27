---
id: STORED-DERIVED-VALUE-PRUNING
kind: design
title: Prune stored derived values that nothing has used for a long time
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/assets, core/store]
issues: [STORED-DERIVED-VALUES-ARE-NEVER-PRUNED]
affects_docs: [reference/ASSETS.md, reference/STORE_SEMANTICS.md, guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md]
created: 2026-10-26
---
# Prune stored derived values that nothing has used for a long time

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented. An explicit `prune_store` pass over a store subtree that drops the
data of computed, recipe-backed values older than a cut-off, through the guarded removal of
`assets-api-admin-operations`, keeping their metadata and version. Split off from
`asset-garbage-collection`.

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

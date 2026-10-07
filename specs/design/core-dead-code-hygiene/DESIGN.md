---
id: ACTIVE-10
kind: design
title: Account for the entities and cache modules of liquers-core
workflow: liquers-project
status: complete
readiness: ready
area: [build]
issues: [REPO-DEAD-CODE-HYGIENE]
affects_docs: [reference/PROJECT_OVERVIEW.md]
created: 2026-09-03
---

# Design Tracking

- [x] Phase 1: High-Level Design
- [x] Phase 2: Architecture
- [x] Phase 3: Examples and Tests
- [x] Phase 4: Implementation Plan
- [x] Phase 5: Documentation (executed 2026-10-06; approved 2026-10-07)

Phases 1-4 were rewritten on 2026-10-05: the first version was generic template text, which the
post-Phase-4 review found too thin to implement from.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

## Review 2026-10-06

Re-verified against HEAD after the latest merges. **Still valid; no change.** `liquers-core/src/cache.rs` is
unchanged (still has the unused `use chrono::format;`), its only consumer is still
`liquers-py/src/cache.rs`, `PROJECT_OVERVIEW.md` still lists it as "Query result caching", and
`CORE-SYNC-STORE-TRAIT-OBSOLETE` still carries the scope note. Readiness stays `ready`.

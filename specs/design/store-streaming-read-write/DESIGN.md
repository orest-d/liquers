---
id: STORE-STREAMING-READ-WRITE
kind: design
title: Streaming reads and writes on AsyncStore (the openbin replacement)
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/store, store/backends, axum]
issues: [CORE-STORE-OPENBIN-MISSING]
affects_docs: [specs/reference/STORE_SEMANTICS.md, specs/guides/STORE_IMPLEMENTATION_GUIDE.md]
created: 2026-10-20
---
# Streaming reads and writes on AsyncStore (the openbin replacement)

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented. Full form: the source was re-sized from `M` to `L` because the change adds methods to
the public `AsyncStore` trait and reaches three crates (`DOCS_STRUCTURE_GUIDE.md` §4.5).

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)

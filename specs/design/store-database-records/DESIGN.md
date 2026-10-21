---
id: STORE-DATABASE-RECORDS
kind: design
title: Metadata records in a database — a SQLite object store under central storage
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/store, store/backends, store/config]
issues: [STORE-NO-DATABASE-STORAGE-MODEL]
affects_docs: [specs/reference/STORE_CONFIG_FSD.md, specs/reference/STORE_SEMANTICS.md, specs/guides/STORE_CONFIG_GUIDE.md, specs/guides/STORE_IMPLEMENTATION_GUIDE.md]
created: 2026-10-20
---
# Metadata records in a database — a SQLite object store under central storage

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented. Re-scopes the source: its proposed `PreparedWrite` is shown unnecessary in Phase 2,
and the database storage model becomes a database-backed **store** used through
`design/central-storage-model/`.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)

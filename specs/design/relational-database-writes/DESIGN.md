---
id: RELATIONAL-DATABASE-WRITES
kind: design
title: Writing records into a relational database — a mirror sink, not write commands
status: in_review
phase: architecture
readiness: phase2-blocked
autofix: not-eligible
area: [records, lib/commands, store/backends]
issues: [NO-WRITE-PATH-FROM-RECORDS-INTO-A-RELATIONAL-DATABASE]
affects_docs: []
created: 2026-10-26
---
# Writing records into a relational database — a mirror sink, not write commands

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-2 written, stopped at
Phase 2 (`phase2-blocked`), not approved, nothing implemented. Two things decide the shape and are
not settled: whether Liquers writes to databases at all, and in which form (a scope decision against
VISION's non-goals), and how a query names a database — the blocking question this design inherits
from [`relational-database-access`](../relational-database-access/DESIGN.md) (also
`phase2-blocked`). The recommended form, a SQL mirror that implements the reconciliation contract
of [`external-index-reconciliation`](../external-index-reconciliation/DESIGN.md), is specified as
far as those answers allow.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- Phase 3 — not produced (blocked)
- Phase 4 — not produced (blocked)

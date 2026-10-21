---
id: RELATIONAL-DATABASE-ACCESS
kind: design
title: Reading relational databases as record sources
status: in_review
phase: architecture
readiness: phase2-blocked
autofix: not-eligible
area: [records, lib/value, lib/commands, store/backends]
issues: [NO-RELATIONAL-DATABASE-ACCESS-LAYER]
affects_docs: []
created: 2026-10-21
---
# Reading relational databases as record sources

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-2 written, stopped at Phase 2
(`phase2-blocked`): how a query names a database and when a result read from it is stale are
contract decisions with incompatible plausible answers (Phase 1, Design Readiness). Read-only
scope; writes are split off as `NO-WRITE-PATH-FROM-RECORDS-INTO-A-RELATIONAL-DATABASE`, exact
types as `RECORD-FIELD-TYPE-HAS-NO-DECIMAL`.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- Phase 3 — not produced (blocked)
- Phase 4 — not produced (blocked)

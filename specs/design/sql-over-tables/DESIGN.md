---
id: SQL-OVER-TABLES
kind: design
title: SQL over a table — the ns-pl/sql command on polars' SQL engine
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [lib/polars, lib/commands]
issues: [NO-SQL-QUERY-CAPABILITY-OVER-STORED-AND-DERIVED-DATA]
affects_docs: [specs/reference/POLARS_COMMAND_LIBRARY.md, specs/reference/RECORD_STREAMS.md]
created: 2026-10-21
---
# SQL over a table — the `ns-pl/sql` command on polars' SQL engine

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented. The first bounded slice of its source: one SQL query statement over one input table.
Joins across entries and a wasm engine are split off as
`SQL-COMMAND-CANNOT-JOIN-SEVERAL-TABLES` and `NO-SQL-ENGINE-IN-WASM-BUILDS`; the external SQL
mirror is `external-index-sync`'s.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)

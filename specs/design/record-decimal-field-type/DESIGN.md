---
id: RECORD-DECIMAL-FIELD-TYPE
kind: design
title: An exact Decimal record field type carried through every table format and bridge
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [records, lib/polars, web]
issues: [RECORD-FIELD-TYPE-HAS-NO-DECIMAL]
affects_docs: [specs/reference/RECORD_STREAMS.md, specs/guides/RECORD_STREAM_GUIDE.md, liquers-web/README.md]
created: 2026-10-26
---
# An exact Decimal record field type carried through every table format and bridge

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented. The scope is `Decimal`. The source issue's other types (UUID, time
of day, intervals, a zone-less timestamp) are split off as
`RECORD-FIELD-TYPE-HAS-NO-UUID-TIME-OR-INTERVAL` (Phase 1, question 1).

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)

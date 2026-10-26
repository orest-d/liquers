---
id: RECORD-UUID-TIME-DURATION-TYPES
kind: design
title: UUID, time-of-day and duration record field types; intervals and zone-less timestamps documented as not types
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [records, lib/polars, web]
issues: [RECORD-FIELD-TYPE-HAS-NO-UUID-TIME-OR-INTERVAL]
affects_docs: [specs/reference/RECORD_STREAMS.md, specs/guides/RECORD_STREAM_GUIDE.md, liquers-web/README.md]
created: 2026-10-26
---
# UUID, time-of-day and duration record field types; intervals and zone-less timestamps documented as not types

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented. Full form: the source was filed `M` and is corrected to `L` (three
new `FieldType` variants carried through every table format, the polars bridge and the browser
bridge, across three crates — the same reach as `record-decimal-field-type`, which was `L` for one
variant). Split off from that design's question 1; it follows its pattern but does not depend on
its open decisions (Phase 1, Design Dependencies).

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)

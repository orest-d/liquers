---
id: RECORD-SOURCE-ROW-STREAMING
kind: design
title: A record source's rows served as a byte stream (CSV, TSV, NDJSON)
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/value, lib/value, records, axum]
issues: [RECORD-SOURCE-ROWS-CANNOT-BE-STREAMED]
affects_docs: [specs/reference/RECORD_STREAMS.md, specs/reference/VALUE_TYPE_SYSTEM.md, specs/reference/WEB_API_SPECIFICATION.md, specs/guides/RECORD_STREAM_GUIDE.md, specs/guides/SERIALIZATION_GUIDE.md]
created: 2026-10-26
---
# A record source's rows served as a byte stream (CSV, TSV, NDJSON)

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented. Full form (`L`). Follow-up of
[`value-serialization-streaming`](../value-serialization-streaming/DESIGN.md) (Phases 1-4 written,
not implemented), whose `SerializedBytes`, `AssetRef::get_serialized` and axum serving functions it
requires; it realises `record-streams` Phase 2 §"Streaming a record source over HTTP".

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)

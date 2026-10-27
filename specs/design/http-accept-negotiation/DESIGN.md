---
id: HTTP-ACCEPT-NEGOTIATION
kind: design
title: Query routes answer in a format the Accept header names, converting by an automatic edge when needed
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [axum, core/value]
issues: [HTTP-ACCEPT-HEADER-NOT-NEGOTIATED]
affects_docs: [specs/reference/WEB_API_SPECIFICATION.md, specs/guides/WEB_API_GUIDE.md]
created: 2026-10-26
---
# Query routes answer in a format the Accept header names, converting by an automatic edge when needed

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented. Full form (`L` source). Split off from `automatic-value-conversion`
(its question 2). The serialization half needs nothing new; the conversion half requires
`value-conversion-registry`, which is designed but not implemented, and is planned as separate
later steps.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)

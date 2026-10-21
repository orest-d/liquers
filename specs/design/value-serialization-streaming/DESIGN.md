---
id: VALUE-SERIALIZATION-STREAMING
kind: design
title: A value's byte form as a pull stream, served over HTTP as it is produced
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/value, core/assets, axum, lib/value]
issues: [VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE]
affects_docs: [specs/reference/VALUE_TYPE_SYSTEM.md, specs/reference/ASSETS.md, specs/reference/WEB_API_SPECIFICATION.md, specs/guides/SERIALIZATION_GUIDE.md, specs/guides/TYPE_SYSTEM_GUIDE.md]
created: 2026-10-20
---
# A value's byte form as a pull stream, served over HTTP as it is produced

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented. Full form (`L`). This design is the **first deliverable slice** of the feature — the
core primitive, the asset read and HTTP serving; lazy values that need the environment and
streamed persistence are filed as follow-ups.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)

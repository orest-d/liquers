---
id: EXTENDED-SCALAR-VALUE-TYPES
kind: design
title: Fifteen extended scalar value types declared in ExtValue behind one feature
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [lib/value, core/value]
issues: [VALUE-EXTENDED-SCALAR-TYPES-MISSING]
affects_docs: [specs/reference/VALUE_TYPE_SYSTEM.md, specs/guides/TYPE_SYSTEM_GUIDE.md, specs/guides/SERIALIZATION_GUIDE.md]
created: 2026-10-26
---
# Fifteen extended scalar value types declared in ExtValue behind one feature

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented. This follows on from `value-type-definition-macro`, which was split
off from `VALUE-TYPE-DEFINITION-MACRO`. It requires that design and `value-conversion-registry`,
and neither is implemented yet. Phases 3-4 are written against the declaration syntax and the
registry interface as those designs specify them.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)

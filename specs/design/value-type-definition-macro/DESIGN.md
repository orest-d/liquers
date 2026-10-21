---
id: VALUE-TYPE-DEFINITION-MACRO-DESIGN
kind: design
title: define_value_types! generates ExtValue and its impls from one declaration per variant
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [macro, lib/value]
issues: [VALUE-TYPE-DEFINITION-MACRO]
affects_docs: [specs/reference/DEFINE_VALUE_TYPES.md, specs/reference/VALUE_TYPE_SYSTEM.md, specs/guides/TYPE_SYSTEM_GUIDE.md]
created: 2026-10-20
---
# define_value_types! generates ExtValue and its impls from one declaration per variant

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented. Requires `value-interface-capability-split`. This is the first slice of
`VALUE-TYPE-DEFINITION-MACRO` (the generator); the extended scalars are proposed as the follow-up
`VALUE-EXTENDED-SCALAR-TYPES-MISSING` (Phase 1, question 1).

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)

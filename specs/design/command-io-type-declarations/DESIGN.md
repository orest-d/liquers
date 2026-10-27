---
id: COMMAND-IO-TYPE-DECLARATIONS
kind: design
title: Commands declare the value types they accept and produce
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/commands, core/value, core/plan, macro]
issues: [COMMAND-INPUT-OUTPUT-TYPES-ARE-UNDECLARED]
affects_docs: [specs/reference/REGISTER_COMMAND_FSD.md, specs/reference/COMMAND_DECLARATION.md, specs/reference/VALUE_TYPE_SYSTEM.md, specs/guides/COMMAND_REGISTRATION_GUIDE.md]
created: 2026-10-26
---
# Commands declare the value types they accept and produce

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented.

First of three designs from one cluster. It defines the declaration contract (`TypeSet`,
`ArgumentInfo::accepts`, `CommandMetadata::produces`) and a plan-time check. The two consumers are
separate designs that require it: `automatic-value-conversion` (`VALUE-NOT-CONVERTED-AUTOMATICALLY`)
and `command-specialization-by-input-type` (`COMMAND-SPECIALIZATION-BY-INPUT-TYPE-MISSING`). They
were not merged because the three `L` scopes together would be `XL` (overlap exclusion E2).

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)

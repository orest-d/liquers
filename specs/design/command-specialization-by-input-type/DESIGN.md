---
id: COMMAND-SPECIALIZATION-BY-INPUT-TYPE
kind: design
title: A generic command dispatches to the variant that accepts its input's type
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/commands, core/plan, lib/commands]
issues: [COMMAND-SPECIALIZATION-BY-INPUT-TYPE-MISSING]
affects_docs: [specs/reference/COMMAND_ALIASES.md, specs/reference/RECIPES_AND_PLANS.md, specs/reference/COMMAND_DECLARATION.md, specs/guides/COMMAND_REGISTRATION_GUIDE.md]
created: 2026-10-26
---
# A generic command dispatches to the variant that accepts its input's type

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented.

Requires `command-io-type-declarations` (declared `accepts` / `produces` and the planner's tracking
of the flowing type). Independent of `automatic-value-conversion`, which, when both exist, applies
to the variant this design selects.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)

## Discussion 2026-11-26: type dispatch as a general mechanism

[type-dispatch-discussion.md](./type-dispatch-discussion.md) records a discussion from the UI
redesign: the generic command of this design as the one mechanism for viewers and editors of
application-defined values, and possibly serializers, instead of a branch element in pipelines.
It lists four changes this design would need (open registration of variants, lookup without
running, lazy arms, a default variant with conversion). Nothing is decided; the phases are
unchanged. The issue was raised to P1 by Orest on that date.

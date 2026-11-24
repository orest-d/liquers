---
id: COMPOSITE-VARIADIC-ARGUMENTS
kind: design
title: Complex values as variadic arguments (tuples, dictionaries, JSON values)
status: in_review
phase: implementation
readiness: ready
autofix: not-eligible
area: [core/commands, macro, core/plan, lib/ui, lib/egui]
issues: [COMMAND-COMPOSITE-VARIADIC-ARGUMENTS]
affects_docs: [specs/reference/REGISTER_COMMAND_FSD.md, specs/guides/COMMAND_REGISTRATION_GUIDE.md, specs/reference/COMMAND_DECLARATION.md, specs/reference/RECIPES_AND_PLANS.md]
created: 2026-10-20
---
# Complex values as variadic arguments

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20: Phases 1-4 written and
reviewed without phase approval. Nothing is implemented. Origin:
`design/variadic-arguments-declaration/` (complete), whose Phase 1 decision 5 recorded this
direction.

**Rewritten 2026-11-24** at Orest's request, after the UI redesign produced a first user
(per-child layout data as key and value pairs, `design/ui-redesign/master-design.md` §10.9).
Phases 1 and 2 now cover tuples, dictionaries, a JSON value, a trait for a command author's own
types, registration macro support and GUI support. Revised on 2026-11-25 after Orest's review:
the shape is a field of a variadic argument and not an argument type, there is no separate scalar
type, and a JSON value shape was added. Phases 3 and 4 were rewritten on 2026-11-25.

Pre-approved after Phase 2 on 2026-11-25 (`proceed all`).

## Phases

- [Phase 1](./phase1-high-level-design.md) — rewritten 2026-11-24, revised 2026-11-25
- [Phase 2](./phase2-architecture.md) — rewritten 2026-11-24, revised 2026-11-25
- [Phase 3](./phase3-examples.md) — rewritten 2026-11-25
- [Phase 4](./phase4-implementation.md) — rewritten 2026-11-25

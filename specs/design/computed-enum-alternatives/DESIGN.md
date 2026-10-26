---
id: COMPUTED-ENUM-ALTERNATIVES
kind: design
title: Enum alternatives computed at run time by a registered provider
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/commands, core/plan]
issues: [COMMAND-ENUM-ALTERNATIVES-CANNOT-BE-COMPUTED]
affects_docs: [specs/reference/REGISTER_COMMAND_FSD.md, specs/guides/COMMAND_REGISTRATION_GUIDE.md, specs/reference/COMMAND_DECLARATION.md]
created: 2026-10-26
---
# Enum alternatives computed at run time by a registered provider

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented.

A global enum may be **computed**: its alternatives come from a provider registered with the
command registry, either from the environment (checked when the query is planned) or from the
action's input state (checked just before the command runs). Computed alternatives validate and
describe a value; they never rewrite it, so cached results and metadata versions are unaffected by
what a provider returns.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)

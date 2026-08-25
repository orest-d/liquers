---
id: VARIADIC-ARGUMENTS-DECLARATION
kind: design
title: Declarable variadic command arguments
workflow: liquers-project
status: draft
phase: architecture
area: [macro, core/commands, lib/polars]
gh_pr: []
issues: [COMMAND-VARIADIC-ARGUMENTS-NOT-DECLARABLE, VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS, UI-VARIADIC-ARGUMENT-LIST-EDITOR, COMMAND-COMPOSITE-VARIADIC-ARGUMENTS, PY-MODULES-NOT-DECLARED-IN-LIB]
affects_docs: [specs/reference/REGISTER_COMMAND_FSD.md, specs/reference/POLARS_COMMAND_LIBRARY.md, specs/guides/COMMAND_REGISTRATION_GUIDE.md, CLAUDE.md]
created: 2026-08-25
superseded_by:
---
# variadic-arguments-declaration Design Tracking

**Created:** 2026-08-25

## Phase Status

- [x] Phase 1: High-Level Design (approved)
- [x] Phase 2: Solution & Architecture (awaiting approval)
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Notes

Closes `COMMAND-VARIADIC-ARGUMENTS-NOT-DECLARABLE`, split out of
`design/excess-action-parameters-error/` at its Phase 2. `VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS`
is listed because it becomes reachable the moment this lands; Phase 1 decision 1 closes it for
macro-registered commands via a compile-time guard, and narrows rather than closes the issue itself
(hand-built metadata is still unguarded — `liquers-py`'s compiled `add_python_command` is the live
example).

Phase 2 filed three issues: `PY-MODULES-NOT-DECLARED-IN-LIB` (found by the known-issue preflight),
`UI-VARIADIC-ARGUMENT-LIST-EDITOR` and `COMMAND-COMPOSITE-VARIADIC-ARGUMENTS` (Phase 1 decision 5).

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

---
id: VARIADIC-ARGUMENTS-DECLARATION
kind: design
title: Declarable variadic command arguments
workflow: liquers-project
status: complete
area: [macro, core/commands, lib/polars]
gh_pr: []
issues: [COMMAND-VARIADIC-ARGUMENTS-NOT-DECLARABLE, VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS, UI-VARIADIC-ARGUMENT-LIST-EDITOR, COMMAND-COMPOSITE-VARIADIC-ARGUMENTS, PY-MODULES-NOT-DECLARED-IN-LIB, POLARS-COMMAND-TESTS-BYPASS-COMMANDS, REGISTRY-IMPL-VERSION-DRIFT-UNDETECTED]
affects_docs: [specs/reference/REGISTER_COMMAND_FSD.md, specs/reference/POLARS_COMMAND_LIBRARY.md, specs/guides/COMMAND_REGISTRATION_GUIDE.md, CLAUDE.md]
created: 2026-08-25
superseded_by:
---
# variadic-arguments-declaration Design Tracking

**Created:** 2026-08-25

## Phase Status

- [x] Phase 1: High-Level Design (approved)
- [x] Phase 2: Solution & Architecture (approved)
- [x] Phase 3: Examples & Testing (approved)
- [x] Phase 4: Implementation Plan (approved)
- [x] Phase 5: Documentation
- [x] Implementation Complete

## Notes

Closes `COMMAND-VARIADIC-ARGUMENTS-NOT-DECLARABLE`, split out of
`design/excess-action-parameters-error/` at its Phase 2. `VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS`
is listed because it becomes reachable the moment this lands; Phase 1 decision 1 closes it for
macro-registered commands via a compile-time guard, and narrows rather than closes the issue itself
(hand-built metadata is still unguarded — `liquers-py`'s compiled `add_python_command` is the live
example).

Phase 2 filed three issues: `PY-MODULES-NOT-DECLARED-IN-LIB` (found by the known-issue preflight),
`UI-VARIADIC-ARGUMENT-LIST-EDITOR` and `COMMAND-COMPOSITE-VARIADIC-ARGUMENTS` (Phase 1 decision 5).
Phase 3 filed `POLARS-COMMAND-TESTS-BYPASS-COMMANDS`: none of the 13 tests in
`liquers-lib/tests/polars_commands.rs` invokes a polars command, so the two this design converts
would keep passing however the conversion went.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

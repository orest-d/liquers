---
id: EXCESS-ACTION-PARAMETERS-ERROR
kind: design
title: Excess action parameters raise an error during plan building
workflow: liquers-project
status: complete
area: [core/plan, core/error]
gh_pr: []
issues: [PLAN-EXCESS-ACTION-PARAMETERS-DROPPED, COMMAND-VARIADIC-ARGUMENTS-NOT-DECLARABLE, VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS, UI-QUERY-CONSOLE-NO-ERROR-HIGHLIGHT, COMMAND-REGISTRY-ISSUE-NAMESPACE-NAME-SWAPPED, POLARS-DOC-EXAMPLES-OMIT-NAMESPACE]
affects_docs: [specs/reference/PROJECT_OVERVIEW.md, specs/reference/POLARS_COMMAND_LIBRARY.md, specs/guides/COMMAND_REGISTRATION_GUIDE.md]
created: 2026-08-12
superseded_by:
---
# excess-action-parameters-error Design Tracking

**Created:** 2026-08-12

## Phase Status

- [x] Phase 1: High-Level Design (approved)
- [x] Phase 2: Solution & Architecture (approved)
- [x] Phase 3: Examples & Testing (approved)
- [x] Phase 4: Implementation Plan (approved)
- [x] Phase 5: Documentation
- [x] Implementation Complete

## Notes

Resolves `PLAN-EXCESS-ACTION-PARAMETERS-DROPPED`, with one deliberate departure from it: the
resolution is an **error**, not the warning the issue proposed. `Step::Warning` carries no
`Position`, so a warning cannot name the offending parameter — see phase 5, learning point 1.

Phase 1 decision 4 (making the polars selection commands variadic) was deferred at Phase 2 to
`COMMAND-VARIADIC-ARGUMENTS-NOT-DECLARABLE`, which is why `macro` and `lib/commands` are not in
this design's `area`. Until it lands, `select_columns-a-b` is an error and the working spelling is
`select_columns-a~_b`.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

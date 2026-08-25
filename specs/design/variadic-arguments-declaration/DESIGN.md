---
id: VARIADIC-ARGUMENTS-DECLARATION
kind: design
title: Declarable variadic command arguments
workflow: liquers-project
status: draft
phase: high-level
area: [macro, core/commands, lib/polars]
gh_pr: []
issues: [COMMAND-VARIADIC-ARGUMENTS-NOT-DECLARABLE, VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS]
affects_docs: [specs/reference/REGISTER_COMMAND_FSD.md, specs/reference/POLARS_COMMAND_LIBRARY.md, specs/guides/COMMAND_REGISTRATION_GUIDE.md]
created: 2026-08-25
superseded_by:
---
# variadic-arguments-declaration Design Tracking

**Created:** 2026-08-25

## Phase Status

- [x] Phase 1: High-Level Design (awaiting approval)
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Notes

Closes `COMMAND-VARIADIC-ARGUMENTS-NOT-DECLARABLE`, split out of
`design/excess-action-parameters-error/` at its Phase 2. `VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS`
is listed because it becomes reachable the moment this lands; whether it is fixed here is Phase 1
open question 1.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

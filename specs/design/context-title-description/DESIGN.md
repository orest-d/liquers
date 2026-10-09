---
id: CONTEXT-TITLE-DESCRIPTION
kind: design
title: Context methods for a command to set its asset's title and description
workflow: liquers-project
status: in_review
phase: implementation
readiness: ready
autofix: not-eligible
area: [core/context]
issues: [CONTEXT-CANNOT-SET-TITLE-OR-DESCRIPTION]
affects_docs: [reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md, guides/COMMAND_REGISTRATION_GUIDE.md, reference/ASSETS.md]
created: 2026-10-04
---
# context-title-description Design Tracking

> **Acceptance scenarios not defined.** This design predates acceptance scenarios
> (`specs/DOCS_STRUCTURE_GUIDE.md` §5.2.1). Consider updating it: state the Phase 1 acceptance
> criteria as `AC-<n>` WHEN/THEN scenarios and cite each from the Phase 3 test that proves it. Remove
> this note when you do; until then `docs_index.py --check` counts this design in its warning.

Autonomous bulk design (`guides/autonomous_bulk_design.md`) for
`CONTEXT-CANNOT-SET-TITLE-OR-DESCRIPTION`.

## Phase Status

- [x] Phase 1: High-Level Design
- [x] Phase 2: Solution & Architecture
- [x] Phase 3: Examples and Tests
- [x] Phase 4: Implementation Plan
- [x] Phase 5: Documentation (executed 2026-10-06; awaiting approval)

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

Adopted the five-phase `liquers-project` contract on 2026-10-05 (maintainer request after the
post-Phase-4 review); Phase 5 holds the documentation plan.

---
id: ACCESS-POLICY-AT-ENTRY-POINTS
kind: design
title: Principals and a key-pattern access policy checked where requests enter
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/context, core/error, axum, py, web, docs]
issues: [CORE-SESSION-AND-KEY-ACL]
affects_docs: [specs/reference/WEB_API_SPECIFICATION.md, specs/guides/WEB_API_GUIDE.md, specs/reference/PROJECT_OVERVIEW.md, specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md, specs/reference/POSITIONING.md]
created: 2026-10-20
---
# Principals and a key-pattern access policy checked where requests enter

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written for a **first slice**
(core policy model + `liquers-axum` enforcement), not approved, nothing implemented. The leading
decision (Q1) chooses between this design's route and the issue's stated expectation; the plan is
written under the recommended answer. Follow-ups are filed as issues (Phase 1, Scope).

## Phase Status

- [x] Phase 1: High-Level Design
- [x] Phase 2: Solution & Architecture
- [x] Phase 3: Examples & Tests
- [x] Phase 4: Implementation Plan
- Phase 5: none — four-phase bulk design

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)

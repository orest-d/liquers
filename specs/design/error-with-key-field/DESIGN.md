---
id: ERROR-WITH-KEY-FIELD
kind: design
title: Structured error context for keys and nested queries
workflow: liquers-project
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/error, core/query, core/assets, core/store, web, py, axum]
issues: [ERROR-WITH-KEY-SETS-QUERY-FIELD]
gh_pr: []
affects_docs: [reference/ERROR_CONTEXT.md, reference/PROJECT_OVERVIEW.md, reference/ASSETS.md, reference/ASSET_LIFECYCLE.md, guides/LANGUAGE-INTEGRATION_GUIDE.md, reference/WEB_API_SPECIFICATION.md, reference/PYTHON_INTEGRATION.md, reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md]
created: 2026-08-29
superseded_by:
---
# Structured Error Context Design Tracking

Resumed 2026-10-20 in bulk design (`specs/guides/autonomous_bulk_design.md` §11). The 2026-09-03
Phase 2 blockers are carried as recommended answers (Phase 1, Design Readiness and Open Questions),
and the maintainer's "no wire or serde backward compatibility yet" removes the projection blocker.
Phases 3-4 are written under those answers. Model: `ErrorPayload.context`, an ordered list of
`Asset` / `Link` / `Action` / `Resource` frames, innermost first; the flat fields name where the
error was raised; `with_key` sets `key`. Store references travel in the `Resource` frame's `store`.

## Phase Status

- [x] Phase 1: High-Level Design (AC-1 to AC-10; readiness `needs-decision`, Q1-Q6)
- [x] Phase 2: Solution & Architecture
- [x] Phase 3: Examples and Tests
- [x] Phase 4: Implementation Plan (not approved; nothing implemented)
- [ ] Phase 5: Documentation

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)

---
id: ERROR-WITH-KEY-FIELD
kind: design
title: Structured error context for keys and nested queries
workflow: liquers-project
status: in_review
phase: architecture
readiness: phase2-blocked
area: [core/error, core/query, core/assets, core/store, web, py, axum]
issues: [ERROR-WITH-KEY-SETS-QUERY-FIELD]
gh_pr: []
affects_docs: [reference/ERROR_CONTEXT.md, reference/PROJECT_OVERVIEW.md, reference/ASSETS.md, reference/ASSET_LIFECYCLE.md, guides/LANGUAGE-INTEGRATION_GUIDE.md, reference/WEB_API_SPECIFICATION.md]
created: 2026-08-29
superseded_by:
---
# Structured Error Context Design Tracking

The original one-field repair has returned to Phase 2 because keyed recipes and nested query
evaluation require multiple role-bearing contexts.

## Phase Status

- [x] Phase 1: High-Level Design
- [x] Phase 2: Solution & Architecture - complete as a blocked decision record
- [ ] Phase 3: Examples and Tests - prior draft invalidated by Phase 2 blockers
- [ ] Phase 4: Implementation - prior draft invalidated by Phase 2 blockers
- [ ] Phase 5: Documentation

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)

---
id: RESOURCE-NAME-UNICODE
kind: design
title: Resource names take any Unicode name, spelled literally or with entities; the decoded name is the key
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/query, core/store, store/backends, web]
issues: [RESOURCE-NAME-ASCII-ONLY]
affects_docs: [specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md, specs/guides/QUERY_ESCAPING_GUIDE.md, specs/reference/PROJECT_OVERVIEW.md, specs/reference/STORE_SEMANTICS.md, specs/guides/STORE_IMPLEMENTATION_GUIDE.md, specs/reference/TEMPLATE_LANGUAGE.md]
created: 2026-10-21
---
# Resource names take any Unicode name, spelled literally or with entities; the decoded name is the key

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written on 2026-10-21, not
approved, nothing implemented. The issue asks for a decision before an implementation. This design
writes the plan under the recommended answers (option C, contract 2) and lists the alternatives with
their consequences in Phase 1's Design Readiness.

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

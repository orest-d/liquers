---
id: ERROR-TYPE-TAXONOMY
kind: design
title: Error types clustered into a category graph, with General retired from library code
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/error, core/assets, core/value, records, lib/value, lib/polars, lib/image, lib/ui, py, web, axum, build]
issues: [ERROR-TYPES-TOO-COARSE-AND-GENERAL-OVERUSED, LIBRARY-CODE-CALLS-ERROR-NEW-DIRECTLY]
merged: 2026-10-26
affects_docs: [specs/reference/WEB_API_SPECIFICATION.md, specs/reference/PYTHON_INTEGRATION.md, specs/reference/PROJECT_OVERVIEW.md, specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md, specs/guides/PYTHON_GUIDE.md]
created: 2026-10-26
---
# Error types clustered into a category graph, with General retired from library code

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented. Two sources share it (`overlap.md` **T2**: the typed constructors
the `Error::new` sites need, and the split of `SerializationError` they fall under, are this
design's contract; **T3**: Stage 1 rewrites the same serializer arms). E1 does not apply:
`LIBRARY-CODE-CALLS-ERROR-NEW-DIRECTLY` spans two crates (auto-fix rule 6).
`ERROR-TYPES-TOO-COARSE-AND-GENERAL-OVERUSED` leads.

Eight new types (`ItemNotFound`, `OutOfRange`, `InvalidData`, `FormatNotSupported`,
`RecipeNotFound`, `DependencyFailed`, `AlreadyExists`, `PayloadMissing`), five new categories
(`InvalidInput`, `Unsupported`, `Conflict`, `Dependency`, `Data`), `CacheNotSupported` removed,
`Error::new` private, about 636 library `General` sites re-typed by one ordered rule in five
crate-group PRs, and a CI script that keeps `General` out of workspace library code.

## Phase Status

- [x] Phase 1: High-Level Design (AC-1 to AC-11; readiness `needs-decision`, Q1-Q7)
- [x] Phase 2: Solution & Architecture
- [x] Phase 3: Examples & Tests
- [x] Phase 4: Implementation Plan (not approved; nothing implemented)
- Phase 5: none — four-phase bulk design

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)

---
id: AXUM-QUERY-NAMED-ARGUMENTS
kind: design
title: Argument overrides over HTTP and WebSocket
workflow: liquers-project
status: in_review
phase: documentation
readiness: ready
autofix: not-eligible
area: [axum, core/assets, core/plan]
affects_docs: [reference/ASSET_LIFECYCLE.md, reference/WEB_API_SPECIFICATION.md, guides/WEB_API_GUIDE.md, guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md, reference/api/DOC_08_RECIPES_PLANS.md, reference/COMMAND_ALIASES.md]
issues: [QUERY-API-ARGUMENTS-ONLY-IN-QUERY-PATH]
created: 2026-10-11
---
# Argument overrides over HTTP and WebSocket

Named argument overrides for the Query API, the Assets API's `q/data`, `q/entry` and `q/submit`,
and WebSocket `subscribe` with `arguments`, which follows the unmanaged evaluation it starts. Converted from the
compact to the full form on 2026-10-12, when the scope grew from the Query API to the Assets API
and the WebSocket (see Phase 1, Scope Changes).

Pre-approved after Phase 4 on 2026-10-12 (`proceed all`): implementation and Phase 5.

## Phase Status

- [x] Phase 1: High-Level Design (approved 2026-10-12)
- [x] Phase 2: Architecture (approved 2026-10-12)
- [x] Phase 3: Examples and Tests (approved 2026-10-12)
- [x] Phase 4: Implementation Plan (approved 2026-10-12)
- [ ] Phase 5: Documentation (in review)

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

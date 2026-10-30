---
id: WEB-UI-USE
kind: design
title: Mount, drive and observe a Liquers UI from JavaScript (UIUSE)
workflow: liquers-project
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [web, lib/ui]
issues: [WEB-CANNOT-USE-A-LIQUERS-UI]
affects_docs: [specs/reference/JAVASCRIPT_INTEGRATION.md, specs/guides/JAVASCRIPT_GUIDE.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md]
created: 2026-10-27
---
# Mount, drive and observe a Liquers UI from JavaScript (UIUSE)

The `UIUSE` feature of `guides/LANGUAGE-INTEGRATION_GUIDE.md` for liquers-web: a page mounts the
existing `liquers-lib` web UI backend into a DOM element, submits queries to its elements, follows
their status and disposes of it. `web-ui-define` (`UIDEF`) builds on it.

Pre-approved after Phase 1 on 2026-10-30 (`proceed all`): Phases 2–4 were written without
stopping. The decision log is in Phase 1's Design Readiness section.

## Phase Status

- [x] Phase 1: High-Level Design — decisions recorded 2026-10-27
- [x] Phase 2: Architecture — 2026-10-30
- [x] Phase 3: Examples and Testing — 2026-10-30
- [x] Phase 4: Implementation Plan — 2026-10-30; at the pre-implementation stop
- [ ] Phase 5: Documentation

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

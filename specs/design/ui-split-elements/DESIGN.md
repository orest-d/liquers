---
id: UI-SPLIT-ELEMENTS
kind: design
title: UI elements split into a server half and a client half, linked by update messages
workflow: liquers-project
status: in_review
phase: high-level
area: [lib/ui, axum, web]
issues: [WEB-UI-CANNOT-BE-DRIVEN-BY-A-REMOTE-ENGINE, AXUM-CANNOT-SERVE-A-LIQUERS-UI]
affects_docs: [specs/reference/UI_INTERFACE_FSD.md, specs/reference/WEB_API_SPECIFICATION.md]
created: 2026-11-14
---
# UI elements split into a server half and a client half

Started 2026-11-14 from a project discussion with Orest on serving the web UI from axum or a
desktop shell. Phase 1 written, awaiting review at the gate.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

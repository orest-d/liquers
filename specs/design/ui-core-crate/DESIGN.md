---
id: UI-CORE-CRATE
kind: design
title: The UI core crate — one element model for every frontend, independent of liquers-lib
workflow: liquers-project
status: in_review
phase: high-level
area: [lib/ui, lib/egui, web]
issues: [UI-NOT-SEPARATED-BY-CRATE-AND-TECHNIQUE, UI-INTERFACE-FSD-DESCRIBES-UNBUILT-API, WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS, UI-ELEMENT-KEEPS-AN-EXPIRED-VALUE-AND-NEVER-RE-REQUESTS, EGUI-ASSET-MANAGER-INTEGRATION]
affects_docs: []
created: 2026-11-22
---
# The UI core crate

Design D1 of the [UI redesign plan](../ui-redesign/plan.md), milestone M0/M1. Phase 2 waits for
spikes S1 (egui, ratatui and web on draft traits) and S5 (Slint).

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

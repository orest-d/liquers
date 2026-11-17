---
id: UI-SPLIT-ELEMENTS
kind: design
title: UI elements split into a server half and a client half, linked by update messages
status: complete
area: [lib/ui, axum, web]
issues: [WEB-UI-CANNOT-BE-DRIVEN-BY-A-REMOTE-ENGINE, AXUM-CANNOT-SERVE-A-LIQUERS-UI, UI-NOT-SEPARATED-BY-CRATE-AND-TECHNIQUE]
affects_docs: []
created: 2026-11-14
---
# UI elements split into a server half and a client half

**Investigative design, closed 2026-11-17 by Orest.** Nothing here is implemented or approved for
implementation. The discussion showed the split belongs in a wider UI redesign: the UI in its own
crate(s), one trait per technique (monolithic web, split server model + client view, egui, …), and a
clear client-server protocol. That redesign is `UI-NOT-SEPARATED-BY-CRATE-AND-TECHNIQUE` (P1); its
design should start from this folder.

What is reusable:

- [Phase 1](./phase1-high-level-design.md): the problem example (a 2-million-row DataFrame and a PNG
  chart), acceptance scenarios AC-1..AC-9, and the answered questions: envelope, server authority,
  WebSocket, single local session first, default wasm client.
- [Discussion](./element-protocol-discussion.md): mode flag vs per-mode methods vs model/view pair
  (option C chosen), trait signatures, the wire messages, runner and AppState support, the asset
  viewer and big-table walk-throughs, and Orest's answers of 2026-11-17 (one trait per frontend,
  `RecordView` pages, old and new UI in parallel, multi-client sessions kept possible).

Phases 2-5 were never written; their empty templates were removed when the design closed.

## Documents

- [Phase 1](./phase1-high-level-design.md)
- [Element protocol discussion](./element-protocol-discussion.md)

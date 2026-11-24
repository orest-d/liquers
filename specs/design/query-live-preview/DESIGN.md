---
id: QUERY-LIVE-PREVIEW
kind: design
title: Query live preview - preview, apply and undo edits, and a console value that does not flicker
workflow: liquers-project
status: in_review
phase: high-level
priority: P2
complexity: L
area: [lib/ui, lib/egui]
issues: [QUERY-EDITOR-HAS-NO-LIVE-PREVIEW, QUERY-CONSOLE-EDITOR-LOSES-EDITS-WHEN-UPDATE-IMMEDIATELY-IS-OFF]
affects_docs: [specs/reference/UI_INTERFACE_FSD.md, docs/guides/rust/11-ui.md, docs/guides/python/11-ui.md, docs/guides/web/11-ui.md]
created: 2026-11-24
---
# Query live preview

A live preview for the query editors and the query console: an edit is evaluated and shown before
it is applied upstream, with Apply, Revert and Undo/Redo; the console re-evaluates on a panel
edit; and the value display keeps the old value until the new one is there, with a status bar and
a dimmed previous value when it takes a while.

Started from Orest's request of 2026-11-24, on top of PRs #283 and #284 (query console editor
UI). Phase 1 written on 2026-11-24; the update model (Open Question 1) was put to Orest the same
day.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

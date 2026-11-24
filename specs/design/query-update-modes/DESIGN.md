---
id: QUERY-UPDATE-MODES
kind: design
title: Update modes for the query editors and console - on demand, live preview and apply immediately, with undo and a value display that does not flicker
workflow: liquers-project
status: in_review
phase: high-level
priority: P2
complexity: L
area: [lib/ui, lib/egui]
issues: [QUERY-CONSOLE-EDITOR-LOSES-EDITS-WHEN-UPDATE-IMMEDIATELY-IS-OFF, QUERY-CONSOLE-EDITOR-CANNOT-BROWSE-THE-STORE, QUERY-EDITOR-HAS-NO-LIVE-PREVIEW]
affects_docs: [specs/reference/UI_INTERFACE_FSD.md, specs/command_registry.yaml, docs/guides/rust/11-ui.md, docs/guides/python/11-ui.md, docs/guides/web/11-ui.md]
created: 2026-11-24
---
# Update modes: on demand, live preview, apply immediately

The query editors and the query console share one model. A GUI edit always rewrites the query;
the mode decides what follows: **On demand** waits for the ✓ button beside the query field or
Enter, **Live preview** evaluates the edit in a preview pane without writing it (an editor of a
recipe sees its new result before the change cascades into costly dependents), and **Apply
immediately** writes it, so dependents expire and views that follow them recalculate. Typed text
counts only on Enter, when valid. Undo/Redo covers every mode; the value display keeps the old value
until the new one is there, with a status bar and a dimmed previous value when it takes a while.

History, 2026-11-24 (Orest): first drafted as a live preview (`query-live-preview`); then set to
Apply immediately on/off with the ✓ button and no second Update button in the console, folding in
related issues (`query-apply-immediately`); then, so an edit's impact on one recipe can be seen
before it cascades, all three modes (`query-update-modes`), with a mode argument per element.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

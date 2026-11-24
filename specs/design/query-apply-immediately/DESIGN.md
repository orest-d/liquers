---
id: QUERY-APPLY-IMMEDIATELY
kind: design
title: Apply immediately and Update for the query editors and console, with undo and a value display that does not flicker
workflow: liquers-project
status: in_review
phase: high-level
priority: P2
complexity: L
area: [lib/ui, lib/egui]
issues: [QUERY-CONSOLE-EDITOR-LOSES-EDITS-WHEN-UPDATE-IMMEDIATELY-IS-OFF, QUERY-CONSOLE-EDITOR-CANNOT-BROWSE-THE-STORE]
affects_docs: [specs/reference/UI_INTERFACE_FSD.md, docs/guides/rust/11-ui.md, docs/guides/python/11-ui.md, docs/guides/web/11-ui.md]
created: 2026-11-24
---
# Apply immediately and Update

The query editors and the query console share one model: a GUI edit always rewrites the query;
with Apply immediately on, the value is recalculated (the accessor written) at once, and with it
off, on the ✓ button beside the query field or Enter. Typed text counts only on Enter, when valid.
Undo/Redo covers both; the value display keeps the old value until the new one is there, with a
status bar and a dimmed previous value when it takes a while.

Started from Orest's request of 2026-11-24, on top of PRs #283 and #284. Phase 1 was first written
as a live preview (slug `query-live-preview`); the same day Orest set the model above, asked for
the ✓ icon button and no second Update button in the console, and asked to fold in related issues.
Phase 1 rewritten and renamed accordingly; live preview deferred to `QUERY-EDITOR-HAS-NO-LIVE-PREVIEW`.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

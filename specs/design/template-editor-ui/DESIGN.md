---
id: TEMPLATE-EDITOR-UI
kind: design
title: A template editor with live diagnostics, saving and a rendered preview
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [lib/ui, lib/egui, lib/commands]
issues: [NO-TEMPLATE-EDITOR-UI]
affects_docs: [TEMPLATE_LANGUAGE, UI_INTERFACE_FSD, TEMPLATE_GUIDE]
created: 2026-10-21
---
# A template editor with live diagnostics, saving and a rendered preview

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written on 2026-10-21, not
approved, nothing implemented. Readiness `needs-decision`: one editor element with modes (leading
question) and previewing only the saved template (question 3) are assumed as recommended.

The last design of a chain: it composes `ui-asset-snapshot-boundary` (the element),
`text-editor-save` (saving), `query-editor-live-validation` (registry in `UIContext`, diagnostics,
`edit_query_live` for query fields) and `template-syntax-highlighter` (tokens), and adds the
`template/editor` command and a preview pane fed by `RequestAssetUpdates`.

## Phases

- [Phase 1](./phase1-high-level-design.md) — reviewed; AC-1 to AC-8
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md) — 5 steps

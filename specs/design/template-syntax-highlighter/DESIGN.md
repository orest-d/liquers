---
id: TEMPLATE-SYNTAX-HIGHLIGHTER
kind: design
title: Templates rendered with syntax highlighting and marked diagnostics
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [lib/commands, lib/egui, core/query]
issues: [TEMPLATE-HAS-NO-HIGHLIGHTER]
affects_docs: [TEMPLATE_LANGUAGE, TEMPLATE_GUIDE]
created: 2026-10-21
---
# Templates rendered with syntax highlighting and marked diagnostics

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written on 2026-10-21, not
approved, nothing implemented. Readiness `needs-decision`: byte-range tokens replace the issue's
text-carrying token sketch (Phase 1 leading question), assumed as recommended.

`liquers-template` gets `highlight` (AST walk to byte-range tokens, query bodies tiled with core's
`tile_syntax_marks` from `query-editor-live-validation`), HTML and ANSI styles; `liquers-lib` gets
`template/highlight` and an egui layout helper. The issue is re-sized from `M` to `L`.

## Phases

- [Phase 1](./phase1-high-level-design.md) — reviewed; AC-1 to AC-9
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md) — 5 steps

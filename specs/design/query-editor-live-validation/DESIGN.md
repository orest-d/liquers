---
id: QUERY-EDITOR-LIVE-VALIDATION
kind: design
title: A query editor that tokenizes and validates the text being typed
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [lib/ui, lib/egui, core/query]
issues: [UI-QUERY-EDITOR-LACKS-LIVE-VALIDATION]
affects_docs: [UI_INTERFACE_FSD, DOC_02_QUERY_LANGUAGE_REFERENCE]
created: 2026-10-21
---
# A query editor that tokenizes and validates the text being typed

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written on 2026-10-21, not
approved, nothing implemented. Readiness `needs-decision`: the source-span token model (Phase 1
leading question) is assumed as recommended.

Core gains source-faithful syntax tokens; `liquers-lib/src/ui/query_editor.rs` analyses the text
with `validate_query` against the registry that `UIContext` now carries; egui lays it out over the
edit buffer and the web backend renders classed spans. Live per-keystroke analysis in the browser
is a follow-up (`WEB-QUERY-EDITOR-NOT-LIVE`).

## Phases

- [Phase 1](./phase1-high-level-design.md) — reviewed; AC-1 to AC-8
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md) — 7 steps

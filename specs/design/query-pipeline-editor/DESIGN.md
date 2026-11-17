---
id: QUERY-PIPELINE-EDITOR
kind: design
title: Query, pipeline, action, resource and key editors as UI elements
workflow: liquers-project
status: in_review
phase: architecture
area: [lib/ui, lib/egui, core/query]
issues: []
affects_docs: [specs/reference/UI_INTERFACE_FSD.md, specs/reference/PIPELINES.md, specs/guides/VALUE_ACCESSOR_GUIDE.md, specs/guides/COMMAND_REGISTRATION_GUIDE.md]
created: 2026-11-16
---
# Query, pipeline, action, resource and key editors as UI elements

A query or pipeline editor shown as a collapsible tree of steps (a link argument is a subtree)
beside the highlighted query text; an action editor whose fields come from each argument's
`gui_info`, with the command and argument documentation on hover, presets, next and filename buttons; and resource and key editors with a store file selector. All are
`UIElement`s bound to value accessors (`settable-value-accessor`), for egui, the browser, and the
split UI of `ui-split-elements`.

Phase 1 written and revised with Orest's notes on 2026-11-16 (it supersedes `command-parameter-editor`); approved by Orest on 2026-11-17 (`proceed`). Phase 2 written on 2026-11-17 and revised the same day with Orest's review (no model / view split, safe namespace edits, continuous or on-demand update), waiting for review.

## Phases

- [Phase 1](./phase1-high-level-design.md): AC-1 to AC-26
- [Phase 2](./phase2-architecture.md): monolithic view and control bound through the accessor, decisions G1 to G8
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

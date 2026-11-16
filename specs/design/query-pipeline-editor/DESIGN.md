---
id: QUERY-PIPELINE-EDITOR
kind: design
title: Query, pipeline, action, resource and key editors as UI elements
workflow: liquers-project
status: in_review
phase: high-level
area: [lib/ui, lib/egui, core/query]
issues: []
affects_docs: [specs/reference/UI_INTERFACE_FSD.md, specs/guides/VALUE_ACCESSOR_GUIDE.md, specs/guides/COMMAND_REGISTRATION_GUIDE.md]
created: 2026-11-16
---
# Query, pipeline, action, resource and key editors as UI elements

A query or pipeline editor shown as a collapsible tree of steps (a link argument is a subtree)
beside the highlighted query text; an action editor whose fields come from each argument's
`gui_info`, with the command and argument documentation on hover, presets, next and filename buttons; and resource and key editors with a store file selector. All are
`UIElement`s bound to value accessors (`settable-value-accessor`), for egui, the browser, and the
split UI of `ui-split-elements`.

Phase 1 written on 2026-11-16 and revised the same day with Orest's notes; it supersedes `command-parameter-editor`. Waiting for review.

## Phases

- [Phase 1](./phase1-high-level-design.md): AC-1 to AC-24
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

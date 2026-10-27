---
id: COMMAND-PARAMETER-EDITOR
kind: design
title: A form that edits the last action's arguments from their gui_info
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [lib/ui, lib/egui, core/plan]
issues: [NO-COMMAND-PARAMETER-EDITOR]
affects_docs: [specs/reference/UI_INTERFACE_FSD.md, specs/reference/RECIPES_AND_PLANS.md]
created: 2026-10-26
---
# A form that edits the last action's arguments from their gui_info

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented. Readiness `needs-decision`: the host (a panel of the query console)
and the type-derived widget rule are proposed resolutions. Implementation needs
`variadic-argument-slots` and Step 3 of `query-editor-live-validation` first, and needs `ui-events`
before its browser half.

A backend-free `ParameterForm` in `liquers-lib/src/ui/parameter_form.rs` reads the last action and
its `CommandMetadata`. It makes one field per visible argument, with the effective `gui_info`. Edits
go through the core slot functions, including a new scalar edit `set_argument_parameter`. egui renders
the form as a toggled panel in `QueryConsoleElement`. The browser renders it as markup whose fields
and buttons travel as `ui-events` user events.

## Phases

- [Phase 1](./phase1-high-level-design.md): AC-1 to AC-11
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md): 8 steps

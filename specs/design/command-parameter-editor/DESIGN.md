---
id: COMMAND-PARAMETER-EDITOR
kind: design
title: A form that edits the last action's arguments from their gui_info
status: superseded
area: [lib/ui, lib/egui, core/plan]
issues: [NO-COMMAND-PARAMETER-EDITOR]
affects_docs: [specs/reference/UI_INTERFACE_FSD.md, specs/reference/RECIPES_AND_PLANS.md]
created: 2026-10-26
superseded_by: query-pipeline-editor
---
# A form that edits the last action's arguments from their gui_info

> **Superseded on 2026-11-16.** Orest's decision: [`query-pipeline-editor`](../query-pipeline-editor/)
> replaces this design and now owns `NO-COMMAND-PARAMETER-EDITOR`. Its action editor is this form,
> generalised to any action and bound through value accessors. Moved there: the type-derived widget
> rule (AC-5), the `gui_info` widget mapping for egui and the browser, enum and `GlobalEnum`
> handling, read-only excess parameters (AC-8), "no form, with a reason" (AC-7), reset to default
> (AC-4) and the console's Parameters panel (AC-10). Dropped: `set_argument_parameter`, which
> duplicates `ActionEditor::set`. Nothing was implemented; this folder is kept for its reasoning.

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

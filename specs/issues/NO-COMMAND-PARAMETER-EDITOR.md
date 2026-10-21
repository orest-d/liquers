---
id: NO-COMMAND-PARAMETER-EDITOR
kind: feature
title: No UI renders a command's arguments as an editable form from their gui_info
status: draft
priority: P3
complexity: L
area: [lib/ui, lib/egui]
design:
created: 2026-10-21
github:
---
## Problem

`ArgumentInfo::gui_info` (`liquers-core/src/command_metadata.rs`) describes the widget for each
command argument, and `ArgumentInfo::multiple` says that a variadic argument is a list of such
widgets. Nothing in the workspace reads `gui_info` (`grep` over `liquers-lib`, `liquers-web`,
`liquers-axum`, `liquers-py` at `8ecfd7f`); the only consumer is the out-of-tree prototype
`orest-d/egui-midi-test` (`src/editor.rs`, `edit_query`). So there is no in-tree form that edits
an action's parameters, and no place for the variadic list editor of
`UI-VARIADIC-ARGUMENT-LIST-EDITOR` to live.

## Impact

Users edit queries only as text (the query console). Commands' declared labels, descriptions,
enums and widget hints are unused by the UI. Workaround: type the query.

## Expected behaviour

A parameter editor element (egui first, the web backend through `render_web`) that, given a query,
shows the last action's arguments with the widget `gui_info` names, writes each change back into
the query, and renders a `multiple` argument as an ordered list with add, delete and reorder. It
should use `argument_slots` and the variadic edit functions of
`specs/design/variadic-argument-slots/` for positions, and `edit_query_live`
(`specs/design/query-editor-live-validation/`) for link-valued arguments.

## Discovery

Split out of `UI-VARIADIC-ARGUMENT-LIST-EDITOR` during bulk design on 2026-10-21: its step 3 (the
list renderer) has no in-tree editor to extend, so the editor itself is recorded here.

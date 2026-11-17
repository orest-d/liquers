---
id: QUERY-CONSOLE-HAS-NO-QUERY-EDITOR-ON-DEMAND
kind: feature
title: The query console cannot show a query editor of its whole query on demand
status: draft
priority: P3
complexity: M
area: [lib/ui, lib/egui, web]
design:
created: 2026-11-17
github:
---
## Problem

The query console (`liquers-lib/src/ui/widgets/query_console_element.rs`) edits its query as text.
Its Parameters panel (from `specs/design/query-pipeline-editor/`) shows only the **last action**
of the text, as an action editor. Nothing opens the full query editor of that design (the tree of
resource, `ns`, actions, directives and link subtrees, with insert, delete, move and the command
picker) over the console's query.

The only way to edit the whole query structurally today is a separate `lui/query_editor` element
bound to an accessor, as in `liquers-lib/examples/ui_spec_editor.rs`; a console has no accessor of
its own text to bind it to.

## Impact

A console user can change the last step's arguments in a form, but must type to insert, remove or
reorder steps, change a resource header, or edit a link argument. Workaround: edit the text, or put
the query in a recipe and open a query editor on the recipe's accessor.

## Expected behaviour

A console toggle (beside Parameters, or replacing it with a choice of "last action" / "whole
query") that shows an `EditorState` of kind `Query` over the console's text, with every node
drawn by `egui::editor::show_editor` (and `ui::web::editor::editor_html` in the browser). Each
accepted edit rewrites the console text without submitting; Enter or the console's submit
evaluates it. The editor follows the text when the user types (a parse error marks the tree
stale, as the editor already does). The toggle is saved with the console (`#[serde(default)]`).

Open: whether the panel binds to a memory accessor of the console text (so another element could
share it) or keeps the direct copy the Parameters panel uses.

## Discovery

Orest, 2026-11-17, while asking for a UI spec example with an editor
(`liquers-lib/examples/ui_spec_editor.rs`).

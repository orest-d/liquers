---
id: NO-RECIPE-OR-RECIPES-EDITOR-UI
kind: feature
title: There is no UI element that edits a whole recipe or a folder's recipes.yaml
status: in_progress
priority: P3
complexity: L
area: [lib/ui, lib/egui]
created: 2026-11-24
design: recipes-editor
github:
---
## Problem

A recipe's **query** can be edited in the GUI: `lui/query_editor` bound through an accessor to
`-R-key/reports/recipes.yaml/-/ns-acc/accessor/recipe-0/query` (`ui::editor::EditorElement`,
`specs/design/query-pipeline-editor/`). Nothing edits the rest of a recipe (title, description,
filename, cwd, volatility, expiration, overrides), and nothing edits a folder's recipe list as a
whole: add, rename, delete or move a recipe, or the `tree:` section. The core has the edits —
`liquers_core::recipe_tree::TreeEditor` adds, renames, moves and deletes nodes, rewriting every
reference — but the UI uses it only to rename a Node argument's node (`EditorCommand::RenameNode`).

## Problem Example

A person building `reports/recipes.yaml` in the egui app can change recipe 0's query, but must edit
the YAML by hand to add a second recipe, set its filename, or rename `wheel` to `rim` across the
folder.

## Expected behaviour

A recipe editor (the recipe's fields, with the query editor inside) and a recipes editor (the
folder's recipes as a list or tree, with `TreeEditor`'s edits and its check findings), both bound
through accessors so the update modes of `specs/design/query-update-modes/` apply to them.
Noticed 2026-11-24 while designing `query-update-modes`.

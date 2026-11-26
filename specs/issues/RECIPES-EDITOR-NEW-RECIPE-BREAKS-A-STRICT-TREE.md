---
id: RECIPES-EDITOR-NEW-RECIPE-BREAKS-A-STRICT-TREE
kind: issue
title: The recipes editor's New recipe writes a missing source, which the provider refuses in a strict tree
status: closed
priority: P1
complexity: S
area: [lib/ui, lib/egui, core/assets]
design: strict-tree-editing
created: 2026-11-26
github:
---
## Problem

`RecipesCommand::New` writes `{title, query: <title>.<ext>}`, a declared source with no data.
`TreeEditor::commit_new` tolerates that node's own `MissingSource` (design `recipes-editor` AC-7),
but the written file is then refused by `RecipesProvider` in a strict tree (design `recipe-tree`
AC-24), so the first node made with ＋ breaks the folder
(`STRICT-TREE-FINDING-LOCKS-ITS-RECIPES-FILE`). The dialog cannot take a query, and its extension
defaults to `txt`. The egui toolbar's ≡, ⑂ and ＋ also draw as empty boxes (no glyphs in egui's
default fonts), so the button is hard to find. Same in the web panel (`rc:new`).

## Expected behaviour

New recipe creates a node valid in a strict tree, or a draft node the provider tolerates until its
query is written; the dialog can take a query; the toolbar uses Phosphor glyphs.

## Workaround

Write the recipe in the YAML view in one valid step.

## Resolution

2026-11-26, design `strict-tree-editing` (PR #292, commits 722943d, 9def70b): New adds an empty
node `<n>.node` (`TreeEditor::add_empty`), which is not a source and is valid in any tree; its
query and filename are set in its window, a query typed without a filename keeps the node's, and a
filename renames the node uniquely. Evidence: `new_adds_an_empty_node_and_writes_a_valid_tree`,
`query_edit_keeps_the_node_filename`, `filename_edit_renames_uniquely` (`liquers-lib`),
`empty_node_evaluates_to_none_and_is_no_source` (`liquers-core`), and the DPL egui workflow.

---
id: RECIPES-EDITOR-NEW-RECIPE-BREAKS-A-STRICT-TREE
kind: issue
title: The recipes editor's New recipe writes a missing source, which the provider refuses in a strict tree
status: draft
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

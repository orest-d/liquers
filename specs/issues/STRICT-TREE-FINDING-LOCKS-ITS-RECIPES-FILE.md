---
id: STRICT-TREE-FINDING-LOCKS-ITS-RECIPES-FILE
kind: issue
title: One error finding in a strict recipe tree makes its recipes.yaml unreadable and unwritable in the recipes editor
status: closed
priority: P1
complexity: M
area: [core/assets, lib/ui, lib/egui]
design: strict-tree-editing
created: 2026-11-26
github:
---
## Problem

`RecipesProvider` refuses a strict folder as a whole when `check_tree` reports any error
(`liquers-core/src/recipes.rs`, design `recipe-tree` AC-24). The recipes editor reads and writes
the folder's `recipes.yaml` through `-R-key/<folder>/recipes.yaml/-/ns-acc/accessor`, and resolving
that key consults the folder's recipes, so both the read and `set_if` fail with the finding
(`Recipe tree: source 'wave.obj' is declared but not stored`).

- The write that makes the tree invalid reaches the store, but the editor gets
  `AccessorUpdate::Failed`, keeps its old fingerprint, and every later edit fails too. The editor
  and its YAML view show the edits; nothing more is saved, and the only sign is the finding's text.
- Reopened, the panel has no rows, ＋ is disabled and the YAML view is empty: the folder cannot be
  repaired from the UI.

Same in egui and the web panel (shared `RecipesState`). Found by DPL (orest-d/dpl issue
`LIQUERS-STRICT-TREE-FINDING-LOCKS-THE-RECIPES-FILE`), with traces in `RecipesElement::on_accessor`
and `AppRunner::handle_accessor`.

## Impact

Any invalid intermediate state of a strict tree, including the one ＋ creates
(`RECIPES-EDITOR-NEW-RECIPE-BREAKS-A-STRICT-TREE`), silently stops saving and makes the folder
uneditable after a restart.

## Expected behaviour

The tree check guards evaluation, not editing: `recipes.yaml` stays readable and writable, the
editor shows the findings, and a failed write shows as unsaved edits.

## Workaround

Fix `recipes.yaml` outside Liquers.

## Resolution

2026-11-26, design `strict-tree-editing` (PR #292, commits 722943d, eb546af): the provider no
longer refuses a strict folder that fails its check; it serves the recipes as plain recipes, and
the editors show the findings as a notice. `recipes.yaml` stays readable and writable. Evidence:
`invalid_strict_folder_downgrades_to_plain_recipes`,
`recipes_yaml_of_an_invalid_tree_is_readable_and_writable` (`liquers-core/tests/recipe_tree.rs`),
`invalid_tree_shows_a_notice` (`liquers-lib`), and DPL's egui workflow re-run against the branch
(new project, New node, net and wave query, rename, quit, reopen, `{}`; an invalid strict file still
saves and shows the notice).

---
id: STRICT-TREE-EDITING
kind: design
title: Keep a strict recipe tree editable when it fails its check, and make New recipe create a valid node
form: compact
workflow: liquers-project
status: in_review
phase: high-level
area: [core/assets, lib/ui, lib/egui]
issues: [STRICT-TREE-FINDING-LOCKS-ITS-RECIPES-FILE, RECIPES-EDITOR-NEW-RECIPE-BREAKS-A-STRICT-TREE]
created: 2026-11-26
---
# Keep a strict recipe tree editable

Two merged designs disagree. `recipe-tree` (AC-24) makes a strict tree "valid as a whole or not
used": `RecipesProvider` refuses the folder on any error finding. `recipes-editor` (AC-7) lets ＋
write a new recipe as a source declaration, `{title, query: <file>}`, which is a `MissingSource`
error until its query is written. Combined, the first node made with ＋ in a strict folder locks the
folder's own `recipes.yaml`: writes fail silently, and after a restart the editor is empty and
read-only. Found by DPL, whose projects are strict trees (orest-d/dpl thread "Test egui project
save and reopen workflow"; DPL's own half is its design `project-node-stopgap`).

- **2026-11-26** Phase 1 from Orest's request to fix it upstream and in DPL, in egui and the web UI.

## Phase 1: High-Level Design

### Purpose

The tree check protects *evaluation*. It must never stop a user from reading, editing and saving
the `recipes.yaml` that describes the tree, and the recipes editor's own actions must not create a
tree that the provider then refuses.

### Problem Example

A folder `p/` holds `recipes.yaml` = `tree: {dependencies: strict}`, `recipes: []`. In the recipes
panel (egui or web) the user presses ＋, enters title `wave`, extension `obj`, then types
`net/wave-z-x-0.1-6.28/wave.obj` in the recipe form.
**Today** the first write stores `query: wave.obj`, and the editor gets
`Failed(Recipe tree: source 'wave.obj' is declared but not stored)`. The query edit is shown but
never stored. Reopened, the panel shows no rows, ＋ is disabled and the YAML view is empty.
**Wanted:** the query edit is stored. Had it not been stored, the panel would say so. On reopen the
panel shows `wave` together with the tree's findings, and the YAML view shows the file.

### Scope and Acceptance Criteria

- **AC-1** An invalid strict tree stays readable
  WHEN `p/recipes.yaml` is strict and `check_tree` reports an error
  THEN the key `p/recipes.yaml` (asset, accessor, `-R-key/p/recipes.yaml`) still reads, and the
  recipes panel lists its recipes with the findings shown.
- **AC-2** And writable
  WHEN the panel writes `p/recipes.yaml` while the tree, before or after, has an error finding
  THEN the write succeeds and the editor's fingerprint follows it.
- **AC-3** Evaluation stays strict
  WHEN any other key of `p/` is evaluated while the tree has an error finding
  THEN it is refused with the finding, as today (`recipe-tree` AC-24 kept for evaluation).
- **AC-4** A failed write is visible
  WHEN a write of the recipes fails for any reason
  THEN the panel marks the edits as unsaved (as On demand's pending state) and shows the error. A
  write that reached the store is not reported as failed.
- **AC-5** New recipe is valid in a strict tree
  WHEN ＋ is used in a strict folder
  THEN the dialog takes a query (optional), and the node it writes passes `check_tree`. Without a
  query it is a draft that neither the provider nor `check_tree` counts as an error (see open
  question 1). The extension defaults to the query's last command's `filename`, else `txt`.
- **AC-6** Toolbar glyphs
  WHEN the egui panel draws Outline, Dependencies and New recipe
  THEN it uses Phosphor glyphs, as the rest of the editor does, not ≡ ⑂ ＋.
- **AC-7** Web parity
  WHEN AC-1 to AC-5 are exercised in the web panel (`liquers-lib/src/ui/web/recipes.rs`, `rc:new`)
  THEN the same results, checked by the ui_spec_demo Playwright suite.

Out of scope: DPL's New Node and repair view (DPL design `project-node-stopgap`); undo of a
destructive write.

### Language Integration

Not a new feature: behaviour of existing Rust APIs. Python (`liquers-py`) and JavaScript
(`liquers-web`) are **not needed**: neither binds the recipes editor, and both read recipes through
the same provider, so AC-1 to AC-3 reach them without changes.

### Documentation Intent

- Reference: `reference/RECIPE_TREES.md` (strictness applies to evaluation, not to the file) and
  `reference/UI_INTERFACE_FSD.md` §Recipe editors (findings, unsaved edits, the New dialog).
- API docs: `liquers-core/src/recipes.rs`, `liquers-lib/src/ui/recipes/mod.rs` module docs.
- User guides: none. The recipes editor is not in the user guides yet.
- Designs: a scope note in `recipe-tree` (AC-24) and `recipes-editor` (AC-7) pointing here.

### Open Questions

1. **A new node without a query** (AC-5): (a) require a query in the dialog when the folder is
   strict; (b) write a *draft* node, a recipe with an empty query and a filename, that `check_tree`
   reports as a warning; or (c) keep source declarations but let the provider tolerate
   `MissingSource`. Recommended: **(b)**, since it keeps ＋ quick and is explicit in the file.
2. **Where AC-1 is decided:** let `RecipesProvider` skip the strict refusal for the key
   `recipes.yaml` itself (recommended, narrow), or let the key accessor bypass recipe lookup for
   `recipes.yaml`?
3. **Is AC-3 too strict while editing?** Today one bad node stops every recipe in the folder.
   Should only the nodes that depend on the finding be refused? Recommended: not here. File it as
   a separate issue if wanted.

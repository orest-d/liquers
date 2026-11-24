---
id: RECIPES-EDITOR
kind: design
title: Recipe and recipes editors - a recipe form, a folder outline (list or tree), a dependencies view and a YAML view
workflow: liquers-project
status: approved
phase: implementation
priority: P3
complexity: L
area: [lib/ui, lib/egui, core/value, core/assets]
issues: [NO-RECIPE-OR-RECIPES-EDITOR-UI]
affects_docs: [specs/reference/UI_INTERFACE_FSD.md, specs/reference/RECIPE_TREES.md, specs/reference/VALUE_ACCESSORS.md]
created: 2026-11-24
---
# Recipe and recipes editors

GUI editors for recipes: `lui/recipe_editor` edits one recipe (its fields, with the query editor
inside); `lui/recipes_editor` edits a folder's `recipes.yaml` as an outline of filenames and titles,
or of node names when the folder is a recipe tree, as a narrow side panel with an optional
dependencies view and a highlighted YAML window. Rows carry a type icon; clicking one opens a recipe window; new, rename,
duplicate and delete go through `TreeEditor`. Recipes are addressed by filename
(`acc/recipe_file`), and the recipe list stays a document (YAML text over accessors) rather than a
new `Value` variant.

- **2026-11-24** Phase 1 written from the maintainer's request; folds in
  `NO-RECIPE-OR-RECIPES-EDITOR-UI` (filed in PR #285). Mockups in `mockups/`. Seven questions at
  the Phase 1 gate. Phase 1 is about twice the one-page limit because it carries the Value-or-document
  evaluation the maintainer asked for; that section moves to Phase 2 once decided.
- **2026-11-24, later** Maintainer review: the outline is a narrow side panel with a configurable
  label (name, title or both), status circles or progress percentage with tooltips, tooltips for
  title and description, a right-click menu (View, Metadata, Edit, Evaluate, Rename, Duplicate,
  Delete, Save as, Export), and YAML in its own window. AC-1, AC-3, AC-12 revised; AC-16 to AC-19
  added; questions 8-10 opened.
- **2026-11-24, evening** Pre-approved after Phase 1 on 2026-11-24 (`proceed all`): recommended
  answers to Q1-Q10 taken as assumptions (Phase 1 Design Readiness). Phases 2-4 written; no item
  needs a decision, so implementation starts.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

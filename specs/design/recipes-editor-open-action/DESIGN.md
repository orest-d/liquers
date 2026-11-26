---
id: RECIPES-EDITOR-OPEN-ACTION
kind: design
title: Let a host choose what a click on a recipes-editor row does
form: compact
workflow: liquers-project
status: in_review
phase: high-level
area: [lib/ui, lib/egui]
issues: [RECIPES-EDITOR-ROW-CLICK-ALWAYS-OPENS-THE-FORM]
affects_docs: [reference/UI_INTERFACE_FSD.md]
created: 2026-11-26
---
# Let a host choose what a click on a recipes-editor row does

A click on a row of the recipes editor always opens Liquers' recipe form. A host that has its own
editor for a node (DPL's recipe workbench: the query with Live preview beside a 3D view of the
result) cannot be reached from the click. This design adds an optional *open query*: when the host
sets one, a click submits it with the clicked file, and the row menu's Edit still opens the form.

- **2026-11-26** Phase 1 from Orest's request in the DPL thread "Fix DPL project node creation and
  repair": "make the design for the Liquers addition to Step 4 first" (orest-d/dpl design
  `project-node-stopgap`, Phase 2 step 4).

## Phase 1: High-Level Design

### Purpose

A host embedding `RecipesElement` decides what a row click does, in egui and on the web, without
wrapping or forking the panel. Hosts that set nothing see no change.

### Problem Example

DPL shows a project with the recipes editor over `projects/p/recipes.yaml` and a recipe workbench
beside it. The user clicks the row `1.node`.
**Today** a recipe window opens with Liquers' form; typing a query there shows nothing until it is
written, and the workbench stays on its old recipe.
**Wanted:** DPL built the panel with the open query `edit_node-{file}`, so the click submits
`edit_node-1.node` with the panel as the asking element; DPL's command points the workbench at
`1.node`, where the query is edited with Live preview. Right-click → Edit on the same row still
opens the form, for the title, the description and a rename.

Checked with `liquers-validate --command edit_node`: `edit_node-1.node` (one argument, `1.node`).

### Scope and Acceptance Criteria

- **AC-1** No open query, no change
  WHEN a `RecipesElement` has no open query (the default, and `lui/recipes_editor`)
  THEN a row click and the menu's Edit open the recipe window exactly as today (`recipes-editor`
  AC-4).
- **AC-2** A click submits the open query
  WHEN the open query is set to a template and a row for file `f` is clicked, in egui or on the web
  THEN `{file}` in the template is replaced by `f` (and `{folder}` by the panel's folder key) and
  the result is submitted with the panel as the current element (`RecipesEffect::Submit`, as View
  does); no recipe window opens.
- **AC-3** Edit still opens the form
  WHEN the open query is set and Edit is chosen in a row's menu (egui context menu; the web `⋯`
  menu, whose Edit now sends `rc:edit:<file>` instead of `rc:open:<file>`)
  THEN the recipe window opens, as today.
- **AC-4** The clicked row is marked
  WHEN a row was clicked with an open query set
  THEN that row is drawn selected until another row is clicked (today a row is marked when its
  window is open, which never happens on this path).
- **AC-5** Saved with the element
  WHEN a `RecipesElement` with an open query is serialized and loaded again (a saved UI)
  THEN it keeps the open query; an element saved before this change loads with none.
- **AC-6** A bad template is refused when set
  WHEN the template does not contain `{file}`, or does not parse as a query once `{file}` and
  `{folder}` are replaced by sample names
  THEN setting it fails with the parse error, and nothing changes.

Out of scope: setting the open query from a query (`lui/recipes_editor` arguments; open question
1); a preview inside the recipe form (filed by DPL as `LIQUERS-RECIPES-FORM-HAS-NO-LIVE-PREVIEW`);
changing what View, Evaluate or the dependencies view's toggle do.

### Language Integration

Not a new feature in the guide's sense: an option of an existing Rust UI element.
`LANGUAGE-INTEGRATION_GUIDE.md` unchanged. Python (`liquers-py`) and JavaScript (`liquers-web`):
**not needed**, neither binds the recipes editor.

### Documentation Intent

- Reference: `reference/UI_INTERFACE_FSD.md` §Recipe editors (the open query, `rc:edit`, the
  selected row).
- API docs: `liquers-lib/src/ui/recipes/mod.rs` module doc (the option, with an executed example).
- User guides: none; the recipes editor is not in the user guides.
- Designs: a scope note in `recipes-editor` (AC-4) pointing here.

### Open Questions

1. **Setting it from a query.** Rust only (`RecipesElement::with_open_query`, as the editor's
   `with_follow`), or also an argument `lui/recipes_editor-<open>`? A template holds `-` and `/`,
   which must be escaped in a query argument. Recommended: **Rust only** now; a UISpec or query
   form later with `EDITOR-FOLLOW`-style settings (DPL filed
   `LIQUERS-EDITOR-FOLLOW-NOT-SETTABLE-FROM-A-QUERY` for the same gap).
2. **Placeholders.** `{file}` and `{folder}` (recommended), or the full key only (`{key}`)?
3. **Double click.** Should a double click still open the form when an open query is set? Recommended:
   no; Edit in the menu is enough, and egui and the web would differ.

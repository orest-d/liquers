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
result) cannot be reached from the click. This design lets the host name an *open command*, a
namespace and a command name given to `lui/recipes_editor`: a click runs that command on the
clicked node, and the row menu's Edit still opens the form.

- **2026-11-26** Phase 1 from Orest's request in the DPL thread "Fix DPL project node creation and
  repair": "make the design for the Liquers addition to Step 4 first" (orest-d/dpl design
  `project-node-stopgap`, Phase 2 step 4).
- **2026-11-26 21:51** Orest: pass the namespace and the command name as arguments of the recipes
  editor; the command takes the node as a key state (`-R-key/folder/node.ext/-/ns-x/cmd`) or as a
  node name argument (`-R-key/folder/-/ns-x/cmd-node`), the second needing no extension. The open
  query with placeholders is replaced by this (open questions 1 and 2 answered).

## Phase 1: High-Level Design

### Purpose

A host embedding `RecipesElement` decides what a row click does, in egui and on the web, without
wrapping or forking the panel. Hosts that set nothing see no change.

### Problem Example

DPL shows a project with the recipes editor over `projects/p/recipes.yaml` and a recipe workbench
beside it. The user clicks the row `1.node`.
**Today** a recipe window opens with Liquers' form; typing a query there shows nothing until it is
written, and the workbench stays on its old recipe.
**Wanted:** DPL builds the panel with
`-R-key/projects/p/recipes.yaml/-/ns-acc/accessor/ns-lui/recipes_editor-dpl-edit_node`, so the
click submits `-R-key/projects/p/1.node/-/ns-dpl/edit_node` with the panel as the asking element;
DPL's command points the workbench at `1.node`, where the query is edited with Live preview. The
same command typed as `-R-key/projects/p/-/ns-dpl/edit_node-1` finds the node by its name. Right-click → Edit on the same row still
opens the form, for the title, the description and a rename.

Checked with `liquers-validate --command edit_node`: `-R-key/projects/p/1.node/-/ns-dpl/edit_node`
and `-R-key/projects/p/-/ns-dpl/edit_node-1` (a `-R-cmd` header does not exist; `-R-key` on the
folder gives the folder's key).

### Scope and Acceptance Criteria

- **AC-1** No open command, no change
  WHEN a `RecipesElement` has no open command (the default, `lui/recipes_editor` with no
  arguments)
  THEN a row click and the menu's Edit open the recipe window exactly as today (`recipes-editor`
  AC-4).
- **AC-2** The open command is given to the recipes editor
  WHEN `lui/recipes_editor-<ns>-<command>` is evaluated (or `RecipesElement::with_open_command(ns,
  command)` is called from Rust)
  THEN the element keeps both; empty arguments mean no open command.
- **AC-7** A click runs the open command on the node's key
  WHEN an open command is set and the row of file `f` in folder `d` is clicked, in egui or on the
  web
  THEN `-R-key/<d>/<f>/-/ns-<ns>/<command>` is submitted with the panel as the current element
  (`RecipesEffect::Submit`, as View does); no recipe window opens.
- **AC-8** The two call forms (a convention for open commands, documented, not enforced)
  WHEN an open command is written for the recipes editor
  THEN it takes the node either as its input state, a key (`-R-key/<d>/<f>/-/…`), or as a node-name
  argument on the folder's key (`-R-key/<d>/-/…/<command>-<name>`), where it resolves the name to
  the node's file itself (no extension needed); Liquers offers the resolution as a helper. The
  command decides by the name: empty, the input key is the node; non-empty, the input key is the
  folder. (Orest, 21:57: a `-R-cwd` variant was considered and dropped, since it would need the
  cwd as a further argument.)
- **AC-3** Edit still opens the form
  WHEN an open command is set and Edit is chosen in a row's menu (egui context menu; the web `⋯`
  menu, whose Edit now sends `rc:edit:<file>` instead of `rc:open:<file>`)
  THEN the recipe window opens, as today.
- **AC-4** The clicked row is marked
  WHEN a row was clicked with an open command set
  THEN that row is drawn selected until another row is clicked (today a row is marked when its
  window is open, which never happens on this path).
- **AC-5** Saved with the element
  WHEN a `RecipesElement` with an open command is serialized and loaded again (a saved UI)
  THEN it keeps it; an element saved before this change loads with none.
- **AC-6** An unknown command is refused when set
  WHEN `lui/recipes_editor-<ns>-<command>` names a command the registry does not have in `<ns>`,
  or only one of the two is given
  THEN the command fails with that reason, and no element is made.

Out of scope: a preview inside the recipe form (filed by DPL as `LIQUERS-RECIPES-FORM-HAS-NO-LIVE-PREVIEW`);
changing what View, Evaluate or the dependencies view's toggle do.

### Language Integration

Not a new feature in the guide's sense: an option of an existing Rust UI element.
`LANGUAGE-INTEGRATION_GUIDE.md` unchanged. Python (`liquers-py`) and JavaScript (`liquers-web`):
**not needed**, neither binds the recipes editor.

### Documentation Intent

- Reference: `reference/UI_INTERFACE_FSD.md` §Recipe editors (the open command and its two call forms, `rc:edit`, the
  selected row).
- API docs: `liquers-lib/src/ui/recipes/mod.rs` module doc (the option, with an executed example).
- User guides: none; the recipes editor is not in the user guides.
- Designs: a scope note in `recipes-editor` (AC-4) pointing here.

### Open Questions

1. **Answered (Orest, 21:51):** set from the recipes editor's arguments (namespace and command),
   and from Rust.
2. **Answered:** no placeholders; the node reaches the command as its key or a name argument.
3. **Double click.** Should a double click still open the form when an open command is set?
   Recommended: no; Edit in the menu is enough, and egui and the web would differ.
4. **Which form the click uses.** The key form (AC-7, recommended: the editor knows the file, and
   it needs no name resolution) or the name form?
5. **The name-resolution helper (AC-8).** A public `liquers_core::recipe_tree` function
   `resolve_node(list, name) -> Result<String, Error>` (name or filename → filename, as
   `TreeEditor::resolve` does today), recommended, so every open command resolves names the same
   way; or leave it to each command.

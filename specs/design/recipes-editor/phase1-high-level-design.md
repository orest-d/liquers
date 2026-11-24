# Phase 1: High-Level Design — Recipe and recipes editors

## Purpose

Let a person build and maintain a folder's recipes in the GUI instead of in a text editor: one
recipe as a form with the query editor inside, a folder's `recipes.yaml` as an outline of its
recipes, a recipe tree (`tree:` section, `specs/reference/RECIPE_TREES.md`) as an outline of its
nodes, and the whole file as highlighted YAML. This is the editing half of "recipes as the source
of truth" (`specs/reference/VISION.md`): the recipe-tree design made a folder checkable and
editable in code (`liquers_core::recipe_tree::TreeEditor`); nothing in the UI uses it beyond
renaming a Node argument's node.

## Problem Example

`scenes/car/recipes.yaml` is a recipe tree:

```yaml
tree: {result: car.glb}
recipes:
  - {query: body.glb, title: Body}
  - {query: tyre.glb, title: Tyre}
  - {query: "-R/./tyre.glb/-/ns-mesh/rim-17/wheel.glb", title: Wheel}
  - {query: "ns-mesh/assemble-body-wheel/car.glb", title: Car}
```

**Today** the GUI can edit one recipe's *query*
(`lui/query_editor` on `-R-key/scenes/car/recipes.yaml/-/ns-acc/accessor/recipe-2/query`). Adding a
fifth recipe, changing a title, renaming `wheel` to `rim` across the folder, or seeing which node
feeds which, means editing the YAML by hand. The accessor addresses a recipe by its *position*, so
an editor of `recipe-2` follows a different recipe after the list is reordered.

**Once this exists** `-R-key/scenes/car/recipes.yaml/-/ns-acc/accessor/ns-lui/recipes_editor`
is a narrow **side panel** (a navigator, not a file browser):

```
 car/   ≡ ⑂ { } ＋
 🫖 body     ●
 🫖 tyre     ●
 🫖 wheel   42%
 🫖 car    ★ ⚠
```

Hovering `wheel` shows its title and description; hovering `42%` shows the progress message;
hovering `⚠` shows the finding. Clicking `wheel` opens a window with its recipe editor (title,
description, filename, the query editor, overrides, options); right-clicking opens a menu (View,
Metadata, Edit, Evaluate, Rename, Duplicate, Delete, Save as, Export). Renaming `wheel` to `rim`
rewrites `car`'s reference and moves stored data. `⑂` switches to the *Dependencies* view
(`car ▸ body, wheel ▸ tyre`); `{ }` opens the whole file as highlighted YAML in its own window.

## Recommendation: the recipe list stays a document, not a new `Value` variant

The maintainer asked whether a recipe list (with its tree) should be a `Value` like `Recipe`, or
travel as YAML text over an accessor. **Recommended: keep it a document** — the accessor carries
the `recipes.yaml` text (or bytes) as it does today, and both ends parse it into the existing typed
model (`RecipeList`, `TreeEditor`). This is not a workaround: it is how the key accessor and
`RecipeStep` already work.

| | `Value::RecipeList` (new variant) | Document text (recommended) |
|---|---|---|
| Source of truth | a second representation of a file the provider reads as bytes; needs a conversion step on read and write | the file itself, unchanged |
| Ripple | core `Value` + every exhaustive match: ~25 files incl. `liquers-py`, `liquers-web`, `liquers-mcp`, serialization, a `TypeInfo`, a feature ID in the integration guide | none outside the new step and the editors |
| Comments and layout | cannot ever be kept (a typed value has no comments) | can be, once `YAML-DOCUMENT-EDIT-DROPS-COMMENTS` is done; the YAML view keeps what was typed |
| Invalid file | cannot be represented; the editor could not show or repair it | shown in the YAML view with the parse error, repairable |
| Typed access | yes, directly | yes, via the parse at each end (cheap: a folder is small) |
| Value dispatch (open a value in its natural editor) | by identifier | by filename `recipes.yaml` or by the explicit `lui` command |

`Value::Recipe` remains right for a *part*: a single recipe has no file of its own, and a
projection must return something typed. **Revisit** when a command must *produce or consume* a
recipe list as data (generating recipes, a tree-analysis view): adding the variant then changes no
editor, because the editors accept either text or such a value.

## Scope and Acceptance Criteria

- **AC-1** Outline of a recipes list
  WHEN `lui/recipes_editor` is given an accessor of a folder's `recipes.yaml` without `tree:`
  THEN it is a narrow side panel listing one row per recipe in file order: the type icon, the
  label (AC-16) and the status (AC-17).
- **AC-2** Icon by asset type
  WHEN a row is drawn
  THEN its icon is the asset's `unicode_icon` from its metadata if the asset manager has it, else
  the icon of the filename's extension (`Recipe::unicode_icon`); a recipe without a filename gets
  the default icon.
- **AC-3** Outline of a recipe tree
  WHEN the document has a `tree:` section
  THEN rows label a recipe by its node name (filename without extension) instead of its filename,
  the type icon standing for the extension; the result is marked, and stored files the folder declares as sources are rows too.
- **AC-4** Open a recipe in a window
  WHEN a row is clicked
  THEN a window opens with that recipe's editor; clicking it again focuses the same window, and
  several windows may be open at once.
- **AC-5** Recipe editor
  WHEN a recipe editor is open
  THEN it edits title, description, filename, the query (the existing query editor), argument and
  link overrides, `cwd`, `volatile`, `expires` and `stored`; rarely used fields are collapsed.
- **AC-6** Recipe addressed by filename
  WHEN an accessor of a recipe is built with
  `-R-key/scenes/car/recipes.yaml/-/ns-acc/accessor/recipe_file-wheel.glb`
  THEN it reads and writes that recipe wherever it sits in the list, and a recipe editor bound to it
  keeps following it when other recipes are added, removed or reordered.
- **AC-7** New recipe
  WHEN *New* is chosen and a title and file type (or filename) are given
  THEN a recipe is appended with a suggested, unused filename (`TreeEditor::suggest_filename`), and
  its window opens; in a tree, *Add child* on a node also names the new node in a chosen argument
  of the parent (`TreeEditor::add_child`).
- **AC-8** Rename
  WHEN a row is renamed (a filename in a list, a node name in a tree, keeping the extension)
  THEN the recipe is renamed, every reference in the folder is rewritten and stored data is moved
  (`TreeEditor::rename`); an open window of it follows the new name; a name in use is refused and
  nothing changes.
- **AC-9** Delete and duplicate
  WHEN a row is deleted or duplicated
  THEN delete is refused while another node references it (naming the parents), and duplicate
  appends a copy under a new unused name; stored data is never deleted.
- **AC-10** Findings at their rows
  WHEN the folder's check (`check_tree`) reports errors or warnings
  THEN each is shown at the row it concerns, and the outline header shows the counts.
- **AC-11** Dependencies view (optional)
  WHEN the Dependencies view is chosen
  THEN the top level lists the nodes nothing in the folder depends on (or the declared roots and
  result), each node's direct dependencies are its children, a node with several parents appears
  under each, and a cycle is shown once and not expanded further. The default view stays the
  outline in file order.
- **AC-12** YAML window (revised 2026-11-24)
  WHEN the YAML button of the panel is pressed
  THEN a separate window shows the whole document as editable YAML with syntax highlighting (queries inside
  `query:` highlighted as queries); typed text is applied only when it parses as a recipe list,
  otherwise the parse error is shown at its line and nothing is applied.
- **AC-13** Update modes apply
  WHEN any of these editors is in a mode of `specs/design/query-update-modes/`
  THEN edits follow that mode: in *On demand* the recipe windows and the outline edit one working
  copy that is written by one ✓; in *Apply immediately* each edit is written and every other view
  of the file follows.
- **AC-14** Refused edits change nothing
  WHEN an edit would add a check error (a cycle, a dangling reference, a strict-mode violation)
  THEN it is refused with the finding, as `TreeEditor` already does, and the document is unchanged.
- **AC-15** Concurrent change
  WHEN the file changes elsewhere while an editor has unwritten edits
  THEN the editor reloads or reports a conflict as the query editor does today (`set_if`), and
  never overwrites the other change silently.

- **AC-16** Configurable label
  WHEN the panel's `label` setting is `name`, `title` or `both` (default `name`)
  THEN rows show the filename (node name in a tree), the title, or both; a recipe without a title
  falls back to its name.
- **AC-17** Status, progress and findings
  WHEN a row's asset has a status in the asset manager
  THEN it is a small coloured circle or icon whose tooltip names the status; while the asset
  reports progress with a total, a tiny percentage (`42%`) replaces it, its tooltip giving the
  progress message; a check finding or an asset error adds a `⚠`/`✕` whose tooltip gives the
  message.
- **AC-18** Tooltips
  WHEN the pointer rests on a row's name
  THEN a tooltip shows the title and description; on the title, the description.
- **AC-19** Context menu
  WHEN a row is right-clicked (egui; the browser renderer too)
  THEN a menu offers, each disabled with a reason when it does not apply:
  **View** (the asset's value, in a window), **Metadata** (its metadata, in a window),
  **Edit** (the recipe window, as a click does), **Evaluate** (evaluate or re-evaluate the asset;
  progress shows in the row), **Rename**, **Duplicate**, **Delete**, **Save as** (write the value
  to a file on the person's computer, not to the store), **Export** (a submenu of the data
  formats the value's type can be written in, `TypeInfo::supported_data_formats`; each saves a
  file like Save as); in a tree also **Add child** and **Set as result**.

Non-goals: reordering by drag (later); comment-preserving writes of structured edits
(`YAML-DOCUMENT-EDIT-DROPS-COMMENTS`); editing several folders in one editor; subfolder nodes
(recipe trees are flat); the portable archive (`RECIPE-TREE-CANNOT-BE-EXPORTED-AS-AN-ARCHIVE`); a
new `Value` variant (above).

## Core Interactions

- **Accessors:** a filename-keyed recipe step (`acc/recipe_file`) beside the index step; the list
  editor's working copy is a memory accessor whose recipe projections it hands to its windows.
- **Recipe trees:** `TreeEditor` for every structural edit, `TreeGraph` for the dependencies view,
  `check_tree` for findings; a plain "append recipe" edit is added to `TreeEditor`.
- **UI:** two new elements and `lui` commands, egui and browser renderers; windows owned by the
  recipes editor; the existing query editor embedded in the recipe editor.
- **Assets:** asset-manager listing and metadata for icons, status and source files.

## Crate Placement

`liquers-core`: the filename step (`accessor/steps.rs`) and the new `TreeEditor` edits
(`recipe_tree.rs`). `liquers-lib`: the `acc/recipe_file` command (`accessor/`), the editor state
and elements (`ui/editor/` or a sibling `ui/recipes/`), the `lui` commands, the egui renderer
(`egui/`) and the browser renderer (`ui/web/`).

## Language Integration

- Integration guide: no new feature ID; the accessor step is a command, reachable from every
  binding by query. The recipe-tree editing API stays tracked by `RECIPE-TREES-NOT-IN-LANGUAGE-BINDINGS`.
- Python (`liquers-py`): not needed — the elements are Rust UI elements that a Python-built UI
  already gets through `lui` queries; `Accessor` wrappers reach `recipe_file` by query.
- JavaScript (`liquers-web`): browser renderer in scope (as for the query editor: structure and
  theme; egui is the main target); no new JavaScript API.

## Documentation Intent

- Reference: extend `specs/reference/UI_INTERFACE_FSD.md` §Editors (recipe and recipes editors),
  `specs/reference/RECIPE_TREES.md` §In the editors, `specs/reference/VALUE_ACCESSORS.md` for `recipe_file`.
- Guide: none new.
- Documents to update: `specs/reference/UI_INTERFACE_FSD.md`, `specs/reference/RECIPE_TREES.md`,
  `specs/reference/VALUE_ACCESSORS.md`.
- API docs: `liquers-core/src/accessor/steps.rs`, `liquers-core/src/recipe_tree.rs`,
  `liquers-lib/src/accessor/mod.rs`, `liquers-lib/src/ui/editor/mod.rs` and the new module.
- User guides: none in this design — the editors are not described there yet
  (`USER-GUIDES-DO-NOT-DESCRIBE-THE-EDITORS` covers them together).

## Open Questions

1. **Value or document.** Accept the recommendation above (document text; `Value::RecipeList`
   only when a command needs it)? *Recommended: yes.*
2. **Who owns the writes.** Recipe windows bind to accessors the recipes editor provides (its
   working copy, keyed by filename), so one ✓ writes everything in On demand mode and renames
   re-bind the windows. In Apply immediately those accessors write straight through to the store
   accessor. *Recommended: yes.* The alternative — each window bound to the store accessor directly
   — is addressable after a restart but cannot hold a multi-window pending edit.
3. **One element or two.** One `lui/recipes_editor` panel with views *Outline* (list or nodes,
   chosen by the `tree:` section) and *Dependencies*, a YAML window, plus `lui/recipe_editor`.
   *Recommended: yes.* A separate tree element would duplicate everything but the row label.
4. **Window form.** egui: floating `egui::Window`s (recipe, YAML, View, Metadata) owned by the
   panel; browser: dialog panels. *Recommended: floating windows; a docked main area later.*
5. **YAML highlighting.** A small built-in YAML highlighter with theme roles and the existing
   query highlighter for `query:` values, instead of pulling in `syntect`. *Recommended: built-in.*
6. **Dependencies view on a plain list.** Offer it for a list without `tree:` too (the graph is
   built for any list, `check_tree` uses defaults). *Recommended: yes.*
7. **Icons.** Unicode from metadata, else extension, as asked; the theme's icon variants
   (`specs/reference/THEME.md`) can replace them later. *Recommended: as asked.*
8. **Save as and Export on the desktop.** egui has no file dialog; add the `rfd` crate (native
   only, behind `egui`) or ask for a path in a small window? Browser: a download.
   *Recommended: `rfd` on native, download in the browser.*
9. **Export formats.** Offer only formats the value's type declares (no conversions such as
   DataFrame to xlsx unless the type lists them); formats needing a conversion command come later.
   *Recommended: yes.*
10. **Click action.** A click opens Edit (the recipe window), double-click opens View.
    *Recommended: yes, as asked; configurable later.*

## References

- Source issue `NO-RECIPE-OR-RECIPES-EDITOR-UI` (filed in PR #285, folded in here).
- `specs/design/recipe-tree/` and `specs/reference/RECIPE_TREES.md` — `TreeEditor`, `TreeGraph`.
- `specs/design/query-pipeline-editor/`, `specs/design/query-editor-ergonomics/` — editors,
  bindings, theme and icons.
- `specs/design/query-update-modes/` (PR #285) — the update modes of AC-13.
- Issues: `YAML-DOCUMENT-EDIT-DROPS-COMMENTS`, `RECIPE-TREES-NOT-IN-LANGUAGE-BINDINGS`,
  `USER-GUIDES-DO-NOT-DESCRIBE-THE-EDITORS`.
- Mockups: [`mockups/`](mockups/).

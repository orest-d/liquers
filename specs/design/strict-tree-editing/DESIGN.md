---
id: STRICT-TREE-EDITING
kind: design
title: An invalid recipe tree downgrades to plain recipes instead of being refused, and a new node is an empty .node recipe
form: compact
workflow: liquers-project
status: in_review
phase: high-level
area: [core/assets, lib/ui, lib/egui]
issues: [STRICT-TREE-FINDING-LOCKS-ITS-RECIPES-FILE, RECIPES-EDITOR-NEW-RECIPE-BREAKS-A-STRICT-TREE]
affects_docs: [reference/RECIPE_TREES.md, reference/UI_INTERFACE_FSD.md]
created: 2026-11-26
---
# Invalid recipe trees downgrade instead of being refused

Two merged designs disagree. `recipe-tree` (AC-24) makes a strict tree "valid as a whole or not
used": `RecipesProvider` refuses the folder on any error finding. `recipes-editor` (AC-7) lets ＋
write a new recipe as a source declaration, `{title, query: <file>}`, which is a `MissingSource`
error until its query is written. Together they mean that the first node made with ＋ in a strict
folder locks the folder's own `recipes.yaml`. Writes then fail silently, and after a restart the
editor is empty and read-only. Found by DPL, whose projects are strict trees (orest-d/dpl thread
"Test egui project save and reopen workflow"). DPL's own stopgap is a separate design there.

- **2026-11-26** Phase 1 from Orest's request to fix it upstream, in egui and the web UI.
- **2026-11-26 18:08** Revised with Orest's direction. The tree check no longer refuses
  anything: an invalid tree is downgraded to plain (non-tree) recipes and the editor shows it. A
  `TreeEditor` always holds a valid tree. A new node is an empty query named `<n>.node`, and
  `node` is a universal extension. Errors propagate through dependencies as for any recipe. The
  former AC-3 (evaluation stays strict) is withdrawn and replaced by AC-8 to AC-10.
- **2026-11-26 18:32** Orest's answers to the four questions, now decisions (AC-14 to AC-16, AC-11
  revised). An empty `.node` node evaluates to `None`. A filename set in the form or in the action
  editor renames the node, made unique. Duplicate filenames are illegal in plain recipes too. A
  tree with a valid recipe always has a result. Strictness is a promise, not a gate, for now: GUI
  editing through `TreeEditor` keeps trees valid, while a hand-edited or plain-edited file is never
  invalidated as a whole and stays salvageable, so an expert can knowingly use plain recipes.
- **2026-11-26 18:50** Orest on `roots`: a display aid, optional. Any node may be listed, and a
  node not listed can still be a root, because the tree logic finds all roots itself (a
  `RecipeList` method). Invalid entries are removed, and an empty list may be dropped (AC-17). The
  tree/DAG vocabulary is to be clarified later (`RECIPE-TREE-VOCABULARY-SAYS-TREE-FOR-A-DAG`).

## Phase 1: High-Level Design

### Purpose

The tree check *describes* a folder; it never blocks it. A folder whose tree is invalid still works
as ordinary recipes, and the editor says plainly that it is not a valid tree. Only what needs a
valid tree, the portable archive, refuses it. The editor always starts from a valid tree and adds
nodes that keep it valid, so the common path never leaves the tree.

### Problem Example

A folder `p/` holds `recipes.yaml` = `tree: {dependencies: strict}`, `recipes: []`. In the recipes
panel (egui or web) the user presses ＋, then types `net/wave-z-x-0.1-6.28/wave.obj` in the new
recipe's form.
**Today** ＋ stores `{title: wave, query: wave.obj}`, a missing source, and the editor gets
`Failed(Recipe tree: source 'wave.obj' is declared but not stored)`. The query edit is never
stored. Reopened, the panel is empty and read-only, and the YAML view is empty.
**Wanted:** ＋ stores `{query: 1.node}`, a valid tree. The query edit renames it to `wave.obj`
and is stored. Had the tree been invalid at any point, the file would still be written, the panel
would show its rows under a "not a valid tree" banner, and every recipe would still evaluate as a
plain recipe.

### Scope and Acceptance Criteria

- **AC-1** An invalid tree stays readable
  WHEN `p/recipes.yaml` declares a tree and `check_tree` reports an error
  THEN `p/recipes.yaml` (asset, accessor, `-R-key/p/recipes.yaml`) still reads, and the panel lists
  its recipes.
- **AC-2** And writable
  WHEN the panel writes `p/recipes.yaml` while the tree, before or after, has an error finding
  THEN the write succeeds and the editor's fingerprint follows it.
- **AC-4** A failed write is visible
  WHEN a write of the recipes fails for any reason
  THEN the panel marks the edits unsaved and shows the error. A write that reached the store is
  not reported as failed.
- **AC-5** New node is an empty `.node` recipe
  WHEN ＋ is used (or `TreeEditor` adds an empty node)
  THEN the recipe is `{query: <n>.node}`, where `<n>` is the first integer from 1 that is no node's
  name. It is not a source declaration, it keeps a valid tree valid, and the dialog asks for no
  extension.
- **AC-6** Toolbar glyphs
  WHEN the egui panel draws Outline, Dependencies and New recipe
  THEN it uses Phosphor glyphs, as the rest of the editor does, not ≡ ⑂ ＋.
- **AC-7** Web parity
  WHEN AC-1, AC-2, AC-4, AC-5 and AC-9 are exercised in the web panel (`rc:new`)
  THEN the results are the same, checked by the ui_spec_demo Playwright suite.
- **AC-8** A downgraded tree evaluates as plain recipes
  WHEN `check_tree` reports an error for `p/`
  THEN `RecipesProvider` does not refuse `p/`: each recipe is served as an ordinary recipe (tree
  and strict rules not applied), and the report is available to the editor and to the tools.
- **AC-9** The editor shows the downgrade
  WHEN the panel shows a folder whose tree is invalid
  THEN a banner says "Not a valid recipe tree: plain recipes" with the findings, and the affected
  nodes are marked.
- **AC-10** Errors propagate as usual
  WHEN a node depends on a node whose evaluation fails, or a recipe cannot be planned (a query
  that does not parse, an unknown command) but has a recognisable filename or node name
  THEN its asset is an error asset under that key, as for any failing recipe. A recipe with
  neither is reported in the findings only.
- **AC-11** `TreeEditor` holds a valid tree
  WHEN `TreeEditor::new` gets no recipes, or recipes with no `tree` section
  THEN it starts with an empty valid tree, or adds `tree: {}`, which changes no recipe's meaning.
  It also drops a `result` or `roots` entry that names no node, and sets the result when AC-16
  requires one. Anything else is left as it is and reported (AC-9).
- **AC-12** `node` is a universal extension
  WHEN a Node argument of any `ext:` names a node stored as `<n>.node`
  THEN it is accepted. A `.node` file is written and read by its metadata (type and data format),
  and reading one with no metadata fails with an error that says so.
- **AC-13** No archive of plain recipes
  WHEN a folder whose tree is invalid is exported as a portable archive
  THEN the export is refused with the findings (recorded on `RECIPE-TREE-CANNOT-BE-EXPORTED-AS-AN-ARCHIVE`,
  which is not implemented yet).

- **AC-14** Setting a filename renames the node, uniquely
  WHEN a filename is given for a node in the recipe form, or through the action editor's Filename
  (for example a query edit ending in `net.obj`, or the first command's declared `filename`)
  THEN the node is renamed through `TreeEditor::rename`, which rewrites every reference to it (Node
  arguments, `-R/./<file>` segments and links, overrides, the tree declaration). When `net.obj` is taken, the name becomes
  the first free one of `net1.obj`, `net2.obj`, and so on. A name that is only an integer takes the
  first free integer. Names compare by node name, so `net.obj` and `net.png` collide.
- **AC-15** Duplicate filenames are illegal in plain recipes too
  WHEN the recipes editor (tree or not) would give two recipes the same filename
  THEN the AC-14 rule picks a unique one, and a file that already has duplicates is reported.
- **AC-16** A tree with a valid recipe has a result
  WHEN a tree has at least one recipe with a filename and no `result` naming a node
  THEN `TreeEditor` sets `result` to the first root, else the first such recipe. The result may be
  an error asset (AC-10).

- **AC-17** Roots are optional display hints
  WHEN `tree.roots` lists nodes, names something that is not a node, is empty, or sits beside a
  `result`
  THEN the listed nodes are shown as roots in addition to the roots the tree logic finds
  (`RecipeList::roots()`: the declared ones, the result, and every node nothing depends on). An
  entry that is not a node is removed, an empty list is dropped, and listing roots beside a
  `result` is no longer an error (`RootsBesideResult`, `UnknownRoot` become repairs).

Out of scope: the archive itself; DPL's New Node and repair view (DPL's own design).

### Language Integration

The `node` extension is visible to every integration: a Node argument in Python or JavaScript
accepts `.node` too. `liquers-py` and `liquers-web` are **not needed** as code changes, because
both go through `RecipesProvider` and the core Node checks. The `LANGUAGE-INTEGRATION_GUIDE.md`
recipe-tree entry gets a sentence on `.node`.

### Documentation Intent

- Reference: `reference/RECIPE_TREES.md` (downgrade instead of refusal, empty nodes, `.node`, error
  propagation, archive) and `reference/UI_INTERFACE_FSD.md` §Recipe editors (banner, unsaved
  edits, New without extension).
- API docs: module docs of `liquers-core/src/recipes.rs`, `liquers-core/src/recipe_tree.rs`,
  `liquers-lib/src/ui/recipes/mod.rs`.
- User guides: none; the recipes editor is not in the user guides yet.
- Designs: scope notes in `recipe-tree` (AC-24) and `recipes-editor` (AC-7) pointing here.

### Decisions

Orest's answers of 2026-11-26 18:32:

1. An empty node is a filename-only query with extension `node`: not a source, it evaluates to
   `None` (AC-5).
2. Setting a filename renames the node, uniquely (AC-14, AC-15).
3. The safe repairs are adding `tree: {}`, dropping a `result` or `roots` entry that names no node
   (and an empty `roots`), and keeping a result (AC-11, AC-16, AC-17).
4. Strictness is a promise checked on save and on archive, not a gate on evaluation, for now. It
   may be tightened once the GUI editing works well.

### Open Questions

None. Phase 2 decides where the downgrade lives (`RecipesProvider`), how the editor gets the report,
and the metadata-based reading of `.node`.

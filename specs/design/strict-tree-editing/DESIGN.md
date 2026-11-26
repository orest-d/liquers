---
id: STRICT-TREE-EDITING
kind: design
title: An invalid recipe tree downgrades to plain recipes instead of being refused, and a new node is an empty .node recipe
form: compact
workflow: liquers-project
status: approved
phase: implementation
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
- **2026-11-26 18:51** `proceed all`: pre-approved after Phase 1; Phases 2 to 5 follow. AC-14 says
  explicitly that only a filename the user gives renames a node, as Orest's answer 2 said, and that
  a query edit without one keeps the node's filename.
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
  WHEN a filename is given for a node, in the form's Filename field, typed at the end of its query
  (`net/net.obj`) or set with the action editor's Filename
  THEN the node is renamed through `TreeEditor::rename`, which rewrites every reference to it (Node
  arguments, `-R/./<file>` segments and links, overrides, the tree declaration). When `net.obj` is
  taken, the name becomes the first free one of `net1.obj`, `net2.obj`, and so on. A name that is
  only an integer takes the first free integer. Names compare by node name, so `net.obj` and
  `net.png` collide. A query edit that gives no filename keeps the node's own filename.
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

### Design Readiness

**Readiness:** ready. **Decision log** (pre-approval, `proceed all`): no item blocks or needs a
decision. Assumptions taken, listed in Phase 2: a plain list stays plain (AC-11), `.node` names are
full filenames in Node arguments (AC-12).

## Phase 2: Architecture

### Solution

Everything that blocks moves out of the provider and into reports: the provider serves what it
can, `check_tree` describes, and `TreeEditor` keeps what it edits valid.

1. **Downgrade, not refusal (AC-1, AC-2, AC-8, AC-10).** `DefaultRecipeProvider::cached_recipes`
   (`liquers-core/src/recipes.rs`) stops returning the tree check's error. The folder's recipes are
   served as plain recipes. Nothing else at evaluation reads `tree` (checked:
   `recipes.rs`, `validate/mod.rs` only). The editor already runs `check_tree` itself
   (`RecipesState::rebuild`). The same function indexes a recipe whose query does not parse under
   the filename its text ends with (`…/name.ext`), so that key is an error asset with the parse
   error (AC-10).
2. **Empty nodes and `.node` (AC-5, AC-12).** `Recipe::is_source_declaration` is false for a
   filename-only query with extension `node`. Such a query evaluates to `None` (probed: the plan is
   empty, the state is `None`). The level-2 seeding of `data_format` from an extension
   (`MetadataRecord::with_filename`, `set_filename`, `set_extension`, `Metadata::with_filename`,
   `liquers-core/src/metadata.rs`) skips `node`, so the value is written in its type's default
   format. It is read back by its metadata, and a `.node` key with no metadata fails with an error
   naming the missing metadata. `NodeArgument::filename` (`command_metadata.rs`) takes a name that
   already ends in `.node` as the filename, so `attach-1.node` links `-R/./1.node` for any `ext:`.
   `TreeEditor::node_choices` offers `.node` nodes by their filename. Constant
   `metadata::NODE_EXTENSION = "node"`.
3. **`TreeEditor` keeps trees valid (AC-11, AC-16, AC-17).** A private `normalize` runs in
   `TreeEditor::new` and in every commit. It applies only when the list has a `tree:` section, or
   has no recipes (then it adds `tree: {}`). It drops a `result` or `roots` entry that names no
   node, drops an empty `roots`, and sets `result` to the first found root, else to the first
   recipe with a filename, when none is set. *Assumption:* a non-empty plain list stays plain. It
   is the "normal recipes editor" of Orest's answer 2 and gets only the uniqueness rule.
4. **Roots (AC-17).** New `RecipeList::roots(&self, cmr) -> Vec<String>`: the result, the declared
   roots that are nodes, and every recipe node nothing depends on, without duplicates.
   `check_tree` no longer reports `RootsBesideResult`, and reports `UnknownRoot` as a warning.
5. **Unique names (AC-5, AC-14, AC-15).** New `TreeEditor::unique_filename(&self, wanted:
   &str, except: Option<&str>) -> String` (stem, `stem1`, `stem2`…; an integer stem takes the first
   free integer from 1; compared by node name against nodes and the listing). New
   `TreeEditor::add_empty(&mut self, title: &str, extension: &str) -> Result<String, Error>` adds
   `{query: <n>.<ext>, title}` with `<n>` the first free integer, and is used by New
   (`extension` defaults to `node`). `rename` makes `to` unique instead of refusing a used name.
   `RecipesState` calls `unique_filename` before re-keying a form. `RecipeForm::apply` keeps the
   recipe's filename when an edited query has none.
6. **Editor (AC-4, AC-6, AC-9, AC-7).** `RecipesState` gains `tree_notice() -> Option<String>`:
   "Not a valid recipe tree: plain recipes" plus the first findings, when the list is a tree and the
   report has errors. `unsaved() -> bool` is true while `pending` holds with a write error. A
   `Failed` write keeps the edits pending (as today) and is shown as "Unsaved edits: <error>". The
   egui panel (`liquers-lib/src/egui/recipes.rs`) and the web panel (`ui/web/recipes.rs`) draw the
   notice, the unsaved marker and the New dialog without an extension. The egui toolbar uses
   Phosphor `list`, `tree-structure` and `plus`. `AddChild` and `Duplicate` use the unique names.

Rejected: requiring a query in New (slower, and Orest chose empty nodes); a per-node refusal
(Orest: plain recipes for now); resolving a bare `wave` to `wave.node` at plan time (the planner
has no folder listing).

Known issues: `RECIPE-TREE-CANNOT-BE-EXPORTED-AS-AN-ARCHIVE` (AC-13 is recorded on it, nothing to
build). `RECIPES-EDITOR-SAVE-AS-HAS-NO-FILE-DIALOG` is unrelated. None blocks.

### Changes

- `liquers-core/src/recipes.rs`: `cached_recipes`, `Recipe::is_source_declaration`,
  `RecipeList::roots`, module doc.
- `liquers-core/src/recipe_tree.rs`: `check_tree` (roots findings), `TreeEditor::{new, normalize,
  unique_filename, add_empty, rename, node_choices, suggest_filename}`, Rewriter's Node names for
  `.node`, module doc.
- `liquers-core/src/command_metadata.rs`: `NodeArgument::{filename, name_of}`.
- `liquers-core/src/metadata.rs`: `NODE_EXTENSION`, level-2 seeding.
- `liquers-core/src/assets.rs`: the error for a `.node` key without metadata.
- `liquers-lib/src/ui/recipes/{state,form,element}.rs`, `egui/recipes.rs`, `ui/web/recipes.rs`.
- No command changes, so `specs/command_registry.yaml` is untouched. All changes are synchronous
  except the provider, which is already async.

### Risks

Tests that pin the refusal (`strict_folder_fails_as_a_whole`) and `RootsBesideResult` change on
purpose. `.node` data written before this change does not exist. The level-2 change affects only the
`node` extension. Certainty: high for core, medium for the UI texts (reviewed by screenshot).

## Phase 3: Examples and Tests

### Examples

`p/recipes.yaml` = `tree: {dependencies: strict}` + `recipes: []`. ＋ gives `{query: 1.node}`, which
is valid, with result `1.node`. Editing its query to `net/wave-z-x-0.1-6.28` gives
`net/wave-z-x-0.1-6.28/1.node`. Setting the filename `wave.obj` renames it and its references, and
the result follows. A hand-written `{query: "-R/../x/y.obj/-/attach-a/bad.glb"}` in that strict file
gives a banner, and `-R/p/bad.glb` still evaluates as a plain recipe.

### Tests

`liquers-core/tests/recipe_tree.rs`:
- `invalid_strict_folder_downgrades_to_plain_recipes` (replaces `strict_folder_fails_as_a_whole`):
  AC-1, AC-8 (the withdrawn third scenario no longer applies)
- `recipes_yaml_of_an_invalid_tree_is_readable_and_writable`: AC-1, AC-2
- `unplannable_recipe_with_filename_is_an_error_asset`: AC-10
- `dependency_on_an_error_node_is_an_error`: AC-10
- `empty_node_evaluates_to_none_and_is_no_source`: AC-5
- `node_file_round_trips_by_metadata`, `node_file_without_metadata_fails_clearly`: AC-12
- `node_argument_accepts_a_node_file`: AC-12
- `tree_editor_normalizes_new_and_sectionless_lists`, `plain_list_stays_plain`: AC-11
- `tree_editor_keeps_a_result`: AC-16
- `roots_are_display_hints`: AC-17
- `add_empty_takes_the_first_free_integer`: AC-5
- `rename_makes_the_name_unique`: AC-14, AC-15

`liquers-lib/src/ui/recipes/tests.rs`:
- `new_adds_an_empty_node_and_writes_a_valid_tree`: AC-5, AC-2
- `query_edit_keeps_the_node_filename`, `filename_edit_renames_uniquely`: AC-14
- `plain_recipes_refuse_duplicate_filenames`: AC-15
- `invalid_tree_shows_a_notice`: AC-9
- `failed_write_marks_unsaved`: AC-4
- `web_panel_new_has_no_extension_field`, `web_panel_shows_the_notice`: AC-7, AC-9
- `egui_toolbar_uses_phosphor_glyphs` (glyph strings): AC-6

AC-13 is recorded on the archive issue, so it has no test. Run: `cargo test -p liquers-core --test
recipe_tree`, `cargo test -p liquers-lib --lib ui::recipes`, plus DPL's workflow under Xvfb.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `metadata.rs`, `recipes.rs` `is_source_declaration`, `command_metadata.rs` — `.node`
  (AC-5, AC-12) — `cargo test -p liquers-core --test recipe_tree`
- [ ] 2. `recipes.rs` `cached_recipes`, `assets.rs` — downgrade, unplannable recipes, `.node`
  read error (AC-1, AC-2, AC-8, AC-10, AC-12) — same
- [ ] 3. `recipe_tree.rs` — `normalize`, `roots`, `unique_filename`, `add_empty`, `rename`,
  findings (AC-11, AC-14 to AC-17) — `cargo test -p liquers-core --lib recipe_tree`, same
- [ ] 4. `liquers-lib` recipes state, form and element — New, unique names, notice, unsaved
  (AC-4, AC-5, AC-9, AC-14, AC-15) — `cargo test -p liquers-lib --lib ui::recipes`
- [ ] 5. egui and web panels — glyphs, dialog, notice (AC-6, AC-7, AC-9) — same, plus a
  screenshot through DPL
- [ ] 6. Specs: `RECIPE_TREES.md`, `UI_INTERFACE_FSD.md` §Recipe editors, issue statuses, scope
  notes in `recipe-tree` and `recipes-editor`, module docs (`check_api_docs.py`)

### Validation

`cargo test -p liquers-core`, `cargo test -p liquers-lib --lib --tests`,
`python3 scripts/check_api_docs.py` on the touched files, `python3 scripts/docs_index.py --check`.
CI cannot run (Actions limit), so the local runs are the record.


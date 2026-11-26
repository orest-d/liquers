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
  Other repairs that keep the meaning (to be listed in Phase 2) are applied; anything else is left
  as it is and reported (AC-9).
- **AC-12** `node` is a universal extension
  WHEN a Node argument of any `ext:` names a node stored as `<n>.node`
  THEN it is accepted. A `.node` file is written and read by its metadata (type and data format),
  and reading one with no metadata fails with an error that says so.
- **AC-13** No archive of plain recipes
  WHEN a folder whose tree is invalid is exported as a portable archive
  THEN the export is refused with the findings (recorded on `RECIPE-TREE-CANNOT-BE-EXPORTED-AS-AN-ARCHIVE`,
  which is not implemented yet).

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

### Open Questions

1. **What an empty node evaluates to** (AC-5). `{query: 1.node}` is a query that is only a
   filename, which today means a source declaration. Proposed: a filename-only query whose
   extension is `node` is an *empty node*. It is not a source, and it evaluates to the empty value
   (`None`), stored as `1.node`. Alternative: write it as a pipeline with only a filename.
2. **How a node is renamed** when its first command declares a `filename` (`net` → `net.obj`).
   Should the query edit rename `1.node` to the command's name (`net.obj`), or keep `1.node` until
   the user renames it? Proposed: keep the name and extension the user sees (`1.node`), and rename
   only on request. That keeps a reference stable, and `.node` is valid everywhere.
3. **Which repairs are meaning-preserving** (AC-11) besides adding `tree: {}`. For example, dropping
   a `result` that names no node, or a `roots` entry that is not a node. Proposed: only these
   three, everything else reported.
4. **Does `dependencies: strict` still matter at evaluation?** With AC-8 a strict tree that checks
   is used as a tree, and one that fails is used as plain recipes. Proposed: yes, as described, so
   strictness is a promise checked on save and archive, not a gate.

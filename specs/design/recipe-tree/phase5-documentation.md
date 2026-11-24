# Phase 5: Documentation - recipe-tree

## Completion Preconditions

- [x] Implementation is finished and validated
- [x] All user comments are answered or incorporated (approved by the maintainer, 2026-11-24)
- [x] All review comments are answered or incorporated (no review comments yet)
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR when practical (PR #281)

## Implementation Summary

Implemented as approved in Phases 1-4, Steps 1-11 of
[`phase4-implementation.md`](phase4-implementation.md):

- **Node arguments.** `ArgumentType::Node(NodeArgument { kind, extension, form })`, the
  `register_command!` statements `node:`, `ext:` and `form:`, the default `NodeSelector` widget,
  and planning of a bare name as `ParameterValue::NodeLink` (a link into the working directory;
  `KeyNotAbsolute` from `Error::node_needs_cwd` without one). Every exhaustive match over
  `ArgumentType`, `ArgumentGUIInfo` and `ParameterValue` in core, lib, mcp, py and axum is updated.
- **Source recipes.** A recipe whose query is only a filename (`Recipe::is_source_declaration`)
  serves the stored data; the asset manager reports status `Source` with the recipe's title and
  description, and `Error` / `KeyNotFound` when the data is missing. Stored-status decisions treat
  the key as having no recipe, as the maintainer asked.
- **The tree.** `liquers_core::recipe_tree`: `TreeDeclaration` (`tree:` section),
  `TreeGraph`, `check_strict`, `check_tree` → `TreeReport` of `TreeFinding`s.
- **Strict enforcement.** `DefaultRecipeProvider` checks a strict `recipes.yaml` with the
  store's listing and refuses the whole file on an error, uncached.
- **`TreeEditor`**: rename, add_child, delete, move_reference, declare_source, save.
- **Validation.** `liquers-validate --recipes` reports the tree (`ValidationReport::tree`,
  `ERROR    [tree] …`, exit 1).
- **Editors.** Node fields list the folder's nodes and offer Rename in the egui and browser
  editors (`NodeContext`, `EditorCommand::RenameNode`, `AppMessage::RenameData`).

Deviations from the Phase 2/4 text, none changing behaviour the maintainer approved:

- **Delete.** Phase 3 Example 2 had `delete(node, cascade)` remove a referenced node. The
  implementation never deletes a node something references (it is refused, naming the parents);
  `cascade` only also removes what the deleted node alone reaches. The example was corrected.
- `check_strict` takes the node's name (for findings) besides the plan; `TreeEdge.to` is a
  filename `String` rather than a node index, because a dangling target is not a node.

## Documentation Delivered

### New Reference Documents
- [`reference/RECIPE_TREES.md`](../../reference/RECIPE_TREES.md): the contract — declaration,
  nodes and names, source recipes, Node arguments, strict mode and its enforcement, the check,
  `TreeEditor`, the editors, limitations and the pinning tests.

### New Guide Documents
None: the command-side usage fits `COMMAND_REGISTRATION_GUIDE.md`, the rest is reference.

### Existing Documents Reviewed or Updated
All of `affects_docs`, each with a History row and `reviewed: 2026-11-23`:
`reference/RECIPES_AND_PLANS.md` (new §Recipe trees), `reference/PIPELINES.md` §8,
`reference/REGISTER_COMMAND_FSD.md` (node statements, `NodeSelector`),
`guides/COMMAND_REGISTRATION_GUIDE.md` (a Node example), `reference/UI_INTERFACE_FSD.md` (Node
arguments), and the two user-guide chapters below.

### Module Docs
Created `liquers-core/src/recipe_tree.rs`; written for `command_metadata.rs` (it had none);
brought up to the guide for `recipes.rs`; extended `validate/mod.rs` and
`liquers-lib/src/ui/editor/mod.rs`. `python3 scripts/check_api_docs.py` on the five files:
`5 modules, 0 warnings, 0 errors`; their doctests pass.

### User-Guide Chapters
`docs/guides/rust/07-assets.md` §7.8 gains "Recipe trees"; `docs/guides/rust/05-commands.md` §5.3
gains "Nodes". Both reviewed and listed in `docs/guides/rust/HISTORY.md`; the queries were checked
with `liquers-validate`. Python and web guides: no change, the feature is not in the bindings.

### Language Integration
Later: `RECIPE-TREES-NOT-IN-LANGUAGE-BINDINGS`. The bindings compile against the new variants
(a Python command receives a NodeLink argument as its resolved value, like any link), but neither exposes the tree API.

### Links and Capability Map
`specs/README.md`: "Self-contained recipe trees" → `reference/RECIPE_TREES.md`, and the design
entry moves to complete.

## Issues Filed

- `RECIPE-TREES-NOT-IN-LANGUAGE-BINDINGS` — Python and JavaScript bindings.
- `RECIPE-TREE-CANNOT-BE-EXPORTED-AS-AN-ARCHIVE` — the portable archive, a separate design.
- `RECIPE-OVERRIDES-MISS-A-FINAL-PIPELINE-COMMAND` — an `arguments` override does not reach a
  final pipeline command (found while writing the rename).

## Important Learning

- A bare name planned as a link (`NodeLink`) reuses the whole link machinery — dependency
  recording, cancellation, caching — so `attach-wheel` and its explicit link give the same value
  for free, the same way enum aliases do with `EnumLink`.
- "Strict" is about what the *recipe* writes. Command defaults and enum aliases are command
  logic and trusted; otherwise no library command with a default link could be used in a strict
  folder.
- Planning without a working directory keeps keys as written, which is what makes relocation and
  the check independent of where the folder is.

## Conformance and Remaining Work

Requested: a special form of recipes that represents trees and is self-sufficient. Approved:
AC-1 to AC-28 (AC-22 retired). Implemented: all of them; the tests that pin them are listed in
`RECIPE_TREES.md`. Remaining work is filed above; nothing is left partial.

## Validation

- `cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-lib --lib --tests` (which
  includes `registry_export`) and `cargo test -p liquers-mcp`: all pass, before and after merging
  `main`.
- `bash scripts/check-build-matrix.sh`: every native row passes; the wasm32 rows failed only
  because the target was not installed, and after `rustup target add wasm32-unknown-unknown`
  all nine wasm32 checks pass (core, lib `webui` and `webui,records,records-ipc`, web with and
  without `records`, records, template, markdown, store).
- `python3 scripts/check_api_docs.py` (5 files): 0 warnings, 0 errors.
- `python3 scripts/docs_index.py --check`: no errors after regeneration.
- `cargo test -p liquers-docs`: passing.

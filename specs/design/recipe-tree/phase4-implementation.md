# Phase 4: Implementation Plan — Self-contained recipe trees

## Overview

Bottom-up, so each step compiles and is tested before the next uses it: the Node argument type and
its planning first (useful on its own), then source declarations, the tree module, its
enforcement and reporting, the editor, and the UI last. No prerequisite issue must be fixed first
(Phase 2 preflight). Steps 9 and 10 can run in parallel after step 8.

## Progress

- [ ] Step 1: Node argument metadata and match arms
- [ ] Step 2: `register_command!` DSL for Node arguments
- [ ] Step 3: Planning a Node argument (`NodeLink`, fallible CWD freeze)
- [ ] Step 4: Source declarations
- [ ] Step 5: Tree declaration, graph, strict rules and tree check
- [ ] Step 6: Strict enforcement in `DefaultRecipeProvider`
- [ ] Step 7: `TreeEditor`
- [ ] Step 8: Tree report in `liquers-validate`
- [ ] Step 9: Node selector and rename in the editors
- [ ] Step 10: Documentation
- [ ] Step 11: Final checks

## Implementation Steps

### Step 1: Node argument metadata and match arms
- Files / symbols: `liquers-core/src/command_metadata.rs` (`NodeArgument`, `NodeKind`,
  `NodeLinkForm`, `ArgumentType::Node`, `ArgumentGUIInfo::NodeSelector`, `NodeArgument::link`,
  `name_of`); every exhaustive `match` on `ArgumentType` / `ArgumentGUIInfo`:
  `pipeline/{editor,convert}.rs`, `recipes.rs`, `command_declaration.rs`, `plan.rs`;
  `liquers-macro/src/registration.rs`; `liquers-lib/src/{commands,utils}.rs`,
  `ui/editor/{state,tree}.rs`, `egui/widgets.rs`; `liquers-mcp/src/{schema,server}.rs` (a string
  schema naming the extension); `liquers-py/src/command.rs`; `liquers-web/src/typescript.rs`.
- Change: the types of Phase 2 §Interfaces; `from_string` returns `not_supported` for Node until
  step 3.
- Depends on: none.
- Proof: `cargo test -p liquers-core --lib command_metadata` (`node_argument_serde_round_trip`,
  `node_link_builds_both_forms`, `node_link_refuses_name_with_slash`); `cargo check -p liquers-lib -p liquers-mcp`.
- Rollback: revert the commit; nothing serializes a Node yet.

### Step 2: `register_command!` DSL for Node arguments
- Files / symbols: `liquers-macro/src/registration.rs` (argument metadata parser beside `enum:`).
- Change: `node: local`, `ext: "…"`, `form: resource|key`; compile errors for `ext` without `node`
  and the reverse; `Key` or `Value` required for the key form.
- Depends on: step 1.
- Proof: `cargo test -p liquers-core --test node_argument register_node_argument_with_macro`.
- Rollback: revert; step 1 metadata can still be built by hand.

### Step 3: Planning a Node argument
- Files / symbols: `liquers-core/src/plan.rs` (`ParameterValue::NodeLink`, `from_string`,
  `ParameterValue::freeze_cwd` and `ResolvedParameterValues::freeze_cwd` returning `Result`,
  `Plan::freeze_cwd_with`, `find_dependencies`); `interpreter.rs`, `commands.rs`,
  `pipeline/plan.rs` (`Source::Value` for a Node → `NodeLink`; back to `Source::Value(name)`),
  `store/realm.rs`, `liquers-axum/src/api_core/overrides.rs`; `error.rs` (`node_needs_cwd`).
- Change: as Phase 2; a `NodeLink` is evaluated and recorded like `ParameterLink`.
- Depends on: step 1.
- Proof: `cargo test -p liquers-core --test node_argument` (`node_name_resolves_to_link`,
  `node_name_without_cwd_fails_naming_argument`, `node_link_needs_no_cwd`,
  `node_argument_accepts_any_link_outside_strict`); `cargo test -p liquers-core --test plan_cwd_freeze`
  unchanged.
- Rollback: revert; step 1's `not_supported` returns.

### Step 4: Source declarations
- Files / symbols: `liquers-core/src/recipes.rs` (`Recipe::is_source_declaration`);
  `liquers-core/src/assets.rs` (where a keyed recipe runs after the stored-data check,
  `try_fast_track` and its caller: a source declaration with no stored data fails with
  `Error::key_not_found` and stores nothing).
- Depends on: none (parallel with steps 1-3).
- Proof: `cargo test -p liquers-core --test recipe_tree source_declaration_serves_stored_data source_declaration_without_data_is_key_not_found`;
  `cargo test -p liquers-core --lib assets` unchanged.
- Rollback: revert; an empty recipe behaves as before.

### Step 5: Tree declaration, graph, strict rules and tree check
- Files / symbols: new `liquers-core/src/recipe_tree.rs` (`TreeDeclaration`, `TreeShape`,
  `DependencyMode`, `NodeOrigin`, `TreeNode`, `RefKind`, `TreeEdge`, `TreeGraph::build`,
  `TreeFinding`, `TreeReport`, `check_strict`, `check_tree`); `recipes.rs` (`RecipeList::tree`);
  `lib.rs`; `error.rs` (`strict_dependency`, `tree_cycle`, `tree_check`).
- Depends on: steps 3, 4.
- Proof: `cargo test -p liquers-core --lib recipe_tree` (all unit tests of Phase 3 item 5);
  `cargo test -p liquers-core --test recipe_tree tree_declaration_round_trips folder_without_tree_unchanged`.
- Rollback: revert; `tree:` is ignored again.

### Step 6: Strict enforcement in `DefaultRecipeProvider`
- Files / symbols: `recipes.rs` `DefaultRecipeProvider::cached_recipes`, `CachedRecipes`.
- Change: a strict folder runs `check_tree` with the store's `listdir` of the folder; any error
  refuses every lookup and the file is not cached.
- Depends on: step 5.
- Proof: `cargo test -p liquers-core --test recipe_tree strict_folder_fails_as_a_whole strict_tree_relocates result_key_evaluates_the_tree`.
- Rollback: revert; strict folders are then only reported.

### Step 7: `TreeEditor`
- Files / symbols: `recipe_tree.rs` (`TreeEditor`, `NodeChoice`, `DataRename` and the methods of
  Phase 2), using `pipeline::{PipelineEditor, QueryEditor}`.
- Depends on: step 5.
- Proof: `cargo test -p liquers-core --test recipe_tree` (rename, add, delete, move, refused-edit
  and unrewritable-reference tests).
- Rollback: revert; the check stays.

### Step 8: Tree report in `liquers-validate`
- Files / symbols: `validate/{mod,report}.rs` (`ValidationReport::tree`, `validate_recipes`),
  `bin/liquers_validate.rs`.
- Depends on: step 5.
- Proof: `cargo test -p liquers-core --test validate_integration recipes_tree_report_and_exit_code`;
  `cargo run -p liquers-core --features cli --bin liquers-validate -- --recipes <fixture> --cwd scenes/car`.
- Rollback: revert; `tree` is absent from the report.

### Step 9: Node selector and rename in the editors
- Files / symbols: `liquers-lib/src/ui/editor/{tree,state,command}.rs` (`NodeSelector` choices,
  `EditorCommand::RenameNode`, `EditorEffect::RenameData`), `ui/runner.rs`
  (`AppMessage::RenameData`), `egui/editor.rs`, `ui/web/editor.rs`.
- Depends on: step 7.
- Proof: `cargo test -p liquers-lib --lib ui::editor` (`node_selector_lists_filename_and_title`,
  `node_selector_shows_link_as_name`, `node_selector_rename_writes_document_and_data`).
- Rollback: revert; Node fields fall back to a text box.

### Step 10: Documentation
- Files: as Phase 2 §Documentation Architecture and the module docs it lists.
- Depends on: steps 1-9.
- Proof: `python3 scripts/check_api_docs.py` on each touched module; `python3 scripts/docs_index.py --check`;
  `cargo test -p liquers-docs`; doc tests `cargo test -p liquers-core --doc recipe_tree`.
- Rollback: revert the documents.

### Step 11: Final checks
- Proof: `cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-lib --lib --tests`,
  `cargo test -p liquers-mcp`, `bash scripts/check-build-matrix.sh` (match arms in feature-gated UI
  code and in `liquers-web`), `cargo test -p liquers-lib --test registry_export` (registry unchanged).
- Rollback: fix forward in the failing step's files.

## Testing Plan

Each step runs only its own proof. Step 11 runs the crate suites once; on failure, only the failing
tests are re-run while fixing. The build matrix runs once, at the end.

## Rollback Plan

Each step is its own commit. Steps 1-3 (Node) are useful alone; if the work stops after step 6,
the remainder (editor, report, UI) becomes an issue under §5.6.

## Documentation Updates

`specs/reference/RECIPE_TREES.md` (new); History rows and `reviewed:` bumps on
`RECIPES_AND_PLANS.md`, `PIPELINES.md`, `REGISTER_COMMAND_FSD.md`, `COMMAND_REGISTRATION_GUIDE.md`,
`UI_INTERFACE_FSD.md`; `docs/guides/rust/07-assets.md` and `05-commands.md`; `specs/README.md`;
regenerate `specs/index.csv`. `specs/command_registry.yaml` is not regenerated (no command changes).

## Phase 5 Entry Criteria

- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour

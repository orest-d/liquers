# Phase 3: Examples & Use-cases — Self-contained recipe trees

Every query below validates with `liquers-validate` (`--command attach --command material` for the
test commands). The fixture is one folder, `scenes/car/`, in an `AsyncMemoryStore` with
`DefaultRecipeProvider`, as in `liquers-core/tests/recipe_cwd_resolution.rs`.

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | A car as a strict tree | declaration, Node names, sources, check, result, relocation | AC-2, AC-10, AC-19, AC-20, AC-26, AC-27 |
| 2 | Example | Renaming and editing | `TreeEditor` rename, add, delete, move | AC-1, AC-11–AC-15, AC-25 |
| 3 | Tests | `liquers-core/src/command_metadata.rs` `tests` | `NodeArgument` serde and links | AC-17 |
| 4 | Tests | `liquers-core/tests/node_argument.rs` | planning a Node, with and without CWD | AC-17, AC-20, AC-21 |
| 5 | Tests | `liquers-core/src/recipe_tree.rs` `tests` | graph, strict rules, tree check | AC-3–AC-8, AC-16, AC-23, AC-27, AC-28 |
| 6 | Tests | `liquers-core/tests/recipe_tree.rs` | provider enforcement, evaluation, editor | AC-1, AC-2, AC-10–AC-15, AC-19, AC-24, AC-26, AC-27 |
| 7 | Tests | `liquers-core/tests/validate_integration.rs` | tree report and exit code | AC-9 |
| 8 | Tests | `liquers-lib/src/ui/editor/tests.rs` | node selector and rename | AC-18, AC-25 |

## Example 1: a car as a strict tree

Test commands: `attach(state, part: Value (node: local, ext: "glb"))` appends `+<part>` to the
input text; `material(state, m: Value (node: local, ext: "mat"))` appends `~<m>`. The store holds
`scenes/car/body.glb` = `body`, `tyre.glb` = `tyre`, `rubber.mat` = `rubber`.

```yaml
# scenes/car/recipes.yaml
tree: {dependencies: strict, result: car.glb}
recipes:
  - {query: body.glb, title: Body}
  - {query: tyre.glb, title: Tyre}
  - {query: rubber.mat, title: Rubber}
  - {title: Wheel, query: "-R/./tyre.glb/-/material-rubber/wheel.glb"}
  - {title: Car,   query: "-R/./body.glb/-/attach-wheel/car.glb"}
```

```rust
let report = check_tree(&list, Some(&listing), &cmr);       // listing = store.listdir(folder)
assert!(report.errors.is_empty());
assert_eq!(report.result.as_deref(), Some("car.glb"));
let car = evaluate(envref.clone(), "-R/scenes/car/car.glb", None).await?;
assert_eq!(car.try_into_string()?, "body+tyre~rubber");
// copy every file of scenes/car/ to scenes/truck/ (AC-10)
let truck = evaluate(envref.clone(), "-R/scenes/truck/car.glb", None).await?;
assert_eq!(truck.try_into_string()?, "body+tyre~rubber");
```

Without the `tree:` line the same file evaluates identically (AC-2, AC-26).

## Example 2: renaming and editing

```rust
let mut editor = TreeEditor::new(list, Some(listing), cmr.clone())?;
editor.rename("wheel", "front_wheel")?;
let car = editor.recipes().recipes.iter().find(|r| r.title == "Car").unwrap();
assert_eq!(car.query, "-R/./body.glb/-/attach-front_wheel/car.glb");
assert!(editor.report().errors.is_empty());
editor.save(&parse_key("scenes/car")?, envref.clone()).await?;   // car still "body+tyre~rubber"
```

`rename("tyre", "tire")` also yields `DataRename { from: "tyre.glb", to: "tire.glb" }`, performed
by `save`. `delete("wheel", false)` is refused naming `car.glb`; `delete("wheel", true)` removes
`wheel.glb` and the source recipes `tyre.glb` and `rubber.mat`, which nothing else references;
stored data is never deleted by the editor.

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| `attach-wheel` evaluated with no CWD | `KeyNotAbsolute` naming `part` and `wheel` | `node_name_without_cwd_fails_naming_argument` |
| `attach-~X~-R/data/x.glb~E` outside a tree | accepted, planned as a link | `node_argument_accepts_any_link_outside_strict` |
| Node name `a/b` | `invalid_token` | `node_link_refuses_name_with_slash` |
| Strict recipe passes a link to an `Any` argument | strict finding naming recipe and argument | `strict_refuses_link_argument` |
| Command default link `-R-key/.` in a strict recipe | allowed | `strict_trusts_default_and_enum_links` |
| Strict file with one bad recipe | every recipe of the folder fails to load; not cached | `strict_folder_fails_as_a_whole` |
| `logo.png` and `logo.jpg`, one referenced | collision error naming both | `name_collision_refused_when_referenced` |
| Source recipe with no stored data | status `Error`, `KeyNotFound`, nothing stored | `source_declaration_without_data_is_key_not_found` |
| Rename meets an unparseable query | refused, list unchanged | `rename_refuses_unrewritable_reference` |
| `tree:` round trip | YAML out equals YAML in | `tree_declaration_round_trips` |

## Test Plan

- Unit, `command_metadata.rs`:
  - `node_argument_serde_round_trip` (AC-17): `{node: {kind: local, extension: glb, form: key}}`;
  - `node_link_builds_both_forms` (AC-17): `-R/./w.glb`, `-R-key/./w.glb`, and `name_of` inverts them;
  - `node_link_refuses_name_with_slash` (AC-17).
- Integration, `liquers-core/tests/node_argument.rs`:
  - `register_node_argument_with_macro` (AC-17): DSL `(node: local, ext: "glb", form: key)` on `Key` and `Value`;
  - `node_name_resolves_to_link` (AC-20): `-R-cwd/scenes/car/-R/./body.glb/-/attach-wheel` equals
    `-R-cwd/scenes/car/-R/./body.glb/-/attach-~X~-R/./wheel.glb~E`;
  - `node_name_without_cwd_fails_naming_argument` (AC-20);
  - `node_link_needs_no_cwd` (AC-20);
  - `node_argument_accepts_any_link_outside_strict` (AC-21).
- Unit, `recipe_tree.rs`:
  - `graph_lists_resource_node_and_link_edges` (AC-1, AC-27);
  - `default_roots_are_unreferenced_nodes`, `declared_root_must_be_a_node` (AC-3);
  - `cycle_reported_in_order` (AC-4);
  - `tree_shape_refuses_two_parents`, `dag_allows_two_parents` (AC-5);
  - `orphan_is_a_warning` (AC-6);
  - `strict_refuses_absolute_parent_and_subfolder_keys` (AC-7);
  - `dangling_reference_is_an_error` (AC-8);
  - `name_collision_refused_when_referenced`, `name_collision_allowed_when_unreferenced` (AC-16);
  - `result_is_the_only_root` (AC-19);
  - `strict_refuses_link_argument`, `strict_trusts_default_and_enum_links` (AC-23);
  - `source_declaration_is_a_node_with_title` (AC-27);
  - `strict_requires_declared_sources` (AC-28).
- Integration, `liquers-core/tests/recipe_tree.rs`:
  - `tree_declaration_round_trips`, `folder_without_tree_unchanged` (AC-2, AC-26);
  - `strict_tree_relocates` (AC-10);
  - `result_key_evaluates_the_tree` (AC-19);
  - `strict_folder_fails_as_a_whole` (AC-24);
  - `source_declaration_serves_stored_data`: value is the stored data; asset metadata has status
    `Source`, the recipe's title and description; the store's metadata is unchanged (AC-27);
  - `source_declaration_without_data_is_key_not_found` (AC-27);
  - `rename_rewrites_every_reference_and_keeps_value` (AC-1, AC-25 core);
  - `rename_renames_stored_file_on_save` (AC-1);
  - `add_child_refuses_used_name` (AC-11);
  - `delete_refused_while_referenced`, `delete_cascades` (AC-12);
  - `move_reference_refused_when_shape_breaks` (AC-13);
  - `refused_edit_changes_nothing` (AC-14);
  - `rename_refuses_unrewritable_reference` (AC-15).
- Integration, `validate_integration.rs`: `recipes_tree_report_and_exit_code` (AC-9).
- Unit, `liquers-lib/src/ui/editor/tests.rs`:
  - `node_selector_lists_filename_and_title` (AC-18);
  - `node_selector_shows_link_as_name` (AC-18);
  - `node_selector_rename_writes_document_and_data` (AC-25).
- Commands: `cargo test -p liquers-core --lib --tests recipe_tree node_argument validate_integration`,
  then `cargo test -p liquers-lib --lib ui::editor`.

## Learning Log

- `body.glb` alone is a valid query: one `Filename` step, no action (checked with
  `liquers-validate --emit pipeline`). That is what makes a source declaration a plain recipe.
- `ParameterValue::EnumLink` is the precedent for turning a string parameter into a link at
  planning; Node follows it with `NodeLink`.

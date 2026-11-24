# Phase 3: Examples & Use-cases — Recipe and recipes editors

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Build a recipe tree in the panel | new, edit, rename, dependencies, write | AC-1, AC-3, AC-7, AC-8, AC-11 |
| 2 | Example | One recipe by filename | `acc/recipe_file` + `lui/recipe_editor` | AC-5, AC-6 |
| 3 | Example | `ui_spec_editor` example | the panel and windows drawn in egui, headless `--check` | AC-4, AC-16, AC-17, AC-19 |
| 4 | Tests | listed under Test Plan | every scenario | AC-1 … AC-19 |

## Example 1: Build a recipe tree in the panel

The backend-free state, as a renderer drives it. The registry has a command `attach` with a Node
argument `part` of extension `glb`, as in `recipe_tree.rs`'s tests.

```rust
let mut panel = RecipesState::new(registry.clone());
panel.on_snapshot(&Value::from("tree: {result: car.glb}\nrecipes:\n  - {query: body.glb, title: Body}\n"), None)?;
assert_eq!(panel.rows()[0].name, "body");                                  // AC-3: node names
let e = panel.apply(RecipesCommand::New { title: "Car".into(), extension: "glb".into() })?;
assert!(matches!(e, RecipesEffect::Write { .. }));                          // continuous: written
assert!(panel.form("car.glb").is_some());                                   // AC-7: its window opens
panel.apply(RecipesCommand::Form { filename: "car.glb".into(),
    command: FormCommand::Editor(EditorCommand::SetText("attach-body/car.glb".into())) })?;
panel.apply(RecipesCommand::Rename { from: "body".into(), to: "chassis".into() })?;   // AC-8
assert!(panel.document_text().contains("attach-chassis/car.glb"));
panel.apply(RecipesCommand::SetView(RecipesView::Dependencies))?;            // AC-11
assert_eq!(panel.rows().iter().map(|r| (r.depth, r.name.as_str())).collect::<Vec<_>>(),
           vec![(0, "car"), (1, "chassis")]);
```

## Example 2: One recipe by filename

The query `-R-key/scenes/car/recipes.yaml/-/ns-acc/accessor/recipe_file-wheel.glb/ns-lui/recipe_editor`
(validated; `recipe_file` and `recipe_editor` added with `--command` until the registry is
regenerated). The accessor follows `wheel.glb` when recipes are added before it. The form's title
edit writes `Value::Recipe` through `set_if`.

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| empty or missing `recipes.yaml` (value `None`) | empty outline, New works | `missing_document_is_an_empty_list` |
| YAML that does not parse | the YAML window shows the error, nothing applied; the panel loads read-only with the message | `yaml_apply_refuses_invalid_text`, `invalid_document_loads_read_only` |
| a recipe without a filename | row `#<index>`, default icon, no rename | `recipe_without_filename_is_listed` |
| a cycle in the folder | Dependencies shows it once, marked | `dependencies_view_repeats_shared_nodes_and_stops_cycles` |
| a write refused by `set_if` | live value loaded, message names the lost text | `conflict_reloads_and_reports` |
| a rename with a stored file | `SaveTree` effect instead of `Write` | `rename_with_stored_data_saves_tree` |
| Save as without a value | the runner's notice reports the error | `save_as_writes_a_file` (error branch) |

## Test Plan

Unit, `liquers-core/src/accessor/steps.rs`:
- `recipe_file_step_reads_and_writes_by_filename` (AC-6)
- `recipe_file_step_follows_reordering` (AC-6)
- `recipe_file_step_refuses_a_filename_change` (AC-6, AC-8)

Unit, `liquers-core/src/recipe_tree.rs`:
- `add_appends_and_refuses_a_used_name` (AC-7)
- `duplicate_copies_under_a_new_name` (AC-9)
- `set_recipe_refuses_a_new_cycle` (AC-5, AC-14)
- `set_result_declares_the_result` (AC-3, AC-19)
- `replace_takes_any_parsed_list` (AC-12)

Unit, `liquers-lib/src/ui/recipes/tests.rs`:
- `outline_lists_recipes_in_file_order` (AC-1)
- `icon_from_metadata_else_extension` (AC-2)
- `tree_rows_show_node_names_result_and_sources` (AC-3)
- `open_twice_keeps_one_form` (AC-4)
- `form_edits_fields_and_query` (AC-5)
- `form_follows_its_recipe_after_reordering` (AC-6)
- `new_recipe_opens_its_form` (AC-7)
- `rename_rewrites_references_and_rekeys_the_form` (AC-8)
- `rename_with_stored_data_saves_tree` (AC-8)
- `delete_is_refused_while_referenced_and_duplicate_appends` (AC-9)
- `findings_attach_to_rows` (AC-10)
- `dependencies_view_repeats_shared_nodes_and_stops_cycles` (AC-11)
- `yaml_apply_refuses_invalid_text` (AC-12), `yaml_spans_mark_keys_and_queries` (AC-12)
- `on_demand_holds_edits_until_update` (AC-13), `apply_immediately_writes_each_edit` (AC-13)
- `refused_edit_changes_nothing` (AC-14)
- `conflict_reloads_and_reports` (AC-15)
- `label_setting_chooses_name_title_or_both` (AC-16)
- `status_progress_and_errors` (AC-17)
- `tooltips_give_title_and_description` (AC-18)
- `menu_actions_produce_their_effects` (AC-19)
- `missing_document_is_an_empty_list`, `invalid_document_loads_read_only`,
  `recipe_without_filename_is_listed` (AC-1)

Unit, `liquers-lib/src/ui/web/recipes.rs`:
- `panel_html_has_rows_tooltips_status_and_menu` (AC-16, AC-17, AC-18, AC-19)
- `recipe_form_html_has_fields` (AC-5)

Integration, `liquers-lib/tests/ui_recipes_editor.rs` (runner, memory store):
- `recipes_editor_writes_through_the_runner` (AC-1, AC-13)
- `recipe_editor_edits_one_recipe_by_filename` (AC-5, AC-6)
- `rename_with_stored_data_through_the_runner` (AC-8)
- `save_as_writes_a_file` (AC-19)

Integration, `liquers-lib/tests/value_accessor_commands.rs`:
- `recipe_file_command_projects_by_filename` (AC-6)

Example check: `cargo run -p liquers-lib --example ui_spec_editor -- --check` (AC-4, AC-16, AC-17,
AC-19 drawn headless without a panic).

Commands: `cargo test -p liquers-core --lib accessor::steps recipe_tree`;
`cargo test -p liquers-lib --lib ui::recipes ui::web::recipes`;
`cargo test -p liquers-lib --test ui_recipes_editor --test value_accessor_commands --test registry_export`.

## Learning Log

- The console's Parameters panel shows how to embed an `EditorState` without an accessor: take
  its `Write` effect as the new value and answer `on_written` with a zero fingerprint.

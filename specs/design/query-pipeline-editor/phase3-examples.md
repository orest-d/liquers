# Phase 3: Examples & Use-cases — Query and pipeline editor

Pre-approved after Phase 2 (`proceed all`, Orest, 2026-11-17). Every test below is runnable; the
egui and browser drawing are tested without a window (an `egui::Context` run headless, the
markup as a string).

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Edit a recipe's query in two editors | tree, widgets, write-back, second editor follows | AC-1, AC-2, AC-9, AC-26 |
| 2 | Example | Swap a namespace safely | `namespace_impact`, confirmation | AC-16, AC-25 |
| 3 | Test | core `pipeline::editor::tests` (9 tests) | new `ActionEditor` / `PipelineEditor` edits | AC-2, AC-16, AC-18, AC-19, AC-20, AC-23, AC-25 |
| 4 | Test | lib `ui::editor::tests` (22 tests) | the backend-free core | AC-1 to AC-10, AC-12 to AC-26 |
| 5 | Test | lib `egui::editor::tests`, `ui::web::editor::tests` | both renderers | AC-3, AC-11 |
| 6 | Test | lib `tests/ui_editor_binding.rs` (4 tests) | runner, accessor, recipe expiry | AC-8, AC-9, AC-13, AC-26 |

## Example 1: Edit a recipe's query in two editors

`reports/recipes.yaml` holds recipe 0 with the problem example's query (shortened to plain
parameters so it needs only the test commands `from_csv`, `is_in`, `head` of
`tests/value_accessor_commands.rs`):

```
-R/data/sales.csv/-/ns-pl/from_csv/is_in-region-EU-US/head-10/top.csv
```

Two elements are created from the same binding (validated: `Ok`, `ns-acc`, `ns-lui`):

```
-R-key/reports/recipes.yaml/-/ns-acc/accessor/recipe-0/query/ns-lui/query_editor
```

```rust
// The runner evaluates the binding, takes the accessor and sends a snapshot to each editor.
runner.run(&app_state).await?;
let tree = editor_tree(&app_state, a).await;           // nodes: Resource, Namespaces, 3 × Action
assert_eq!(tree.nodes[4].action().fields[0].widget, ArgumentGUIInfo::IntegerField);
// The user appends APAC to `values` of `is_in` (path: element 3, argument "values").
send(a, EditorCommand::InsertItem { path: path![3, "values"], at: 2, text: "APAC".into() });
runner.run(&app_state).await?;                       // set_if, then Changed to editor b
let tree_b = editor_tree(&app_state, b).await;
assert_eq!(tree_b.text, "-R/data/sales.csv/-/ns-pl/from_csv/is_in-region-EU-US-APAC/head-10/top.csv");
// recipes.yaml now holds the new query, and the keyed recipe asset `reports/top.csv` is expired.
```

With "Update immediately" off on editor `a`, the same edit leaves `tree.pending == true`, nothing
is written, and editor `b` still shows `EU-US` until `EditorCommand::Update`.

## Example 2: Swap a namespace safely

With `pl` and `rec` both registering `head`, but `from_csv` only in `pl`:

```rust
let q = QueryEditor::from_query(&parse_query("-R/a.csv/-/ns-pl/from_csv/head-5")?, cmr)?;
assert_eq!(q.namespace_impact(1, &["pl".into(), "rec".into()])?, NamespaceImpact::Same);
assert!(matches!(q.namespace_impact(1, &["rec".into(), "pl".into()])?,
                 NamespaceImpact::Changed(c) if c[0].after.namespace == "rec"));   // head moves
assert!(matches!(q.namespace_impact(1, &["rec".into()])?, NamespaceImpact::Invalid(_))); // from_csv lost
```

In the editor, the second edit shows "head resolves to rec/head instead of pl/head" with Confirm
and Cancel; the third is refused at the `ns` node.

## Edge and Error Cases

| Symptom | Expected behaviour | Test |
|---|---|---|
| Text that does not parse | error position highlighted, tree stale, nothing written | `text_edit_rebuilds_tree_or_marks_it_stale` |
| An action with more parameters than its command takes | opens; extras read-only | `excess_parameters_are_kept_and_listed`, `unknown_command_and_excess_are_shown_read_only` |
| A refused edit | message at the field, query unchanged, no `Write` | `refused_edit_changes_nothing` |
| Two editors edit from the same fingerprint | second is a conflict, reloads | `second_editor_conflict_reloads` |
| Fast slider | one write in flight; later edits coalesce | `one_write_in_flight_coalesces` |
| Binding evaluates to a non-accessor | read-only with the error | `binding_that_is_not_an_accessor_is_read_only` |
| A failed directory listing | selector keeps the error, typed key accepted | `browse_lists_directory_and_accepts_any_key` |
| Old app state JSON | loads, `continuous` defaults to true | `element_round_trips_and_old_console_state_loads` |
| No registry in the context | text only, message "no command registry" | `editor_without_registry_shows_text_only` |

## Test Plan

Core — `liquers-core/src/pipeline/editor/tests.rs`:
- `reset_removes_last_and_writes_default_in_middle` (AC-18)
- `variadic_items_insert_remove_move` (AC-2)
- `set_source_switches_between_value_and_link` (AC-19)
- `excess_parameters_are_kept_and_listed` (AC-23)
- `namespace_impact_same_changed_invalid` (AC-25)
- `set_namespaces_needs_acceptance_for_changed` (AC-25, AC-16)
- `available_and_unused_namespaces` (AC-16)
- `action_from_preset_request` (AC-20, AC-21)
- `parameter_preset_accessors` (AC-24), in `liquers-core/src/command_metadata.rs` tests

Lib core — `liquers-lib/src/ui/editor/tests.rs`:
- `tree_of_the_problem_example` (AC-1), `effective_widgets_from_type_and_gui_info` (AC-2),
  `fields_carry_hover_documentation` (AC-3), `text_edit_rebuilds_tree_or_marks_it_stale` (AC-4),
  `refused_edit_changes_nothing` (AC-5), `insert_remove_append_and_move` (AC-6),
  `resource_header_and_key` (AC-7), `action_kind_edits_one_element_pipeline` (AC-8),
  `second_editor_conflict_reloads` (AC-9), `pipeline_text_is_read_only` (AC-10),
  `element_round_trips_and_old_console_state_loads` (AC-12),
  `numeric_controller_scales_into_the_argument` (AC-13),
  `browse_lists_directory_and_accepts_any_key` (AC-14), `key_kind_and_key_links` (AC-15),
  `directives_are_nodes` (AC-16), `command_catalog_filters_and_inserts_ns` (AC-17),
  `reset_field_to_default` (AC-18), `link_argument_is_a_subtree` (AC-19),
  `presets_replace_and_next_inserts` (AC-20, AC-21), `filename_button_field_and_default` (AC-22),
  `unknown_command_and_excess_are_shown_read_only` (AC-23), `argument_preset_menu` (AC-24),
  `namespace_change_awaits_confirmation` (AC-25), `continuous_and_on_demand_writes` (AC-26),
  `one_write_in_flight_coalesces` (AC-26), `editor_without_registry_shows_text_only` (AC-5)

Renderers:
- `liquers-lib/src/egui/editor.rs` — `show_editor_draws_every_node_kind` (AC-11, AC-3): a headless
  `egui::Context::run` over the problem example's tree returns no commands and does not panic.
- `liquers-lib/src/ui/web/editor.rs` — `editor_html_marks_fields_and_events` (AC-11, AC-3) and
  `ui_events_decode_to_commands` (AC-11, AC-2).

Integration — `liquers-lib/tests/ui_editor_binding.rs` (runner, memory store, `recipes.yaml`):
- `two_editors_share_a_recipe_query` (AC-9, AC-26)
- `on_demand_update_writes_only_on_update` (AC-26)
- `action_editor_writes_through_the_action_accessor` (AC-8)
- `binding_that_is_not_an_accessor_is_read_only` (AC-9, AC-13)

Commands to run:

```bash
cargo test -p liquers-core --lib pipeline::editor
cargo test -p liquers-lib --lib ui::editor egui::editor ui::web::editor
cargo test -p liquers-lib --test ui_editor_binding --test registry_export
cargo test -p liquers-lib --no-default-features --features webui --lib ui::web::editor
```

## Learning Log

- The query accessor's value is `Value::Query`; the action accessor's is a one-element
  `Value::Pipeline` whose index counts the resource (`ActionStep`), so an editor of kind `Action`
  edits element 0 of its own copy.
- `register_command!` already accepts `gui:` per argument (`head(state, n: i64 = 10 (gui:
  IntegerField))`), which the tests use instead of building metadata by hand.

# Phase 3: Examples & Use-cases — composite-variadic-arguments

Rewritten on 2026-11-25 for the scope of Phases 1 and 2 as revised that day. Written under the
recommended answers to Phase 1's open questions (the shape is a field of the argument;
`BTreeMap` and `serde_json::Map`; one element trait; widgets follow the component type; 2 to 4
components).

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Rename pairs | declaring and using tuples | AC-1, AC-4, AC-14 |
| 2 | Example | Layout data | a dictionary with JSON values | AC-9, AC-10, AC-17 |
| 3 | Example | A JSON value | the four forms | AC-15, AC-16 |
| 4 | Test | `plan::tests::shaped_variadic_*` | plan typing, counts, duplicate keys | AC-2, AC-3, AC-4, AC-8, AC-10, AC-16 |
| 5 | Test | `commands::tests::get_multiple_*` | retrieval of tuples, maps, JSON | AC-1, AC-7, AC-9, AC-11, AC-15, AC-17 |
| 6 | Test | `command_metadata::tests::shape_*` | serde and check | AC-5, AC-6, AC-14 |
| 7 | Test | `recipes::tests::shaped_variadic_override_*` | override forms | AC-7 |
| 8 | Test | `liquers-macro` `registration::tests::variadic_shape_*` | DSL and rejections | AC-5, AC-6, AC-14 |
| 9 | Test | `liquers-core/tests/composite_variadic.rs` | end to end through the macro | AC-1, AC-2, AC-3, AC-7, AC-9, AC-10, AC-12, AC-15, AC-16, AC-17 |
| 10 | Test | `liquers-mcp` `schema::tests::shaped_argument_*` | tool schema and input | AC-5 |
| 11 | Test | `liquers-lib` `ui::editor::tests::shaped_*` | editor rows, pairs, JSON form | AC-13, AC-14 |

## Example 1: Rename pairs

```rust
fn rename_pairs(state: &State<Value>, pairs: Vec<(String, String)>) -> Result<Value, Error> {
    Ok(Value::from(pairs.iter().map(|(a, b)| format!("{a}->{b}")).collect::<Vec<_>>().join(",")))
}
register_command!(cr, fn rename_pairs(state,
    pairs: Vec<(String, String)> multiple (label: "Renames", columns: ["Old name", "New name"])
) -> result)?;
```

`rename_pairs-a-x-b-y` gives `"a->x,b->y"`; `rename_pairs` gives `""` (AC-4);
`rename_pairs-a-x-b` fails at plan time, positioned at `b` (AC-3). The metadata has
`shape: {"tuples": ["string", "string"]}` and `columns: ["Old name", "New name"]` (AC-5, AC-14).
With `weights: Vec<(String, i64)> multiple`, `w-a-1-b-x` fails at plan time at `x` (AC-2).

## Example 2: Layout data

```rust
fn add(state: &State<Value>, position_word: String, reference_word: String,
       layout: BTreeMap<String, serde_json::Value>) -> Result<Value, Error> { /* … */ }
register_command!(cr, fn add(state, position_word: String, reference_word: String,
    layout: BTreeMap<String, serde_json::Value> multiple) -> result)?;
```

- `add-child-current-row-0-col-8-role-sidebar` gives `{col: 8, role: "sidebar", row: 0}` (AC-9).
- `add-child-current-row-0-row-1` fails at plan time at the second `row` (AC-10).
- `add-child-current-row` fails at plan time at `row` (AC-3).
- `add-child-current-flag-true` gives `flag: "true"`, a string: there are no booleans.
- Declared as `serde_json::Map<String, serde_json::Value>` the command gets the same (AC-17).

## Example 3: A JSON value

```rust
register_command!(cr, fn set_data(state, data: serde_json::Value multiple) -> result)?;
```

| Query | `data` |
|---|---|
| `set_data` | `null` |
| `set_data-5` | `5` |
| `set_data-abc` | `"abc"` |
| `set_data-` | `[]` |
| `set_data--1-a` | `[1, "a"]` |
| `set_data-1-a` | `{"1": "a"}` |
| `set_data-row-0-col-8` | `{"col": 8, "row": 0}` |
| `set_data-row-0-col` | a plan error at `col` (AC-16) |

## Edge and Error Cases

- **Link inside** (AC-7): `w-a-1-~X~more~E`, where `more` gives `["b", 2]`. Grouping happens
  after links are materialised; with `more` giving `["b"]`, retrieval fails naming `weights`.
- **Link in first place of a JSON value** (AC-16): nothing is checked at plan time; the form is
  decided from the materialised value at retrieval.
- **Override** (AC-7): `weights: [["a", 1], ["b", 2]]` or `["a", 1, "b", 2]`;
  `layout: {row: 0, col: 8}` for a dictionary; any JSON without nested containers for `json`.
- **Serialization** (AC-5): a plain list has no `shape` key, so `specs/command_registry.yaml` is
  byte-identical until a command adopts a shape.
- **Misdeclaration** (AC-6): `origin: (f64, f64)` without `multiple`, a five-component tuple, a
  `Vec` component and a `HashMap<i64, String>` are compile errors.
- **Custom element** (AC-12): `Point` implementing `FromParameterGroup` with two floats;
  `curve-0-0-1-2.5` gives two points.
- **Feature flags**: the egui and web editor views are feature-gated; the build matrix checks them.

## Test Plan

- Unit `liquers-core/src/plan.rs`: `shaped_variadic_types_each_position` (AC-2),
  `shaped_variadic_incomplete_element_is_positioned` (AC-3), `shaped_variadic_empty_is_allowed`
  (AC-4), `shaped_variadic_duplicate_key_is_positioned` (AC-10),
  `shaped_variadic_json_pairs_are_checked` (AC-16), `scalar_variadic_plan_unchanged` (AC-8).
- Unit `liquers-core/src/commands.rs`: `get_multiple_grouped_builds_pairs` (AC-1),
  `get_multiple_grouped_reports_remainder` (AC-7), `get_multiple_map_reads_json_scalars` (AC-9,
  AC-11), `get_multiple_json_forms` (AC-15), `get_multiple_map_collects_json_object` (AC-17).
- Unit `liquers-core/src/command_metadata.rs`: `shape_round_trips_and_list_is_omitted` (AC-5),
  `shape_requires_multiple_and_valid_components` (AC-6), `shape_columns_match_arity` (AC-14).
- Unit `liquers-core/src/recipes.rs`: `shaped_variadic_override_accepts_flat_nested_and_object`
  (AC-7).
- Unit `liquers-macro/src/registration.rs`: `variadic_shape_from_rust_type` (AC-5),
  `variadic_shape_rejections` (AC-6), `variadic_shape_columns` (AC-14).
- Integration `liquers-core/tests/composite_variadic.rs`: `pairs_from_flat_query` (AC-1),
  `typed_component_error_is_positioned` (AC-2), `incomplete_element_fails_planning` (AC-3),
  `link_elements_group_after_materialisation` (AC-7), `dictionary_from_pairs` (AC-9),
  `duplicate_key_fails_planning` (AC-10), `custom_element_type` (AC-12), `json_value_forms`
  (AC-15), `json_value_pair_errors` (AC-16), `json_object_argument` (AC-17).
- Unit `liquers-mcp/src/schema.rs`: `shaped_argument_schema` (AC-5).
- Unit `liquers-lib/src/ui/editor/tests.rs`: `shaped_tuple_rows_edit_whole_elements` (AC-13,
  AC-14), `shaped_map_pair_is_added_whole` (AC-13), `shaped_json_form_change_rewrites_items`
  (AC-13).
- Run: `cargo test -p liquers-core --lib`, `cargo test -p liquers-core --test
  composite_variadic`, `cargo test -p liquers-macro`, `cargo test -p liquers-mcp`,
  `cargo test -p liquers-lib --lib --tests`, `bash scripts/check-build-matrix.sh`.

The editor views (egui, web) are checked by the editor-core tests above and by the build matrix;
no pixel or browser test is added.

## Learning Log

- The flat plan is the key simplification. Nothing that walks `MultipleParameters` today needs
  to know about shapes.
- Plan-time checks are complete only for literals. A link defers them to retrieval (AC-7, AC-16).
- Moving the shape out of `ArgumentType` removed every exhaustive-match change from the design.

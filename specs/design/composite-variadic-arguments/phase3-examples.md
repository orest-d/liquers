# Phase 3: Examples & Use-cases — composite-variadic-arguments

Written under the recommended answers to Phase 1 questions 2-4.

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Rename pairs | declaring and using pairs | AC-1, AC-4 |
| 2 | Example | Typed pairs | per-component types and errors | AC-2, AC-3 |
| 3 | Test | `plan::tests::tuple_variadic_*` | plan grouping and errors | AC-2, AC-3, AC-4, AC-8 |
| 4 | Test | `commands::tests::get_multiple_grouped_*` | retrieval and run-time arity | AC-1, AC-7 |
| 5 | Test | `command_metadata::tests::tuple_*` | serde and check | AC-5, AC-6 |
| 6 | Test | `recipes::tests::tuple_variadic_override_*` | override shapes | AC-7 |
| 7 | Test | `liquers-macro` `registration::tests::variadic_tuple_*` | DSL and rejections | AC-5, AC-6 |
| 8 | Test | `liquers-core/tests/composite_variadic.rs` | end to end through the macro | AC-1, AC-2, AC-3, AC-7 |
| 9 | Test | `liquers-mcp` `schema::tests::tuple_argument_schema` | tool schema | AC-5 |

## Example 1: Rename pairs

```rust
fn rename_pairs(state: &State<Value>, pairs: Vec<(String, String)>) -> Result<Value, Error> {
    Ok(Value::from(pairs.iter().map(|(a, b)| format!("{a}->{b}")).collect::<Vec<_>>().join(",")))
}
register_command!(cr, fn rename_pairs(state, pairs: Vec<(String, String)> multiple) -> result)?;
```

`rename_pairs-a-x-b-y` → `"a->x,b->y"`; `rename_pairs` → `""` (AC-4). Both queries validate today
with a permissive stub (`--command rename_pairs`) and plan as a flat list of four (or zero)
`ParameterValue` elements, which is exactly the plan this design keeps. The grouping happens at
retrieval.

## Example 2: Typed pairs

```rust
register_command!(cr, fn w(state, weights: Vec<(String, i64)> multiple) -> result)?;
```

- `w-a-1-b-2` → `[("a", 1), ("b", 2)]`.
- `w-a-1-b-x` → plan error *x* is not an integer, positioned at column 9 (AC-2).
- `w-a-1-b` → plan error *argument `weights` takes elements of 2 parameters; 3 given*, positioned
  at `b` (AC-3).

## Edge and Error Cases

- **Link inside** (AC-7): `w-a-1-~X~more~E`, where `more` → `["b", 2]`. After expansion there are
  four elements, so the result is two pairs. With `more` → `["b"]`, `get_multiple_grouped` errs at
  run time, naming `weights`.
- **Override** (AC-7): a recipe `parameters: {weights: [["a", 1], ["b", 2]]}` or `["a", 1, "b",
  2]` → the same pairs; `[["a"]]` → a conversion error naming `weights`.
- **Serialization** (AC-5): `ArgumentInfo` with `Tuple([String, Integer])` round-trips through
  JSON and YAML as `{"Tuple": ["string", "int"]}`.
- **Feature flags**: the `egui` display arm is under `cfg(feature = "egui")`. The build matrix
  checks it.

## Test Plan

- Unit `liquers-core/src/plan.rs`: `tuple_variadic_types_each_position` (AC-2),
  `tuple_variadic_incomplete_group_is_positioned` (AC-3), `tuple_variadic_empty_is_allowed` (AC-4),
  `scalar_variadic_plan_unchanged` (AC-8; it compares with today's plan for
  `ns-pl/select_columns-date-amount`).
- Unit `liquers-core/src/commands.rs`: `get_multiple_grouped_builds_pairs` (AC-1),
  `get_multiple_grouped_reports_remainder` (AC-7).
- Unit `liquers-core/src/command_metadata.rs`: `tuple_argument_type_round_trips` (AC-5),
  `tuple_requires_multiple_and_scalar_components` (AC-6).
- Unit `liquers-core/src/recipes.rs`: `tuple_variadic_override_accepts_flat_and_nested` (AC-7).
- Unit `liquers-macro/src/registration.rs`: `variadic_tuple_generates_tuple_type` (AC-5),
  `variadic_tuple_rejects_arity_one_and_five`, `variadic_tuple_rejects_vec_component`,
  `tuple_without_multiple_is_rejected` (AC-6).
- Integration `liquers-core/tests/composite_variadic.rs`: `pairs_from_flat_query` (AC-1),
  `typed_component_error_is_positioned` (AC-2), `incomplete_group_fails_planning` (AC-3),
  `link_elements_group_after_expansion` (AC-7).
- Unit `liquers-mcp/src/schema.rs`: `tuple_argument_schema` (AC-5).
- Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib tuple_`, `cargo test -p liquers-core
  --test composite_variadic`, `cargo test -p liquers-macro`, `cargo test -p liquers-mcp`,
  `cargo test -p liquers-lib --lib --tests`, `bash scripts/check-build-matrix.sh`.

## Learning Log

- The flat plan is the key simplification. Nothing that walks `MultipleParameters` today (the
  interpreter, dependency recording, plan display) needs to know about tuples.
- The plan-time arity check is complete only without links. This is stated rather than hidden
  (AC-3 versus AC-7).

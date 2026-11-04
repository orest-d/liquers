# Phase 3: Examples & Use-cases — Pipelines

Written on 2026-11-03 under the pre-approval given after Phase 2 ("proceed all"). Core tests use
small test commands in the style of `liquers-core/tests/command_alias.rs`: `text`, `pick`, a
variadic `tag(prefix, items…)`, `join(other)`, a source `ticker` and `fail`. Only Example 1 needs
`liquers-lib` (polars). Queries were checked with `liquers-validate`.

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | The `sales/top_in` pipeline command | registry pipeline command mixed with a query | AC-1, AC-16, AC-17, AC-19 |
| 2 | Example | A recipe pipeline that runs before its query | structured value to a middle step | AC-15, AC-30 |
| 3 | Example | Building and editing a query with `QueryEditor` | chained editor, refusal, round trip | AC-26, AC-31, AC-32, AC-37 |
| 4 | Tests | `liquers-core/src/pipeline/*` unit tests | types, conversions, editors | see Test Plan |
| 5 | Tests | `liquers-core/tests/pipeline_*.rs` | planning, commands, recipes, values | see Test Plan |

## Example 1: a pipeline command in the registry

`sales/top_in` is the Phase 1 problem example, written as a bare element list (shorthand for
`{elements: […]}`):

```yaml
- { namespace: sales, name: top_in,
    arguments: [ {name: by, argument_type: string, default: !Value amount},
                 {name: n, argument_type: int, default: !Value 10},
                 {name: regions, argument_type: string, multiple: true} ],
    definition: !Pipeline { pipeline: [
      {action: from_csv, ns: pl},
      {action: is_in, ns: pl, arguments: {column: region, values: {placeholder: regions}}},
      {action: sort, ns: pl, arguments: {column: {placeholder: by}, ascending: false}},
      {action: head, ns: pl, arguments: {n: {placeholder: n}}} ] } }
```

`-R/data/sales.csv/-/ns-sales/top_in-amount-5-EU-US/ns-pl/tail-2` is planned as follows:
1. `GetAsset(data/sales.csv)`;
2. `Step::PipelineCommand{sales/top_in}`, whose body binds `regions = [EU, US]` (spliced, V4),
   `by = amount` and `n = 5`;
3. `pl/tail-2`.

The result equals the result of
`-R/data/sales.csv/-/ns-pl/from_csv/is_in-region-EU-US/sort-amount-f/head-5/tail-2`. It is
cached under the calling query. Editing the command's metadata invalidates that cache entry (AC-8).

## Example 2: a recipe pipeline

```yaml
- pipeline:
  - "-R/data/sales.csv/-/ns-pl/from_csv"
  - {action: is_in, ns: pl, arguments: {column: region, values: [EU, US]}}
  query: ns-pl/sort-amount-f/head-10/eu.csv
```

The pipeline runs first and the query continues from its state. The list reaches `is_in`, which
is not the last action. The filename `eu.csv` comes from the query. `Recipe::to_pipeline` gives
the pipeline's two elements followed by `ns-pl/sort-amount-f/head-10` and the filename. That
pipeline plans as `Recipe::to_plan` does, and it converts back to the query
`-R/data/sales.csv/-/ns-pl/from_csv/is_in-region-EU-US/sort-amount-f/head-10/eu.csv`.
Moving `ns-pl/sort…` in front of `-R/…` in the recipe's query would be refused: the query would
start with a resource and discard the pipeline's state.

## Example 3: the query editor

```rust
let cmr = env.command_metadata_snapshot();
let mut q = QueryEditor::new(cmr.clone());
q.resource("data/sales.csv", Read::Asset)?;
q.action("is_in")?.ns("pl")?.set("column", "region")?.push("EU")?.push("US")?;
assert_eq!(q.query().encode(), "-R/data/sales.csv/-/ns-pl/is_in-region-EU-US");
assert!(q.action("sort")?.ns("pl")?.placeholder("column", "by").is_err()); // AC-32: not representable
let p = PipelineEditor::from_query(&q.query(), cmr)?;                       // typed parameters
assert_eq!(Query::try_from(&p)?, q.query());                                // AC-26
```

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| A placeholder in a recipe pipeline | planning error naming it and the recipe | `recipe_pipeline_refuses_placeholders` |
| A recipe query starting with `-R/` after a pipeline | refused as a dead pipeline | `recipe_pipeline_before_resource_query_is_refused` |
| A pipeline command calling itself, directly or through another | `pipeline_cycle` naming the chain | `pipeline_command_cycle_is_reported` |
| `{state}` passed through two nested pipeline commands | the outer input reaches the inner argument | `input_state_passes_through_nested_commands` |
| NaN, ±inf and `0.1 + 0.2` as JSON values | survive JSON and query round trips exactly | `non_finite_and_exact_floats_round_trip` |
| A header level other than `-` | refused, naming the segment | `header_level_is_refused` |
| A key segment `my file.csv` in an edit | refused, editor unchanged | `editor_refuses_invalid_key_segment` |
| A command missing from the default namespaces, then `.ns("pl")` | resolves after `ns` | `action_editor_resolves_after_ns` |
| Registry changed after an editor was opened | the editor keeps its snapshot | `editor_keeps_registry_snapshot` |
| Delegate and Native planning disagree | the corpus test fails, naming the query | `plans_are_equal_in_both_modes` |

## Test Plan

**Unit, `liquers-core/src/pipeline/mod.rs`:**
- `pipeline_serde_round_trip_json_yaml_toml` (AC-21): every element and source kind survives JSON,
  YAML and TOML;
- `bare_list_is_a_pipeline` (AC-21);
- `unknown_element_key_names_the_index` (AC-21);
- `bind_replaces_every_placeholder` and `bind_reports_unbound_placeholder` (AC-38);
- `non_finite_and_exact_floats_round_trip` (AC-21).

**Unit, `liquers-core/src/pipeline/convert.rs`:**
- `every_corpus_query_converts` (AC-25);
- `round_trip_gives_canonical_form` (AC-26);
- `round_trip_is_idempotent` (AC-27);
- `query_origin_pipeline_converts_back` (AC-28);
- one test per rule, `not_representable_<rule>` for P1–P4, E1–E8, S1–S7 and N1–N4 (AC-29);
- `typed_scalars_keep_type_through_registry` (AC-25, AC-27);
- `namespace_placement_three_steps` (AC-27);
- `realm_segments_merge_and_nest` and `header_level_is_refused` (AC-40);
- `recipe_to_pipeline_concatenates` (AC-30);
- `parameter_value_to_pipeline` (AC-18).

**Unit, `liquers-core/src/pipeline/editor.rs`:**
- `editor_validates_each_edit` and `editor_refuses_invalid_key_segment` (AC-31);
- `editor_errors_carry_position_and_query` and `edited_element_position_from_reparse` (AC-41);
- `query_editor_refuses_non_representable_edit` and `query_editor_query_is_infallible` (AC-32);
- `editor_exposes_metadata_and_argument_info` and `editor_keeps_registry_snapshot` (AC-37);
- `action_editor_chain` and `action_editor_resolves_after_ns` (AC-31, AC-37);
- `editor_accepts_empty_parameter` (AC-36).

**Unit, elsewhere:**
- `liquers-core/src/escape.rs`: `predicates_match_parser` (AC-34);
- `liquers-core/src/query.rs`: `check_encodable_names_first_bad_token` (AC-33);
- `liquers-core/src/value.rs`: `pipeline_value_type_info_and_conversions` (AC-39);
- `liquers-template`, `src/query/instantiate.rs`: `validators_use_core_predicates` (AC-34);
- `liquers-web/tests/encode.rs`: `web_encode_param_accepts_the_empty_string` (AC-36), replacing
  `web_encode_param_refuses_the_empty_string`.

**Integration, `liquers-core/tests/pipeline_planning.rs`:**
- `plans_are_equal_in_both_modes` (AC-10): every query of the parser corpus and of
  `tests/command_alias.rs`, planned `Delegate` and `Native`;
- `positions_reach_plan_steps` (AC-41);
- `value_and_resource_elements_set_state` (AC-5);
- `placeholder_outside_definition_is_error` (AC-11);
- `query_then_pipeline_then_query` (AC-16);
- `positional_then_named_arguments` (AC-17);
- `nested_pipeline_argument_representable_and_not` (AC-18);
- `variadic_rules_v1_to_v9` (AC-19);
- `list_for_single_argument` (AC-23).

**Integration, `liquers-core/tests/pipeline_commands.rs`:**
- `pipeline_command_runs_its_pipeline` (AC-1);
- `arguments_in_any_order` (AC-2);
- `defaults_and_missing_arguments` (AC-3);
- `input_state_as_argument` and `input_state_passes_through_nested_commands` (AC-4);
- `registration_issues_are_reported` (AC-6);
- `elements_resolve_without_caller_ns` and `pipeline_command_cycle_is_reported` (AC-7);
- `result_cached_and_invalidated_by_metadata` (AC-8);
- `injection_and_payload_forwarded` (AC-20);
- `registry_export_round_trip` (AC-21).

**Integration, `liquers-core/tests/recipe_pipelines.rs`:**
- `recipe_pipeline_runs_before_query` (AC-15);
- `recipe_pipeline_refuses_placeholders` (AC-11, AC-15);
- `recipe_pipeline_before_resource_query_is_refused` (AC-15);
- `recipe_overrides_need_a_query` (AC-15);
- `existing_recipes_unchanged` (AC-15);
- `recipe_to_pipeline_plans_as_recipe` (AC-30).

**Integration, `liquers-core/tests/pipeline_values.rs`:**
- `pipeline_value_stored_and_read_back` (AC-39).

**Integration, `liquers-core/tests/validate_pipelines.rs`:**
- `validate_pipeline_file`, `validate_emit_query_and_pipeline`, `validate_build_from_shell_args`
  and `validate_not_representable_exit_1` (AC-35).

**Integration, `liquers-lib/tests/pipeline_polars.rs`:**
- `top_in_matches_long_query` (Example 1: AC-1, AC-16, AC-17, AC-19);
- `recipe_pipeline_eu_csv` (Example 2: AC-15).

**Commands:** `cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-lib --test
pipeline_polars`, `cargo test -p liquers-template`, and the liquers-web Node loop for AC-36.

## Learning Log

- A pipeline command body is a `Pipeline` written as a bare list. The struct form is needed only
  for a realm or a filename.
- Placeholders bound to runtime values (`{state}`) are resolved by the calling
  `PipelineCommand` step, so a body reads only its own `InputState`.
- Phase 1's recipe example predated Q26 and was updated to the "pipeline before query" form.

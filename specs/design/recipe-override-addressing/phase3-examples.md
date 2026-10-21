# Phase 3: Examples & Use-cases — recipe-override-addressing

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | List argument of a middle command | the problem example, fixed | AC-1, AC-2 |
| 2 | Example | MCP tool as a recipe with `@key` | key hole replaces `fill_key` | AC-5, AC-6, AC-7 |
| T-1 | Test | `liquers_core::plan::tests::argument_address_parse_and_encode` | grammar round trip and malformed names | AC-1, AC-2, AC-5, AC-8 |
| T-2 | Test | `liquers_core::recipes::tests::recipe_overrides_an_earlier_action_by_ordinal` | `1.values` and `1.1` patch the middle action | AC-1 |
| T-3 | Test | `liquers_core::recipes::tests::recipe_overrides_an_earlier_action_by_name` | unique name works; duplicate is `ParameterError` listing ordinals | AC-2 |
| T-4 | Test | `liquers_core::recipes::tests::unqualified_override_still_targets_the_last_action` | same plan and same error text as before | AC-3 |
| T-5 | Test | `liquers_core::recipes::tests::override_address_errors` | out of range, absent, injected, alias head, `@key` in `links` | AC-8 |
| T-6 | Test | `liquers_core::query::tests::with_key_suffix_contract` | `fill_key`'s refusals and no-op cases | AC-6 |
| T-7 | Test | `liquers_core::recipes::tests::key_hole_fills_before_planning` | `get_query`, `key`, `filename`, `to_plan` see the filled key; `query_as_written` does not | AC-5 |
| T-8 | Test | `liquers_core::plan::tests::cut_declines_over_an_overridden_action` | finalized plan keeps the overridden action inline; `Info` line | AC-4 |
| T-9 | Test | `liquers-core/tests/recipe_override_addressing.rs` `earlier_override_is_not_served_from_a_cached_prefix` | end to end: the same prefix evaluated plain and overridden gives two different values | AC-4 |
| T-10 | Test | `liquers-template/tests/render.rs` `non_scalar_fills_an_earlier_action` | template emits `1.1`; ordinal agrees with the plan for a query with `ns`, `v`, `q` | AC-7 |
| T-11 | Test | `liquers-mcp/src/server.rs` `key_argument_fills_through_the_recipe` | tool call with `key` builds `@key`; injection refused | AC-6, AC-7 |
| T-12 | Test | `liquers-axum/src/api_core/overrides.rs` `qualified_text_argument_converts_by_type` | `?1.n=3` converts to `int` without a warning | AC-1, AC-7 |

## Example 1: List argument of a middle command

`reports/recipes.yaml` (all queries validated with `liquers-validate`):

```yaml
recipes:
  - query: -R/data/sales.csv/-/ns-pl/from_csv/is_in-region/head-10/eu.csv
    arguments:
      "1.values": [EU, US]        # or is_in.values, or 1.1
```

Plan: `GetAsset(data/sales.csv)`, `from_csv`, `is_in(region, values=[EU, US])` (override),
`head(10)`. Finalized: no boundary covers `is_in`; the walk logs
`Predecessor boundary expanded at '-R/data/sales.csv/-/ns-pl/from_csv/is_in-region': it contains an
action a recipe argument overrides` and cuts at `from_csv`, which is unaffected and stays cacheable.
`Step::Action::query` is `None` for `is_in` and `head`.

## Example 2: MCP tool as a recipe with `@key`

Tool `read` declared as `-R/notes/-/ns-mem/read` with a key argument. A call with
`{"key": "ideas/plan.md"}` builds:

```rust
let mut recipe = Recipe::new("-R/notes/-/ns-mem/read".into(), String::new(), String::new())?;
recipe.cwd = set.cwd.clone();
recipe.arguments.insert("@key".into(), Value::String("ideas/plan.md".into()));
// recipe.get_query()?.encode() == "-R/notes/ideas/plan.md/-/ns-mem/read"
```

`{"key": "x/-/ns-mem/write-y"}` fails to plan: `an element begins with '-'`.

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| `values` (unqualified) on the problem query | today's `ParameterError … last action` | T-4 |
| `head.n` where `head` occurs twice | `ParameterError`: "action 'head' occurs at ordinals 1 and 3" | T-3 |
| `5.n` on a three-command query | `ParameterError`: "the query has 3 commands" | T-5 |
| `1.9` beyond the command's parameters, no `multiple` | `ParameterError` | T-5 |
| `@key` = `"/abs"`, `"a//b"`, `".."`, `"-R"`, `42` | refused, nothing evaluated | T-6, T-7, T-11 |
| `@key` = `""` or `"/"` | no-op (folder tool) | T-6 |
| `@key.1` on a query with one resource segment | `ParameterError` | T-5 |
| override on an earlier action inside a stateful application | plan is expanded anyway; no change | T-8 |
| a link override on an earlier action | dependency analysis sees the link (it is a parameter) | T-9 |

## Test Plan

- Unit, `liquers-core/src/plan.rs` `mod tests`: T-1, T-8 (registry built in code like
  `recipe_overrides_a_variadic_argument`; T-8 freezes with `freeze_cwd(None)` then `cut_predecessor`).
- Unit, `liquers-core/src/recipes.rs` `mod tests`: T-2…T-5, T-7; `liquers-core/src/query.rs`: T-6
  (the three `fill_key_*` cases moved from `liquers-mcp`, plus segment 1).
- Integration, new `liquers-core/tests/recipe_override_addressing.rs`: T-9 — environment with a
  memory store and a recipe provider as in `tests/async_hellow_world.rs`; commands `seed`
  (returns 1), `add(state, n: i32)`, `double(state)`; evaluate `seed/add-1/double` (→ 4), then a
  recipe `seed/add-1/double` with `{"1.n": 10}` (→ 22, not 4 from the cached `seed/add-1`).
- `liquers-template`: T-10, and invert `instantiate.rs` test at line 453 and `tests/render.rs:67`
  (they assert the old refusal).
- `liquers-mcp`: T-11; `liquers-axum`: T-12.
- Run: `cargo test -p liquers-core --lib plan recipes query`, `cargo test -p liquers-core --test recipe_override_addressing`,
  `cargo test -p liquers-template`, `cargo test -p liquers-mcp`, `cargo test -p liquers-axum --lib overrides`.

## Learning Log

- The last-action rule was hiding a cache-soundness invariant: a predecessor boundary is keyed by
  query text, and only text-describable patches are safe inside one. Worth a pitfall row in DOC-08.
- Recipe arguments already flow through every transport as `HashMap<String, Value>`; extending the
  key grammar, not the struct, is what keeps bindings untouched.

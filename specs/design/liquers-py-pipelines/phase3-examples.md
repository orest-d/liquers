# Phase 3: Examples & Use-cases — liquers-py-pipelines

## Overview Table

| # | Kind | Name | Scenarios |
|---|---|---|---|
| 1 | Example | Evaluate and convert a pipeline | AC-1, AC-3 |
| 2 | Example | Edit a query step by step; declare a pipeline command | AC-5, AC-7, AC-8 |
| 3 | Test | `liquers-py/tests/test_PIPELINE_evaluate.py` | AC-1, AC-2, AC-9 |
| 4 | Test | `liquers-py/tests/test_PIPELINE_convert.py` | AC-3, AC-4, AC-10 |
| 5 | Test | `liquers-py/tests/test_PIPELINE_commands.py` | AC-5, AC-6 |
| 6 | Test | `liquers-py/tests/test_PIPELINE_editors.py` | AC-7, AC-8 |

**Validated** (`liquers-validate`, all `Ok`):
1. The problem pipeline (`--pipeline`, YAML of Example 1): `encoded`
   `-R/data/sales.csv/-/ns-pl/from_csv/is_in-region-EU-US/head-5`, which also validates as a query.
2. `-R/data/sales.csv/-/top_in-5-EU-US` (`--command top_in`).

Fixture: `data/sales.csv` with columns `region`, `amount` and rows in `EU`, `US`, `ASIA`. The
`polars` Python package is installed in the test environment (the frame is compared with
`frame.equals`).

## Example 1: evaluate and convert a pipeline

```python
import pytest
import liquers as lq
env = lq.Environment()
env.store_set("data/sales.csv", b"region,amount\nEU,1\nUS,2\nASIA,3\n")
top = [
    "-R/data/sales.csv/-/ns-pl/from_csv",
    {"action": "is_in", "ns": "pl", "arguments": {"column": "region", "values": ["EU", "US"]}},
    {"action": "head", "ns": "pl", "parameters": [5]},
]
q = "-R/data/sales.csv/-/ns-pl/from_csv/is_in-region-EU-US/head-5"
assert env.evaluate(top).equals(env.evaluate(q))                      # AC-1 (apply_queued)
p = lq.Pipeline(top)
assert p.to_query(env).encode() == q                                  # AC-3
assert lq.Pipeline.from_query(q).to_query(env).encode() == q          # canonical, stable
with pytest.raises(lq.ParameterError, match="P1"):                     # AC-4
    lq.Pipeline([{"value": 1}]).to_query(env)
```

## Example 2: edit a query; declare a pipeline command

```python
e = env.query_editor()
e.resource("data/sales.csv").action("from_csv", ns="pl")
a = e.action("is_in", ns="pl")
assert [i.name for i in e.unset_arguments(a.index)] == ["column", "values"]     # AC-8
a.set("column", "region").push("EU").push("US")
with pytest.raises(lq.ParameterError):                                          # AC-7
    a.set("colum", "x")
assert e.query().encode() == "-R/data/sales.csv/-/ns-pl/from_csv/is_in-region-EU-US"

body = [
    {"action": "from_csv", "ns": "pl"},
    {"action": "is_in", "ns": "pl", "arguments": {"column": "region", "values": {"multiple": [{"placeholder": "regions"}]}}},
    {"action": "head", "ns": "pl", "parameters": [{"placeholder": "n"}]},
]
env.pipeline_command("top_in", body, arguments=["n", lq.ArgumentInfo.from_json('{"name": "regions", "multiple": true}')])
assert env.evaluate("-R/data/sales.csv/-/top_in-5-EU-US").equals(env.evaluate(q))   # AC-5
d = env.command_metadata("top_in").definition
assert d.kind == "pipeline" and d.pipeline == lq.Pipeline(body)
```

Placeholders are written `{"placeholder": name}` (`PIPELINES.md` §4). The variadic `regions`
binds the remaining parameters `EU`, `US`, spliced into `values` by `{"multiple": …}` (§3).

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| A pipeline whose step fails | the same error type as its query | `test_failing_step_raises_like_its_query` |
| `{"a": [1, 2], "b": None}` passed to a Python command's `any` argument | received unchanged | `test_structured_argument_arrives_whole` |
| A non-representable pipeline (a `{"value": …}` element) evaluated inside a command | evaluates via `apply_ad_hoc` (Decision 3) | `test_non_representable_in_a_command` |
| `get_asset(pipeline)` | `TypeError` | `test_get_asset_refuses_a_pipeline` |
| A `datetime` literal; a `dict` containing itself; `2**64` | `lq.ConversionError` naming the path | `test_only_json_in_json_slots` |
| A body with an undeclared placeholder; a step command that does not exist | the typed error; no `top_in` | `test_bad_pipeline_command_is_refused` |
| A multi-step `action(…, ns=…)` whose second step fails | editor unchanged | `test_refused_edit_changes_nothing` |
| `make` returns `lq.Pipeline(top)`; `env.evaluate("make")`; `env.evaluate_state("make").metadata.type_identifier()`; `env.recipes().recipe(key)` over a recipe with `pipeline:`; a default set with `env.import_metadata` (`default: {pipeline: …}`) | an equal `lq.Pipeline` back; `"Pipeline"`; `Recipe.pipeline` and `ArgumentInfo.default` are `lq.Pipeline` | `test_pipeline_values_keep_their_type` |
| A top-level pipeline with a relative key (`[{"resource": "x.csv"}]` under `-R-cwd/data/`) | planned as its query is | `test_relative_key_pipeline` |
| Inside a command, `context.evaluate([{"value": 1}, {"resource": "x.csv"}])` | `lq.NotSupportedError` naming the relative key (`CONTEXT-APPLY-AD-HOC-SKIPS-RELATIVE-PIPELINE-KEYS`) | `test_relative_key_in_a_command_is_refused` |
| `KeyboardInterrupt` during `env.evaluate(top)` whose step runs the conftest `slow` | the asset ends `Cancelled`; `slow` saw `context.is_cancelled()` | `test_interrupt_cancels_the_pipeline` |
| `lq.Pipeline("-R/x")`; `lq.Pipeline(42)` | `TypeError` (a query string is `Pipeline.from_query`) | `test_constructor_takes_a_list` |
| `float("nan")`, `float("inf")`, `-inf` in a JSON slot | written `"NaN"`, `"inf"`, `"-inf"`, as the reference serializer does | `test_non_finite_floats` |
| An `ActionEditor` kept after its owner's step was removed | `lq.ParameterError` naming the index; editor unchanged | `test_stale_action_editor` |
| Two threads editing one editor while a third holds the GIL | no deadlock within 5 s; both edits applied | `test_editor_threads` |

## Test Plan

- `test_PIPELINE_evaluate.py`: `test_pipeline_evaluates_as_its_query` (AC-1: `evaluate`,
  `evaluate_state`, `evaluate_async`); `test_failing_step_raises_like_its_query` (AC-1);
  `test_structured_argument_arrives_whole` (AC-2); `test_pipeline_values_keep_their_type` (AC-9);
  `test_non_representable_in_a_command`, `test_get_asset_refuses_a_pipeline`,
  `test_relative_key_pipeline` (AC-1); edge cases, no AC: `test_relative_key_in_a_command_is_refused`,
  `test_interrupt_cancels_the_pipeline`.
- `test_PIPELINE_convert.py`: `test_round_trip` (AC-3, the problem query and one with a link
  parameter); `test_not_representable_names_the_rule` (AC-4, rules P1 and S7);
  `test_only_json_in_json_slots` (AC-10); edge cases, no AC: `test_constructor_takes_a_list`,
  `test_non_finite_floats`.
- `test_PIPELINE_commands.py`: `test_pipeline_command_runs_as_the_long_query` (AC-5, also after a
  rebuild; `definition.pipeline == lq.Pipeline(body)`; a second command whose `is_in` writes
  `values: {"placeholder": "regions"}` gives the same result); `test_bad_pipeline_command_is_refused` (AC-6).
- `test_PIPELINE_editors.py`: `test_refused_edit_changes_nothing` (AC-7);
  `test_editor_exposes_the_registry` (AC-8); edge cases, no AC: `test_stale_action_editor`,
  `test_editor_threads`.
- Run: `bash scripts/test-liquers-py.sh`.

## Learning Log

- A recipe holding only a pipeline is the existing way to evaluate a pipeline through the asset
  harness, at the top level (`apply_queued`) and in a command (`Context::apply_ad_hoc`).
- The editors' clone-then-commit gives Python atomic multi-step calls for free, because the core
  editors are `Clone`.

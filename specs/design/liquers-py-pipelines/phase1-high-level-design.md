# Phase 1: High-Level Design — liquers-py-pipelines

## Purpose

Let Python code write a query as data (a list of elements whose arguments are any JSON value),
run it, convert it to and from query text, edit it with the registry's checks, and register a
pipeline command without writing a Python function. This is the `PIPELINE` feature of
`LANGUAGE-INTEGRATION_GUIDE.md` (§PIPELINE, reference `PIPELINES.md`), for `VISION.md`'s Python
data-science use case and for code that assembles queries programmatically. It is milestone M6 of
the liquers-py plan, with [`liquers-py-templates`](../liquers-py-templates/DESIGN.md) and
[`liquers-py-data-nodes`](../liquers-py-data-nodes/DESIGN.md).

## Design Readiness

- **Readiness:** `needs-decision` (questions 1, 2 and 6 change the public API or another design)
- **Automatic fixing:** `not-eligible`: an `L` feature with new public Python classes.
- **Leading issue:** `PIPELINES-NOT-IN-LANGUAGE-BINDINGS` (P2 L), its Python half.
- **Explanation:** the core API exists and is tested (`composite-command-definition`, complete):
  `Pipeline` with serde, `try_from(&Query)` and `to_query_with`, `interpreter::evaluate_pipeline`,
  `CommandRegistry::register_pipeline`, and `PipelineEditor` / `QueryEditor`. This design adds the
  Python surface over that API. The JSON form is the bridge, so no rule is reimplemented in Python.
- **Open questions:**
  1. **Open design: a dedicated class.** Recommended: a frozen `lq.Pipeline` wrapping the core
     `Pipeline` (`from_json` / `to_json`, `to_list()` for the native structure, `==`, `repr`),
     *and* every entry point also accepts the native `list` / `dict` form. A `Value::Pipeline`
     reaches Python as `lq.Pipeline`, and an `lq.Pipeline` returned by a command crosses back as
     `Value::Pipeline`. Today a pipeline value and `Recipe.pipeline` arrive as a `dict`
     (`SimpleValue::Pipeline` in `liquers-py/src/value.rs`), and a `dict` returned by a command
     becomes `Value::Object`, so the type is lost on the way back. Changing both to `lq.Pipeline`
     breaks nothing that is published, because no release exists yet. Alternative: native
     structures only, with the type lost on return.
  2. **Open design: the editors.** Recommended: wrap `QueryEditor` and `PipelineEditor` as
     `lq.QueryEditor` / `lq.PipelineEditor`, built from an environment (`env.query_editor()`, over
     `command_metadata_snapshot`). They mutate like Rust's: each step returns the editor for
     chaining, and a refused step raises the typed error and leaves the editor unchanged
     (PIPELINE05). The alternative is building a native structure and converting it. That gives
     no per-step checks and no `ArgumentInfo` for an editor UI. This answers
     `QUERY-BUILDER-NOT-IN-BINDINGS` for Python. Its design `query-builder-in-bindings` targets a
     `QueryBuilder` that no longer exists in `liquers-core`, so see question 6.
  3. **Proposed resolution: the not-representable error.** `to_query` raises
     `lq.ParameterError`, carrying the core message, which names the rule and the path
     (`Error::not_representable`). No separate Python class is added.
  4. **Proposed resolution: Python objects in JSON slots.** A literal value or placeholder binding
     must be JSON-compatible (`None`, `bool`, `int` within `VALUE`'s range, `float`, `str`, and
     `list` / `dict` with `str` keys). NaN and ±inf become the reference's strings. Anything else
     raises `ConversionError` when the pipeline is built, naming its path. A `py.Object` never
     reaches a JSON slot. To pass one, a link or a nested pipeline supplies it.
  5. **Open design: entry points.** Recommended: `evaluate`, `evaluate_state` and their `_async`
     forms accept a pipeline as well as a query. A `str` stays a query. They run
     `interpreter::evaluate_pipeline` in a temporary asset, which is never cached, as the core
     does. `get_asset` stays query-only, because a pipeline has no asset key; the caller converts
     to a query for a cached asset. Inside a command, `context.evaluate(pipeline)` accepts only a
     representable pipeline and evaluates its query, so the dependency is recorded. A
     non-representable one raises `NotSupported`, because `Context` has no `evaluate_pipeline`.
     That gap is filed as a core issue in Phase 2 unless Phase 2 adds the method.
  6. **Open design: scope split.** Recommended: this design takes the Python half of
     `PIPELINES-NOT-IN-LANGUAGE-BINDINGS` and of `QUERY-BUILDER-NOT-IN-BINDINGS`. At Phase 5 both
     are narrowed to `liquers-web`, or replaced by `WEB-…` issues. `query-builder-in-bindings`
     becomes `superseded` for Python, and its JavaScript half is re-based on `QueryEditor`.

## Problem Example

```python
import liquers as lq
env = lq.Environment()        # a store holding data/sales.csv (region, amount)

top = [
    "-R/data/sales.csv/-/ns-pl/from_csv",
    {"action": "is_in", "ns": "pl", "arguments": {"column": "region", "values": ["EU", "US"]}},
    {"action": "head", "ns": "pl", "parameters": [5]},
]
frame = env.evaluate(top)
```

**Today** `evaluate` raises `GeneralError` ("a query must be a str or an lq.Query, not list",
`to_query` in `liquers-py/src/environment.rs`). A Python program can build a pipeline only as a
YAML file that the core reads. **After** `frame` equals
`env.evaluate("-R/data/sales.csv/-/ns-pl/from_csv/is_in-region-EU-US/head-5")`, and
`lq.Pipeline(top).to_query(env)` encodes as that query. Both the pipeline and the query validate
(`liquers-validate --pipeline`, `Ok`, `encoded` as above).

## Scope and Acceptance Criteria

- **AC-1** A pipeline evaluates as its query
  WHEN the problem example's `top` is evaluated with `evaluate`, `evaluate_state` and `evaluate_async`
  THEN each gives the value (or a state with the metadata) that evaluating its query gives (PIPELINE01)
- **AC-2** A structured argument arrives whole
  WHEN a pipeline passes `{"a": [1, 2], "b": None}` in `arguments` to a Python command's `any` argument
  THEN the command receives that `dict` unchanged (PIPELINE02)
- **AC-3** Pipeline and query convert both ways
  WHEN `lq.Pipeline.from_query(q)` is converted back with `to_query(env)`, for the problem query and a query with a link parameter
  THEN the result is the query's canonical form, and converting again changes nothing
- **AC-4** A non-representable pipeline names the rule
  WHEN `to_query` is called on a pipeline with a `{"value": …}` element, and on one with a placeholder outside a definition
  THEN it raises `lq.ParameterError` whose message names rule P1, or S7, and the element's path (PIPELINE03)
- **AC-5** A pipeline command registered from Python runs as the long query
  WHEN `env.pipeline_command("top_in", body, arguments=[…])` registers a body with placeholders `regions` and `n`, and `-R/data/sales.csv/-/top_in-5-EU-US` is evaluated (arguments `n`, then `regions` as the variadic last)
  THEN it gives what the expanded query gives, and `command_metadata("top_in").definition` shows the pipeline (PIPELINE04)
- **AC-6** A bad pipeline command is refused whole
  WHEN the body names a placeholder that is not a declared argument, or a step command that does not exist
  THEN registration raises the typed error naming it, and no `top_in` is registered
- **AC-7** A refused edit changes nothing
  WHEN a `QueryEditor` built from the problem query is given an unknown argument name, then a value that would make it unrepresentable
  THEN each step raises `lq.ParameterError`, and `editor.query()` still encodes the problem query (PIPELINE05)
- **AC-8** Editors expose the registry's view
  WHEN an action is added with `editor.action("is_in", ns="pl")`
  THEN the editor returns its `CommandMetadata` and the `ArgumentInfo` of `column` and `values`, and `unset_arguments` lists what is still missing
- **AC-9** Pipeline values keep their type across the boundary
  WHEN a query's result is `Value::Pipeline`, a Python command returns an `lq.Pipeline`, and a recipe with a `pipeline` is read through `env.recipes`
  THEN Python receives `lq.Pipeline` in the first and third cases, and the second result's state reports type `Pipeline`
- **AC-10** Only JSON reaches a JSON slot
  WHEN a pipeline is built with a `datetime` as a literal argument
  THEN it raises `lq.ConversionError` naming the path of the value

Non-goals: pipelines in `liquers-web`; a new core planning mode; evaluating a non-representable
pipeline inside a command (question 5); placeholders in query text; typed `lq.PipelineElement`
classes (elements stay JSON-shaped `dict`s).

## Core Interactions

Query and Plan: `Pipeline` conversions, `evaluate_pipeline`. Commands: `register_pipeline`, which
also covers the retained declarations, so a rebuild re-registers the command. Value types:
`Value::Pipeline` in both directions of the value bridge. Bindings: `lq.Pipeline`, the two editor
classes, and new `Environment` and `Context` members.

## Crate Placement

`liquers-py` only: a new `src/pipeline.rs`, plus changes to `environment.rs`, `context.rs`,
`value.rs` and `recipes.rs`, the stubs and `liquers/__init__.py`. A core addition
(`Context::evaluate_pipeline`) would go in `liquers-core` in its own PR, only if Phase 2 decides
it is needed.

## Documentation Intent

- Reference: `PYTHON_INTEGRATION.md`, a new §Pipelines (class, entry points, conversion,
  editors, registration, errors).
- Guide: a "Pipelines" chapter in `PYTHON_GUIDE.md`.
- Other documents: the `liquers-py/README.md` conformance row; `LANGUAGE-INTEGRATION_GUIDE.md`
  §PIPELINE and §6 Python (what liquers-py does); `PIPELINES.md` §Related; `specs/README.md`.
- Documents to update: `PYTHON_INTEGRATION.md` §5, where pipelines arrive as a `dict`; the user
  guide chapters that `docs_index.py chapters-for` lists.
- API docs: the module doc of the new `liquers-py/src/pipeline.rs`, plus `value.rs` and
  `environment.rs` for the new members; docstrings on every new Python name.

## Design Dependencies

- `composite-command-definition` (complete): **requires**. It provides every core API used here.
- `PIPELINES-NOT-IN-LANGUAGE-BINDINGS`: **implements** its Python half (question 6).
- `QUERY-BUILDER-NOT-IN-BINDINGS` / `query-builder-in-bindings`: **overlaps**, and is superseded
  for Python if question 6 is taken.
- `liquers-py-templates`, `liquers-py-data-nodes`: siblings in M6, with no dependency either way.
- `liquers-py-stubs` (complete): **required-by**. The stubs must declare the new names.

## References

- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §PIPELINE; `specs/reference/PIPELINES.md`;
  `specs/guides/PIPELINE_GUIDE.md`
- `liquers-core/src/pipeline/` (`mod.rs`, `convert.rs`, `editor.rs`), `liquers-core/src/interpreter.rs`
  `evaluate_pipeline`, `liquers-core/src/commands.rs` `register_pipeline`

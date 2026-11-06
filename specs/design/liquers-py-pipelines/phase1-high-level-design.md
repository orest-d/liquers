# Phase 1: High-Level Design — liquers-py-pipelines

## Purpose

Let Python code write a query as data (a list of elements with JSON arguments), run it, convert it
to and from query text, edit it with the registry's checks, and register a pipeline command with no
Python function: the `PIPELINE` feature of `LANGUAGE-INTEGRATION_GUIDE.md` (reference
`PIPELINES.md`), for `VISION.md`'s Python data-science use case. Milestone M6 of the liquers-py
plan, with [`liquers-py-templates`](../liquers-py-templates/DESIGN.md) and
[`liquers-py-data-nodes`](../liquers-py-data-nodes/DESIGN.md).

## Design Readiness

- **Readiness:** `ready`: Phases 2-4 written and reviewed (2026-11-04); no user decision is open (Orest decided questions 1, 2, 5 and 6 at the gate;
  Decision 3, a refinement logged as an assumption, is reported with the pre-implementation summary)
- **Automatic fixing:** `not-eligible`: an `L` feature with new public Python classes.
- **Leading issue:** none open. Source issue
  `PIPELINES-NOT-IN-LANGUAGE-BINDINGS` (P2 L), narrowed to Python; its JavaScript half is
  `WEB-CANNOT-USE-PIPELINES`, filed with this design and since closed by `web-pipelines`.
- **Explanation:** the core API exists and is tested (`composite-command-definition`, complete):
  `Pipeline` with serde, `try_from(&Query)` and `to_query_with`, `interpreter::evaluate_pipeline`,
  `CommandRegistry::register_pipeline`, and `PipelineEditor` / `QueryEditor`. This design adds the
  Python surface over that API. The JSON form is the bridge, so no rule is reimplemented in Python.
- **Open questions:**
  1. **Decided (Decision 1): a dedicated class.** Taken as recommended: a frozen `lq.Pipeline` wrapping the core
     `Pipeline` (`from_json` / `to_json`, `to_list()` for the native structure, `==`, `repr`),
     *and* every entry point also accepts the native `list` / `dict` form. A `Value::Pipeline`
     reaches Python as `lq.Pipeline`, and an `lq.Pipeline` returned by a command crosses back as
     `Value::Pipeline`. So does a `CommandParameterValue::Pipeline` default in command metadata.
     Today a pipeline value and `Recipe.pipeline` arrive as a `dict`
     (`SimpleValue::Pipeline` in `liquers-py/src/value.rs`), and a `dict` returned by a command
     becomes `Value::Object`, so the type is lost on the way back. Changing both to `lq.Pipeline`
     breaks nothing that is published, because no release exists yet. Alternative: native
     structures only, with the type lost on return.
  2. **Decided (Decision 1): the editors.** Taken as recommended: wrap `QueryEditor` and `PipelineEditor` as
     `lq.QueryEditor` / `lq.PipelineEditor`, built from an environment (`env.query_editor()`,
     `env.pipeline_editor()`). Each takes `command_metadata_snapshot` when it is created, so a
     command registered later is unknown to it, and the docs say so. They mutate like Rust's: each
     step returns the editor for chaining, and a refused step raises the typed error and leaves the
     editor unchanged (PIPELINE05). The core `ActionEditor` borrows its editor, so it cannot be a
     Python class. `lq.ActionEditor` instead holds the editor and the element's index, and calls
     `edit_action(i)` for each step. A Python call that is several core steps
     (`action("is_in", ns="pl")`) runs on a clone of the editor (it is `Clone`) and is committed
     only when every step succeeds. The alternative is building a native structure and
     converting it. That gives no per-step checks and no `ArgumentInfo` for an editor UI. This
     answers `PY-HAS-NO-VALIDATING-QUERY-EDITOR`. The older design `query-builder-in-bindings`
     targets a `QueryBuilder` that no longer exists in `liquers-core`, so see question 6.
  3. **Proposed resolution: the not-representable error.** `to_query` raises
     `lq.ParameterError`, carrying the core message, which names the rule and the path
     (`Error::not_representable`). No separate Python class is added.
  4. **Proposed resolution: Python objects in JSON slots.** A literal value or placeholder binding
     must be JSON-compatible (`None`, `bool`, `int` within `VALUE`'s range, `float`, `str`, and
     `list` / `dict` with `str` keys). NaN and ±inf become the reference's strings. Anything else
     raises `ConversionError` when the pipeline is built, naming its path. That needs a walker of
     its own (`json_slot`, Phase 2): `py_to_simple` returns `None` for an unconvertible object and tracks no path. A
     `py.Object` never reaches a JSON slot. To pass one, a link or a nested pipeline supplies it.
  5. **Decided (Decision 1), refined by Decision 3: entry points.** `evaluate`, `evaluate_state`
     and their `_async` forms accept a pipeline as well as a query. A `str` stays a query. A
     pipeline is evaluated as an ad-hoc recipe holding only that pipeline, which is never cached:
     `apply_queued` at the top level, so the wait can cancel the asset. `get_asset` stays
     query-only, because a pipeline has no asset key; the caller converts to a query for a cached
     asset. Inside a command, `context.evaluate(pipeline)` evaluates a representable pipeline as
     its query, so the dependency and the cache are the query's, and any other through
     `Context::apply_ad_hoc`, whose dependencies are passed on. (Decision 1 had named
     `interpreter::evaluate_pipeline` and `NotSupported` for a non-representable pipeline in a
     command; Decision 3 replaced both, so no core issue is needed.)
  6. **Decided (Decision 1): `query-builder-in-bindings`.** Two designs cannot own one issue
     (§5.1.1), so `QUERY-BUILDER-NOT-IN-BINDINGS` was split at the gate. Its Python half,
     `PY-HAS-NO-VALIDATING-QUERY-EDITOR` (the editors of question 2), joined this design
     (`merged:`, Scope Changes below). `query-builder-in-bindings` took a Scope Change narrowing it
     to `liquers-web`, to be re-based on `QueryEditor`.
  7. **Proposed resolution: declared pipeline commands.** `env.pipeline_command(name, body,
     arguments=[…], namespace=…, label=…, doc=…, filename=…)` goes into a new slot of the retained
     declarations (`Declarations` in `liquers-py/src/environment.rs`). The slot is applied after
     the Python commands and before imported metadata, so a rebuild keeps it. It is shared with
     the template commands of `liquers-py-templates`, which lands first and adds it.
     The call checks at once against the next build's registry and raises on an error,
     so a pipeline command is declared after the Python commands its body calls (documented).
     `lq.CommandDefinition` gains `kind`, `alias_of`, `head_parameters` and `pipeline`, specified
     once for both designs.

## Decisions

1. Orest, 2026-11-04, at the Phase 1 gate, on questions 1, 2, 5 and 6 as recommended: "agreed".
   Landing order data nodes, templates, pipelines: "ok".
2. Orest, 2026-11-04: "proceed all": Phases 2-5 pre-approved; no decision was open.
3. Phase 2, 2026-11-04, under the pre-approval, an assumption Orest can reverse before
   implementation: question 5 is refined. A pipeline is evaluated as an ad-hoc recipe holding only
   that pipeline: `apply_queued` at the top level (the asset harness, cancellable on an interrupt)
   and `Context::apply_ad_hoc` inside a command, so a non-representable pipeline evaluates there
   too instead of raising `NotSupported`. A representable one in a command still evaluates as its
   query. The answers to AC-1 are unchanged.

## Scope Changes

- **2026-11-04** — `PY-HAS-NO-VALIDATING-QUERY-EDITOR` merged in (issue-triage case 2: the editors
  of question 2 are exactly that issue's Python half, split from `QUERY-BUILDER-NOT-IN-BINDINGS`).
  Example: building `-R/data/sales.csv/-/ns-pl/is_in-region-EU-US` step by step, a misspelt
  argument refused with the editor unchanged. Covered by AC-7 and AC-8; no new criterion; size
  stays `L`. Phases: 1. Approved: Orest at the Phase 1 gate ("agreed").

- **2026-11-04** — Decision 3 (Phase 2): evaluating a non-representable pipeline inside a command
  moves from non-goal to scope, through `Context::apply_ad_hoc` with a pipeline-only recipe. Example:
  `context.evaluate([{"value": 1}])` returns `1` instead of raising `NotSupported`. Covered by AC-1
  (its Phase 3 test `test_non_representable_in_a_command`); no new criterion; size stays `L`.
  Phases: 1, 2. Approved: under the pre-approval (Decision 2), logged as an assumption.

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
  WHEN the problem example's `top` is evaluated with `evaluate`, `evaluate_state` and `evaluate_async`, and so is a pipeline whose step fails
  THEN each gives the value (or a state with the metadata) that evaluating its query gives, and the failing one raises the same error type as its query (PIPELINE01)
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
  WHEN `env.pipeline_command("top_in", body, arguments=[…])` registers a body starting with `{"action": "from_csv", "ns": "pl"}` (so it reads its input state), with placeholders `regions` and `n`, and `-R/data/sales.csv/-/top_in-5-EU-US` is evaluated (arguments `n`, then `regions` as the variadic last)
  THEN it gives what the expanded query gives, and `command_metadata("top_in").definition` has `kind == "pipeline"` and its `pipeline` equals the body (PIPELINE04)
- **AC-6** A bad pipeline command is refused whole
  WHEN the body names a placeholder that is not a declared argument, or a step command that does not exist
  THEN registration raises the typed error naming it, and no `top_in` is registered
- **AC-7** A refused edit changes nothing
  WHEN a `QueryEditor` built from the problem query is given an unknown argument name, then a value that would make it unrepresentable, and a `PipelineEditor` is given an unknown argument and a multi-step call whose second step fails
  THEN each step raises `lq.ParameterError`, `editor.query()` still encodes the problem query, and the pipeline editor's pipeline is unchanged (PIPELINE05)
- **AC-8** Editors expose the registry's view
  WHEN an action is added with `editor.action("is_in", ns="pl")`
  THEN the editor returns its `CommandMetadata` and the `ArgumentInfo` of `column` and `values`, and `unset_arguments` lists what is still missing
- **AC-9** Pipeline values keep their type across the boundary
  WHEN a query's result is `Value::Pipeline`, a Python command returns an `lq.Pipeline`, a recipe with a `pipeline` is read through `env.recipes().recipe(key)`, and a command's metadata has a pipeline default
  THEN Python receives `lq.Pipeline` in the first, third and fourth cases, and the second result's state reports type `Pipeline`
- **AC-10** Only JSON reaches a JSON slot
  WHEN a pipeline is built with a `datetime` as a literal argument
  THEN it raises `lq.ConversionError` naming the path of the value

Non-goals: pipelines in `liquers-web`; a new core planning mode; placeholders in query text; typed `lq.PipelineElement`
classes (elements stay JSON-shaped `dict`s).

## Core Interactions

Query and Plan: conversions, pipeline-only recipes (`apply_queued`, `apply_ad_hoc`). Commands: `register_pipeline`, kept by a new
declaration slot (question 7). Value types: `Value::Pipeline` both ways. Bindings: `lq.Pipeline`,
the editor classes, new `Environment` and `Context` members.

## Crate Placement

`liquers-py` only: a new `src/pipeline.rs` (class, editors, question 4's walker); changes to
`closure.rs` (`wait_for_asset`), `environment.rs`, `context.rs`, `value.rs`, `recipes.rs`, `command_metadata.rs`, the stubs and
`__init__.py`.

## Documentation Intent

- Reference: `PYTHON_INTEGRATION.md`, a new §Pipelines (class, entry points, conversion,
  editors, registration, errors).
- Guide: a "Pipelines" chapter in `PYTHON_GUIDE.md`; `PIPELINE_GUIDE.md` gains the Python form.
- Other documents: the `liquers-py/README.md` conformance row; `LANGUAGE-INTEGRATION_GUIDE.md`
  §PIPELINE and §6 Python (what liquers-py does); `PIPELINES.md` §Related; `specs/README.md`.
- Documents to update: `Pipeline` rows added to `PYTHON_INTEGRATION.md` §5's conversion tables;
  the user guide chapters that `docs_index.py chapters-for` lists.
- API docs: the module doc of the new `liquers-py/src/pipeline.rs`, plus `value.rs`,
  `environment.rs`, `context.rs`, `recipes.rs` and `command_metadata.rs` for the new members; docstrings on every new Python name.

## Design Dependencies

- `composite-command-definition` (complete): **requires**. It provides every core API used here.
- `PY-HAS-NO-VALIDATING-QUERY-EDITOR`: merged source (Scope Changes). `QUERY-BUILDER-NOT-IN-BINDINGS`
  / `query-builder-in-bindings` is now its JavaScript counterpart; no dependency either way.
- `liquers-py-templates`: **requires** (lands first, order of Decision 1). It adds the declaration
  slot, `EnvCell::declare` and the `lq.CommandDefinition` accessors (question 7); this design adds
  the `Pipeline` arm and `CommandDefinition.pipeline`.
- `liquers-py-data-nodes`: **requires** (lands first): `liquers-py/src/closure.rs`, whose wait
  helpers this design extends with `wait_for_asset`.

## References

- Guide §PIPELINE; `PIPELINES.md`; `PIPELINE_GUIDE.md`; `liquers-core/src/pipeline/`,
  `interpreter.rs` `evaluate_pipeline`, `commands.rs` `register_pipeline`

# Phase 2: Solution & Architecture — liquers-py-pipelines

## Overview

A new `liquers-py/src/pipeline.rs` holds `lq.Pipeline` (a frozen wrapper of the core `Pipeline`),
the three editor classes, and the JSON-slot walker of question 4. Every Python form of a pipeline
(an `lq.Pipeline`, a `list`, a `dict`) is read once, by `pipeline_from_py`, into the core type
through its JSON form (`Pipeline::from_json`), so no rule is reimplemented. The value bridge maps
`SimpleValue::Pipeline` to and from `lq.Pipeline`. The evaluation entry points accept a pipeline and
run it as an ad-hoc recipe whose `pipeline` field is the whole recipe (`Recipe.pipeline`, "alone, it
is the whole recipe"): `apply_queued` at the top level, `Context::apply_ad_hoc` inside a command.
Pipeline commands use the declaration slot of `liquers-py-templates` (`DeclaredCommand::Pipeline`).

**Question 5, refined here (Decision 3, an assumption).** Phase 1 named
`interpreter::evaluate_pipeline` at the top level and `NotSupported` for a non-representable
pipeline inside a command, because "`Context` has no `evaluate_pipeline`". A recipe carrying only a
pipeline gives both through existing API: `apply_queued` runs it through the asset harness (status,
log, and a handle to cancel on an interrupt, which `evaluate_pipeline`'s temporary asset lacks),
and `Context::apply_ad_hoc` evaluates any pipeline inside a command with its dependencies passed
on. A representable pipeline in a command still evaluates as its query (cached, as decided). So
AC-1's three entry points behave as decided, and the `NotSupported` case disappears. The core issue
question 5 would have filed is not needed.

Rejected:
- `interpreter::evaluate_pipeline` (above).
- Converting through `SimpleValue` (`try_from_json_value`) as `value.rs` does today: a `list`
  becomes `SimpleValue::Array`, so the type is lost (question 1).
- Python element classes (`lq.ActionElement`): a non-goal; elements stay JSON-shaped.
- An `lq.ActionEditor` that holds a core `ActionEditor<'a>`: it borrows its editor, which a Python
  object cannot (question 2).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `RECIPE-OVERRIDES-MISS-A-FINAL-PIPELINE-COMMAND` | draft | P2 | A recipe override of a Python pipeline command at the end of a query is missed | No | No | Inherited from core; documented in §Pipelines |
| `PIPELINE-COMMAND-OPERAND-TYPES-UNCHECKED` | draft | P3 | A placeholder's type in `pipeline_command(arguments=…)` is not checked | No | No | Inherited; `ArgumentInfo` given by the user is taken as is |
| `PLAN-BUILDER-DUPLICATES-PIPELINE-PLANNING` | draft | P3 | None (planning is core's) | No | No | — |
| `PY-HAS-NO-VALIDATING-QUERY-EDITOR` | draft | P2 | Implemented here (merged) | — | — | Closed in Phase 5 |
| `PY-REPRS-SHOW-RUST-DEBUG-FORMAT` | draft | P3 | New classes need reprs | No | No | `Pipeline([…])` as `to_list()`'s repr |

Command namespaces: none added. A pipeline command lands in the namespace its declaration names;
the tests use `pl` (polars) steps as the problem example does.

## Interfaces

**`lq.Pipeline`** (`pipeline.rs`). All sync: conversion never waits.

```rust
#[pyclass(frozen, eq, name = "Pipeline", module = "liquers")]
pub struct Pipeline(pub liquers_core::pipeline::Pipeline);
// Python: Pipeline(data) where data is a list, a dict or an lq.Pipeline;
//   Pipeline.from_json(text), Pipeline.from_query(q: str | lq.Query), p.to_json() -> str,
//   p.to_list() -> list | dict (the JSON form), p.to_query(env) -> lq.Query,
//   p.elements_count -> int, ==, repr.

/// Any Python form of a pipeline as the core type. `path` is reported in errors.
pub(crate) fn pipeline_from_py(py: Python<'_>, object: &Bound<'_, PyAny>) -> Result<CorePipeline, Error>;
/// Question 4's walker: a JSON slot's value, or ConversionError naming `path` (`$[1].arguments.values[0]`).
pub(crate) fn json_slot(object: &Bound<'_, PyAny>, path: &mut JsonPath) -> Result<serde_json::Value, Error>;
```

- `pipeline_from_py`: an `lq.Pipeline` is cloned; otherwise `json_slot` walks the whole structure
  (`None`, `bool`, `int` in `i64`, finite `float`, the reference's strings for NaN and ±inf, `str`,
  `list` / `tuple`, `dict` with `str` keys; a cycle is refused with the path), then
  `Pipeline::from_json`. A `str` at the top is refused (a query is not a pipeline; use
  `Pipeline.from_query`).
- `to_query(env)`: `to_query_with(envref.get_command_metadata_registry())`; its error is
  `Error::not_representable` → `lq.ParameterError` (question 3).

**Value bridge** (`value.rs`): `simple_to_py` maps `SimpleValue::Pipeline` to `lq.Pipeline`;
`wrapper_to_simple` maps `lq.Pipeline` to `SimpleValue::Pipeline`. `recipes.rs`
`Recipe.pipeline` returns `lq.Pipeline`; `command_metadata.rs` `ArgumentInfo.default` returns an
`lq.Pipeline` for a pipeline default (AC-9).

**Entry points** (`environment.rs`, `context.rs`):

```rust
pub(crate) enum Evaluable { Query(Query), Pipeline(CorePipeline) }
/// A str or lq.Query is a query; an lq.Pipeline, a list or a dict a pipeline.
pub(crate) fn to_evaluable(py: Python<'_>, object: &Bound<'_, PyAny>) -> Result<Evaluable, Error>;
fn pipeline_recipe(pipeline: CorePipeline) -> Recipe;   // `Recipe { pipeline: Some(p), ..Default::default() }`
```

- `Environment.evaluate` / `evaluate_state` / `_async`: a pipeline goes to
  `get_asset_manager().apply_queued(pipeline_recipe(p), State::new())`, then `asset.get()`. The
  sync forms wait in `block_on`; when the wait is interrupted they cancel the asset
  (`AssetRef::cancel`) before raising, as `closure::wait_for_run` does (shared helper
  `closure::wait_for_asset`). `get_asset` stays query-only and raises `TypeError` for a pipeline.
- `Context.evaluate` / `evaluate_async`: a representable pipeline evaluates its query
  (`to_query_with`), so the dependency and the cache are the query's; any other goes to
  `context.apply_ad_hoc(pipeline_recipe(p), State::new())`, whose dependencies are passed on.

**Editors** (`pipeline.rs`). Frozen classes with interior `std::sync::Mutex` (free-threaded safe);
every method locks, edits a **clone** of the core editor, and stores it only on success, so a
refused step, including the second step of one Python call, leaves the editor unchanged (AC-7).

```rust
#[pyclass(frozen, name = "PipelineEditor", module = "liquers")]
pub struct PipelineEditor { inner: Mutex<core::PipelineEditor> }
#[pyclass(frozen, name = "QueryEditor", module = "liquers")]
pub struct QueryEditor { inner: Mutex<core::QueryEditor> }
#[pyclass(frozen, name = "ActionEditor", module = "liquers")]
pub struct ActionEditor { owner: Owner, index: usize }
enum Owner { Pipeline(Py<PipelineEditor>), Query(Py<QueryEditor>) }
```

- Construction: `env.pipeline_editor(pipeline=None)` and `env.query_editor(query=None)` take
  `Context::command_metadata_snapshot`'s equivalent on the built environment
  (`Arc::new(envref.get_command_metadata_registry().clone())`); a command registered later is
  unknown to the editor (documented).
- Editor members: `push(element)`, `insert(at, element)`, `replace(at, element)`, `remove(at)`,
  `move_to(from, to)`, `set_realm(r)`, `set_filename(name)`, `resource(key, read="asset")`,
  `default_ns(ns)`, each returning `self`; `action(name, ns=None) -> ActionEditor`,
  `edit_action(at) -> ActionEditor`; `command_metadata(at) -> lq.CommandMetadata`,
  `argument_info(at, arg) -> lq.ArgumentInfo`, `unset_arguments(at) -> list[lq.ArgumentInfo]`,
  `validate()`; `pipeline` (an `lq.Pipeline`); `QueryEditor.query()` (an `lq.Query`). An element
  is any Python form `pipeline_from_py` reads for one element.
- `ActionEditor` members: `ns(ns)`, `set(arg, value)`, `placeholder(arg, name)`, `link(arg, q)`,
  `pipeline(arg, p)`, `state(arg)`, `push(value)`, `unset(arg)`, `rename(action)`, each returning
  `self`; `metadata`, `argument_info(arg)`, `index`. Each is one locked clone-edit-commit through
  `edit_action(index)` on the owner. `arg` is a name or a position (`ArgRef`); `value` goes through
  `json_slot`.
- `action("is_in", ns="pl")` is two core steps (`action`, then `ns`) on one clone.

**Pipeline commands** (`environment.rs`). `env.pipeline_command(name, body, *, arguments=(),
namespace="", label=None, doc=None, filename=None)`: `body` through `pipeline_from_py`;
`arguments` are names (`str`, an `any` argument with no default) or `lq.ArgumentInfo`; then
`EnvCell::declare(DeclaredCommand::Pipeline { … })`, which checks at once against the next build's
registry (`register_pipeline`, `check_pipeline_command`, `builder.validate()`) and raises on an
error, leaving nothing registered (AC-6). The slot, `declare` and the `CommandDefinition`
accessors come from `liquers-py-templates`; this design adds the `Pipeline` arm and fills
`CommandDefinition.pipeline`.

## Integration Points

- `liquers-py/src/pipeline.rs` (new): `Pipeline`, editors, `pipeline_from_py`, `json_slot`.
- `liquers-py/src/value.rs`: `simple_to_py`, `wrapper_to_simple`.
- `liquers-py/src/environment.rs`: `to_evaluable` (replacing `to_query` at the evaluate call sites),
  `evaluate*`, `pipeline_editor`, `query_editor`, `pipeline_command`, the `Pipeline` arm of
  `DeclaredCommand`.
- `liquers-py/src/context.rs`: `evaluate`, `evaluate_async`.
- `liquers-py/src/closure.rs`: `wait_for_asset` (beside `wait_for_run`).
- `liquers-py/src/recipes.rs`, `command_metadata.rs`: `lq.Pipeline` getters.
- `liquers-py/src/lib.rs`, `python/liquers/__init__.py`, `__init__.pyi`, `_native.pyi`.
- `liquers-py/tests/test_pipelines.py` (new).

## Error Handling

| Case | Error |
|---|---|
| A JSON slot holds a non-JSON value (a `datetime`), a cycle, an `int` outside `i64` | `Error::conversion_error_with_message` naming the path → `lq.ConversionError` (AC-10) |
| A structure that is not a pipeline | `Pipeline::from_json`'s error (`ParseError` / `ParameterError`), path included |
| `to_query` of a non-representable pipeline | `Error::not_representable` → `lq.ParameterError` naming the rule and path (AC-4) |
| A refused editor step | the core editor's typed error (`ParameterError`), editor unchanged (AC-7) |
| `get_asset(pipeline)` | Python `TypeError` |
| A pipeline command that does not plan or names an unknown placeholder | `register_pipeline` / `check_pipeline_command` issue → `lq.LiquersError`, nothing declared (AC-6) |
| A failing step at evaluation | the step's error, as for its query (AC-1) |
| Interrupted top-level wait | the asset is cancelled, then `KeyboardInterrupt` |

## Relevant Commands

None added. A pipeline command is user-declared; its body names existing commands (the tests use
`pl/from_csv`, `pl/is_in`, `pl/head`, which `liquers-validate --pipeline` checks against
`specs/command_registry.yaml`).

## Documentation Architecture

| Path | Kind | Audience | Change |
|---|---|---|---|
| `specs/reference/PYTHON_INTEGRATION.md` | reference | integrators | New §Pipelines; `Pipeline` rows in §5's conversion tables |
| `specs/guides/PYTHON_GUIDE.md` | guide | Python users | "Pipelines" chapter (build, evaluate, convert, edit, declare) |
| `specs/guides/PIPELINE_GUIDE.md` | guide | all | The Python form beside YAML and Rust |
| `specs/reference/PIPELINES.md` | reference | all | §Related: the Python binding |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | guide | integrators | §PIPELINE, §6 Python |
| `liquers-py/README.md`, `specs/README.md` | readme, map | all | PIPELINE row; status |
| `docs/guides/python/` chapters | user guide | Python users | per `docs_index.py chapters-for` (Phase 5) |

Module docs: `pipeline.rs` (new), `value.rs`, `environment.rs`, `context.rs`, `recipes.rs`,
`command_metadata.rs`, `closure.rs`.

## Risks

| Risk | Mitigation |
|---|---|
| A pipeline-only recipe through `apply_queued` plans differently from `evaluate_pipeline` (the cwd) | Both plan with `PipelinePlanBuilder`; AC-1 compares against the query; a relative-key pipeline is a Phase 3 edge case |
| `apply_queued` takes a job-queue slot | Top level only, where `evaluate` takes one too; inside a command `apply_ad_hoc` takes none |
| Changing a pipeline value from `dict` to `lq.Pipeline` | No release exists (question 1); the M5 tests that read a pipeline as a `dict` are updated |
| An editor snapshot is stale after a new command | Documented; `env.query_editor()` again takes a fresh one |
| Decision 3 refines an agreed answer | Logged in the Phase 1 decision log for Orest at the pre-implementation stop |

---
title: Pipelines
kind: reference
audience: internal
area: [core/commands, core/plan, core/query, core/validate]
reviewed: 2026-11-04
---
# Pipelines

A **pipeline** is the structured, serializable form of a query: a list of elements in one realm.
Every query converts to a pipeline. A pipeline converts back to a query when it is
**representable**, and then it plans exactly as that query. A pipeline can also say what a query
cannot: an argument that is any JSON value, a named argument, a nested pipeline, the input state,
or a placeholder.

Pipelines are used in three places:
- **pipeline commands** (`CommandDefinition::Pipeline`), whose arguments are the pipeline's
  placeholders (§4);
- **recipes**, whose `pipeline` runs before their `query` (§5);
- code that **builds or edits queries**, through `PipelineEditor` and `QueryEditor` (§8).

Code: `liquers-core/src/pipeline/` (`mod.rs` types and serde, `convert.rs` conversions and
representability, `editor.rs` editors, `plan.rs` `PipelinePlanBuilder`). How-to:
[`guides/PIPELINE_GUIDE.md`](../guides/PIPELINE_GUIDE.md).

## 1. The model

```rust
pub struct Pipeline {
    pub realm: String,                      // the segment header name; "" is the root realm
    pub header_parameters: Vec<HeaderParameter>,
    pub rooted: bool,                       // a leading `/`
    pub elements: Vec<PipelineElement>,     // Located<ElementKind>
    pub filename: Option<String>,           // top level only
}
```

`Located<T>` is an item with an optional `pos`: set when the pipeline was converted from a parsed
query, absent when it was written by hand. Positions are not part of equality.

| Element (`ElementKind`) | Serialized | Meaning |
|---|---|---|
| `Action` | `{action, ns?, parameters?, arguments?}` | call a command; `ns` is a name or a list, absent for the default namespaces (never the caller's) |
| `Value` | `{value: …}` | the value (any JSON) becomes the state (`Step::UseValue`) |
| `Resource` | `{resource: "a/b", read?}` | read a key; `read` is `asset` (default), `binary`, `metadata`, `recipe`, `directory`, `stored_binary`, `stored_metadata`, `stored_directory` |
| `KeyValue`, `Cwd` | `{key_value: k}`, `{cwd: k}` | the key as the state; set the working directory |
| `Evaluate` | `{evaluate: "<query>"}` or `{evaluate: […]}` | evaluate a query or pipeline as a separate asset, its result the state |
| `QueryValue` | `{query_value: …}` | the query itself as the state |
| `Quote`, `Volatile` | `{quote: true}`, `{volatile: true}` | `q` and `v` |
| `Pipeline` | `{pipeline: …}` | a nested pipeline in its own realm; an empty realm is the root realm |
| `Text` | a bare string | query text, inlined in place |

An action's argument is a **source** (`Source`), given in `parameters` (positional, consumed
first) or `arguments` (by name):

| Source | Serialized | Meaning |
|---|---|---|
| `Value` | a bare scalar, array or object, or `{value: …}` | a literal of any JSON shape |
| `Placeholder` | `{placeholder: name}` | a pipeline command's argument (§4) |
| `Link` | `{link: "<query>"}` | the query is evaluated and its result passed |
| `Pipeline` | `{pipeline: […]}` | a nested pipeline evaluated and its result passed |
| `State` | `{state: true}` | the enclosing pipeline command's input state |
| `Multiple` | `{multiple: […]}` | the elements of a variadic argument |

A literal object whose only key is one of those six is written inside `{value: …}`. A bare list
is a pipeline with only `elements`. An unknown key is an error naming the element's index.
Argument values are `serde_json::Value` with `float_roundtrip`, so a float survives JSON and query
round trips exactly; NaN and ±inf are written as the strings `"NaN"`, `"inf"`, `"-inf"`.

```yaml
realm: ""
elements:
  - "-R/data/sales.csv/-/ns-pl/from_csv"
  - {action: is_in, ns: pl, arguments: {column: region, values: [EU, US]}}
  - {action: head, ns: pl, parameters: [{placeholder: n}]}
filename: top.csv
```

`Value::Pipeline` holds a pipeline like `Value::Query` holds a query: type identifier `Pipeline`,
default filename `pipeline.json`, written as `json`, `yaml` or, when representable, `txt`.
`ValueInterface::try_into_pipeline` reads a pipeline, a query, or JSON or YAML text
(`reference/VALUE_TYPE_SYSTEM.md`).

## 2. Conversions

| Call | Result |
|---|---|
| `Pipeline::try_from(&query)` | always succeeds for a parsed query; keeps positions and typed parameters as text |
| `pipeline.to_query_with(&cmr)` | the query, or `Error::not_representable(rule, path, …)` |
| `Pipeline::from_query_text(text)`, `pipeline.to_query_text()` | the same without a registry (lenient: no named arguments are placed) |
| `recipe.to_pipeline(&cmr)` | the recipe's pipeline followed by its query (§5) |
| `TryFrom<&ParameterValue> for Pipeline` | the pipeline of any link-like parameter; a value is refused |

The rules: every query is a pipeline; a pipeline from a query converts back; the round trip
gives the query's **canonical form**, and converting again changes nothing. Converting to a query
needs the registry, because a query has positional parameters only: named arguments are placed in
the command's parameter order and typed scalars keep their declared type.

### Representability rules

`to_query_with` checks these in element order; the first that fails is the error.

| Id | Rule |
|---|---|
| P1 | No `value` or `evaluate` element. |
| P2 | Only the top-level pipeline has a `filename`, and it is a filename. |
| P3 | `rooted` and the header parameters are carried as they are. |
| P4 | The realm is a realm name or empty; a nested pipeline writes its segments under its own realm. |
| E1 | An action is an identifier other than `ns`, `q`, `v`, and its sources obey S1–S7. |
| E2 | Each namespace is an identifier. |
| E3 | A nested pipeline obeys these rules itself. |
| E4, E5 | A `resource`, `key_value` or `cwd` key has resource-name segments and is not a placeholder. |
| E6 | Inlined text parses and obeys these rules. |
| E7 | `quote` and `volatile` are always representable. |
| E8 | A `query_value` is the first element and its query is representable. |
| S1 | A scalar is its text; read back, it takes the argument's type (`any` and `text` read text). |
| S2, S3 | An array, or `{multiple}`, fills only the variadic argument, written last. |
| S4 | Null, an object, or an array for a single argument: never. |
| S5, S6 | A link, or a nested pipeline, is representable. |
| S7 | A placeholder or the input state: never. |
| N1–N4 | Named arguments need the registry; a gap is filled with its default, as text; a gap at a required or injected argument, or an argument given twice, is not representable. |

Namespaces are never the reason: per transform segment the conversion tries no `ns`, then one
`ns` listing the segment's namespaces in order of use, then an `ns` before each action that still
resolves wrongly. Segments of one realm merge. A header level other than `-` is refused: levels
are reserved for nesting.

## 3. Planning

`PipelinePlanBuilder` (`pipeline/plan.rs`) plans queries and pipelines; `Pipeline::to_plan(&cmr)`,
`make_plan` and `interpreter::evaluate_pipeline` go through it. It has two modes, and
`DEFAULT_PLANNING_MODE` is `Native`:

- **`Native`** plans each element itself. A representable run plans exactly as its query, prefix
  queries and positions included (AC-10); `plans_are_equal_in_both_modes` checks every query of
  the parser corpus, parsed and canonical, against `PlanBuilder`.
- **`Delegate`** gives representable runs to `PlanBuilder`.

Elements that have no query form plan as their one step: `UseValue`, `GetAsset*`/`GetResource*`,
`UseKeyValue`, `Evaluate` (or `Step::Plan`, starting from an empty state, for a pipeline with no
query form), `UseQueryValue`. An action with named arguments or structured values assigns
positional sources first, then named ones; an omitted argument takes its default, and an unknown,
injected or doubly given name is a `ParameterError`. A list for a variadic argument gives one
element per entry; a placeholder bound to a list inside `{multiple}` is spliced in place. A list
for a single argument reaches an `any` argument whole and is a `ConversionError` otherwise.

A pipeline argument plans as a link when representable (a shared asset, `ParameterLink`), else as
`ParameterValue::ParameterPipeline` / `OverridePipeline` / `DefaultPipeline`. The interpreter
evaluates the latter as a nested plan from an empty state: its dependencies are recorded, but it
has no asset of its own and is not cached. A command default may be a pipeline:
`CommandParameterValue::Pipeline`, written `{pipeline: …}`.

A placeholder outside a pipeline command's definition is a planning error naming it.

## 4. Pipeline commands

A pipeline command is a command whose body is a pipeline and whose arguments are its declared
placeholders. Calling it runs the body on its input with the arguments bound.

```rust
cr.register_pipeline(CommandKey::new("", "sales", "top_in"), pipeline, vec![by, n, regions])?;
```

```yaml
definition: !Pipeline
  pipeline:
    - {action: from_csv, ns: pl}
    - {action: is_in, ns: pl, arguments: {column: region, values: {placeholder: regions}}}
    - {action: sort, ns: pl, arguments: {column: {placeholder: by}, ascending: false}}
    - {action: head, ns: pl, arguments: {n: {placeholder: n}}}
```

**Registration** (`CommandRegistry::register_pipeline`) stores metadata only. The command is a
source command (`state_argument: None`) when the body's first element sets the state. An argument
of type `any` takes the type of the slot its placeholder fills, and the last argument becomes
variadic when it fills a variadic slot whole. A step that needs a payload makes the command
`payload: required`. `CommandMetadata::check` and `CommandMetadataRegistry::check_pipeline_command`
report each problem as an `Issue::CommandRegistry`; an error refuses registration:
- a placeholder that is not a declared argument, an `injected` argument, a body filename, a source
  command reading the input state;
- a step command that does not resolve, or a named argument it does not have;
- a cycle through pipeline command bodies.

An argument no placeholder uses is a warning. An alias cannot target a pipeline command.

**Planning.** Either builder expands a call into one `Step::PipelineCommand { realm, ns, name,
position, parameters, query, plan }`. The call is resolved against the command's own arguments,
so errors name the pipeline command's arguments, and an omitted one takes its default or is
`ArgumentMissing`. Each argument is bound to its placeholder (a value, a link, a pipeline), and the
body is planned in `Native` mode in the command's realm. Elements resolve through their own `ns`
and the default namespaces, never the caller's `ns`. A body may call aliases and other pipeline
commands; a repeated command, or nesting deeper than `MAX_PIPELINE_DEPTH` (16), is
`Error::pipeline_cycle` naming the chain. The body's volatility, payload requirement and
expiration fold into the caller; its actions record no prefix query, so nothing is cut or cached
inside it. `freeze_cwd` freezes the body against the caller's working key.

**Running.** The step runs its body on the current state in a **pipeline frame**
(`context::PipelineFrame`): `{state: true}` (`ParameterValue::InputState`) reads the frame's input.
The frame is lexical. An argument bound to the caller's own input state is read by the step in the
caller's frame (the step's `parameters`) and reaches the body as
`ParameterValue::FrameArgument(argument, placeholder)`. Outside a pipeline command the input state
is an error. An error inside the body is suffixed `(in pipeline command '<key>')`
(`Error::with_pipeline`).

**Caching and dependencies.** The result is cached under the calling query. The dependency scan
records the pipeline command's metadata key, so editing its pipeline invalidates the result, and
scans the body's steps as dependencies of the caller. Injected arguments of the body's commands
are injected from the caller's context, as for a direct call.

## 5. Recipe pipelines

```yaml
- pipeline:
  - "-R/data/sales.csv/-/ns-pl/from_csv"
  - {action: is_in, ns: pl, arguments: {column: region, values: [EU, US]}}
  query: ns-pl/sort-amount-f/head-10/eu.csv
```

`Recipe::pipeline` (optional, omitted when absent) runs first and the query continues from its
state. Alone, the pipeline is the recipe, filename included. A recipe pipeline holds literal
values only. `Recipe::combined_pipeline` refuses a placeholder, a query that starts with a
resource (it would discard the pipeline's state), a filename in both parts, and `arguments` /
`links` without a query (they override the query's last action). A recipe with a pipeline has
no `key()` and is not a pure query; its filename is the query's, or the pipeline's when it has no
query. A recipe without a pipeline plans exactly as before. Overrides of a final pipeline command
are not applied yet (`RECIPE-OVERRIDES-MISS-A-FINAL-PIPELINE-COMMAND`).

## 6. Values, steps and parameters added

| Type | Added |
|---|---|
| `Step` | `UseValue(serde_json::Value)`, `PipelineCommand { … }` |
| `ParameterValue` | `ParameterPipeline`, `OverridePipeline`, `DefaultPipeline`, `InputState`, `FrameArgument` |
| `CommandDefinition` | `Pipeline { pipeline }` |
| `CommandParameterValue` | `Pipeline(Pipeline)`, tag `pipeline` |
| `Value` | `Pipeline` (`SimpleValue::Pipeline`, delegated by `ExtValue`) |
| `Error` | `invalid_pipeline`, `not_representable`, `invalid_token`, `invalid_argument`, `pipeline_cycle`, `with_pipeline` |

## 7. Validation

`liquers-validate --pipeline <file|->` validates a pipeline: its query form, when it has one, is
`encoded`, and a pipeline without one is a warning naming the rule. `--emit query` makes that an
error (exit 1); `--emit pipeline` adds every input's pipeline form; `--emit plan` keeps the plan
under `--detail summary`. `--build R:<key> A:<name> P:<value> …` spells a query as shell
arguments that need no escaping. `--recipes` accepts recipes with a `pipeline`.

## 8. Editors

`PipelineEditor` edits a pipeline through validated steps: each edit is checked against an
`Arc<CommandMetadataRegistry>` snapshot (`EnvRef::command_metadata_snapshot`) and refused with the
editor unchanged. It exposes each action's `CommandMetadata` and `ArgumentInfo`. `ActionEditor`
(from `action(name)`) chains `ns`, `set`, `push`, `placeholder`, `link`, `pipeline`, `state`,
`unset`. `QueryEditor` wraps it and refuses any edit after which the pipeline has no query form,
so `query()` always succeeds.

## Related

- [`guides/PIPELINE_GUIDE.md`](../guides/PIPELINE_GUIDE.md) — building, converting, registering.
- [`COMMAND_ALIASES.md`](COMMAND_ALIASES.md) — the other command with no implementation.
- [`RECIPES_AND_PLANS.md`](RECIPES_AND_PLANS.md) — plans, steps and recipes.
- [`api/DOC_02_QUERY_LANGUAGE_REFERENCE.md`](api/DOC_02_QUERY_LANGUAGE_REFERENCE.md) — the query text.
- [`PYTHON_INTEGRATION.md`](PYTHON_INTEGRATION.md) §16 — pipelines from Python (`lq.Pipeline`, the editors, `pipeline_command`).
- Design: [`design/composite-command-definition/`](../design/composite-command-definition/).

## History

| Date | Change | Source |
|---|---|---|
| 2026-11-04 | Related: the Python integration's pipelines section. | `design/liquers-py-pipelines/` |
| 2026-11-04 | Created: the model and serialized form, conversions and representability rules, planning, pipeline commands, recipe pipelines, validation and editors, as implemented and tested. | phase-5, `design/composite-command-definition/` |

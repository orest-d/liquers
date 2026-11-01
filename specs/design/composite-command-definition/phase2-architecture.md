# Phase 2: Solution & Architecture — Pipeline commands

Rewritten on 2026-11-01 after Phase 1's approval. It replaces the earlier draft, which assumed
placeholders in query text. Shapes and names were explored in
[`notation-and-integration.md`](./notation-and-integration.md) §2 and
[`pipeline-and-plan.md`](./pipeline-and-plan.md); this document is the decision.

## Overview

A **`Pipeline`** (new `liquers-core/src/pipeline.rs`) is a serde list of elements: the author-facing,
stable counterpart of a `Plan`'s steps. A new **`PipelineBuilder`** plans it. The existing
`PlanBuilder` stays as it is for queries, and both share one crate-private **`PlanAssembly`**, the
state the two append steps to. That makes a plan that mixes queries and pipelines one assembly:
a query calls a pipeline command, a pipeline embeds `{query: …}` text, and a recipe runs a pipeline
after its query. A **pipeline command** is `CommandDefinition::Pipeline`. The planner expands it,
from either builder, into a nested `Step::PipelineCommand` whose body is planned with the call's
arguments bound to the body's placeholders. A value element becomes the current state through the
new `Step::UseValue`.

Two rules keep the new path honest:

- **Text-form runs delegate.** A maximal run of elements that has a query text form
  (`Pipeline::to_query`) is planned by `PlanBuilder` on that text, so a text-form pipeline plans
  *exactly* as its query (AC-10, AC-24). Only elements without a text form (a structured value, a
  value as state, a nested pipeline, `{state}`) go through `PipelineBuilder`'s own code.
- **Named arguments reuse overrides.** An action element is resolved by building the positional
  part with `ResolvedParameterValues::from_action`, then applying each named argument through the
  existing override path (`override_value` / `override_link` and `convert_override`). The
  conversion, enum and variadic rules of recipe overrides therefore apply unchanged.

Rejected alternatives:

- **Replacing `PlanBuilder`** (`pipeline-and-plan.md` U1 in full: every query converted to a
  pipeline first). The maintainer chose to keep the old builder. Delegating text-form runs gives
  the same guarantee without touching the query path, which `parse.rs` and the plan tests pin.
- **Flattening a pipeline command's steps into the caller's plan** with an `ActionOrigin::Pipeline`
  tag. `{state}` then needs a runtime marker of where the command's input began, and
  `cut_predecessor` would need a rule not to cut inside it. A nested step has both properties by
  construction.
- **An untyped `serde_json::Value` body** checked at planning. It gives no schema for authors or
  tools, and pushes every shape error to run time.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `RECIPE-OVERRIDE-ONLY-LAST-ACTION` | draft | P3 | Owned by this design: solved by recipe pipelines (AC-15) | — | — | Close when AC-15 ships |
| `LINK-IN-VARIADIC-DOES-NOT-EXPAND` | draft | P3 | A link inside a variadic gives one element, in pipelines as in queries | No | No | Same rule; §Variadic arguments |
| `VARIADIC-KEY-OR-QUERY-LINK-FAILS-TO-MATERIALIZE` | not-eligible | P3 | Same limit for `{link}` inside `{multiple}` | No | No | Inherits its fix |
| `VARIADIC-EMPTY-ELEMENT-CONVERSION` | eligible | P3 | Defines the empty-element rule this design relies on | No | No | Follow its rule |
| `VALUE-NOT-CONVERTED-AUTOMATICALLY` | draft | P2 | A link's result is not converted to the slot's type | No | No | Inherits its fix |
| `COMMAND-COMPOSITE-VARIADIC-ARGUMENTS` | draft | P3 | Tuple elements would make a list element valid in a variadic | No | No | §Variadic arguments, rule V3 |
| `plan-default-namespaces` (PR #236) | in review | — | Elements without `ns` resolve through its chain (AC-7) | No | No | Uses the chain at HEAD |

## Interfaces

**Pipeline types** (`liquers-core/src/pipeline.rs`, new). Sync: pure data. Owned, `Serialize`,
`Deserialize`, `Clone`, `PartialEq`, `Debug`. `Value` is core's `Value`.

```rust
pub struct Pipeline(pub Vec<PipelineElement>);

/// Told apart by its key, as written: `{action: …}`, `{query: …}`, … (custom `Deserialize`, so an
/// unknown key is an error that names it instead of serde's "did not match any variant").
pub enum PipelineElement {
    Action(ActionElement),
    Resource { resource: String, mode: Option<String> },  // mode: a resource header instruction
    Cwd { cwd: String },                                  // SetCwd
    Key { key: String },                                  // UseKeyValue
    Query { query: String },                              // embedded query text (no placeholders)
    Value { value: Value },                               // `{value: …}` → UseValue
    Placeholder { placeholder: String },                  // the bound value → UseValue (a link: Evaluate)
    Quote,                                                // `{quote: true}` → UseQueryValue of the text so far;
                                                          // an error when what precedes has no text form
    Volatile,                                             // `{volatile: true}`, as `v`
    Filename { filename: String },                        // last element only
}

pub struct ActionElement {
    pub action: String,
    pub ns: Option<Namespaces>,          // `Namespaces`: a string or a list (untagged), as `ns-a-b`
    pub realm: Option<String>,
    pub parameters: Vec<Source>,         // positional, consumed first
    pub arguments: BTreeMap<String, Source>, // by name; BTreeMap: deterministic order
}

pub enum Source {                       // externally tagged: `{value: …}`, `{placeholder: …}`, …
    Value(Value),
    Placeholder(String),
    Link(String),                        // query text
    Pipeline(Pipeline),                  // a nested pipeline, evaluated as a link
    State(bool),                         // `{state: true}`: the pipeline command's input
    Multiple(Vec<Source>),               // elements of a variadic argument
}

impl Pipeline {
    pub fn to_query(&self) -> Option<Query>;                 // text form, if any
    pub fn placeholders(&self) -> BTreeSet<String>;
    pub fn to_plan(&self, cmr: &CommandMetadataRegistry) -> Result<Plan, Error>; // PipelineBuilder
}
impl Query {
    pub fn to_pipeline(&self) -> Pipeline;                   // syntactic, positional, total
}
```

A bare scalar or array is accepted where a `Source` is expected, as `{value: …}`, as
`CommandParameterValue`'s deserializer already does. A map must be tagged, so `{value: {a: 1}}`
stays unambiguous.

**Command definition** (`command_metadata.rs`, `commands.rs`). Sync: registration.

```rust
pub enum CommandDefinition {
    Registered,
    Alias { command: CommandKey, head_parameters: Vec<CommandParameterValue> },
    Pipeline { pipeline: Pipeline },
}
impl<E: Environment> CommandRegistry<E> {
    pub fn register_pipeline(&mut self, key: CommandKey, pipeline: Pipeline,
        arguments: Vec<ArgumentInfo>) -> Result<&mut CommandMetadata, Error>;
}
```

The placeholders are the command's `arguments`, as ordinary `ArgumentInfo`s, and `state_argument`
is set as usual. `register_pipeline` sets `state_argument: None` when the first element is a
resource, `key`, `value` or a source command (AC-5). It fills an omitted `argument_type`,
`multiple` or `gui_info` from the slot each placeholder first fills (AC-3). It refuses the
registration when the check below reports an error. A pipeline command has no executor and no
`impl_version`. Its `metadata_version` covers the pipeline.

**Checks** (`CommandMetadataRegistry::check`, `issue_report.rs`). `CommandMetadata::check` gains
the registry-free checks: placeholders not declared as arguments, `injected` arguments, `{state}`
in a source command, a filename that is not last. `CommandMetadataRegistry::check` adds the checks
that need the registry: unknown step commands, a named argument that is not an argument of its
step, a type conflict between two slots of one placeholder, and cycles. Each is an
`Issue::CommandRegistry` with the pipeline command's key, `Error` or `Warning` (an unused argument,
an inferred type).

**Plan** (`plan.rs`). Sync.

```rust
pub enum Step {
    // … existing variants …
    UseValue(Value),
    PipelineCommand {
        realm: String, ns: String, name: String,   // the pipeline command
        position: Position,
        parameters: ResolvedParameterValues,       // its own arguments, as resolved at the call
        query: Option<Query>,                      // the calling prefix, as `Action::query`
        plan: Box<Plan>,                           // the expanded body
    },
}
pub enum ParameterValue {
    // … existing variants …
    /// The input state of the enclosing pipeline command at nesting `level` (0 = outermost).
    State(String, u16),
    /// A nested pipeline with no text form, evaluated as an unmanaged dependency.
    PlanLink(String, Box<Plan>),
}
pub struct PipelineBuilder<'c> { /* registry, PlanAssembly, bindings, expansion stack, level */ }
impl<'c> PipelineBuilder<'c> {
    pub fn new(pipeline: Pipeline, cmr: &'c CommandMetadataRegistry) -> Self;
    pub fn build(&mut self) -> Result<Plan, Error>;
}
```

`PlanAssembly` (crate-private) holds what `PlanBuilder` keeps today: the `Plan` under
construction, `is_volatile`, `payload_required`, `expires`, `uncached_by`, plus the running prefix
query (`Option<Query>`, `None` from the first element without a text form). `PlanBuilder` keeps
its public API and moves those fields into an assembly. Its `process_action` gains the
`Pipeline` arm described below. `apply_command_flags` and the step push move to the assembly, so
both builders call the same code.

**Interpreter** (`interpreter.rs`). Async, as every step is. `Step::UseValue` returns its value.
`Step::PipelineCommand` runs `apply_plan_state` on its body with the input it receives, and
pushes that input on a per-evaluation frame stack. `ParameterValue::State(_, level)` reads the
frame at `level` when the action's arguments are assembled, beside the existing link loop.
`PlanLink` is evaluated with an empty state through `apply_plan_state`. Its result is recorded in
the log, but not as a keyed dependency, because it has no query identity.

## Planning a pipeline, and combining it with queries

`PipelineBuilder::build` walks the elements:

1. **A text-form run** (consecutive elements whose `to_query` exists) is converted to its query and
   handed to `PlanBuilder::process_query` on the shared assembly, prefixed with the running prefix
   query, so step prefixes match the direct query.
2. **`Action`** without a text form: the command is looked up through the element's `realm` and
   `ns`, else the default chain (AC-7). `from_action` builds the positional part with placeholders
   allowed, then named arguments are applied as overrides and placeholders as bindings. Any slot
   still `ParameterValue::Placeholder` is `missing_argument`. The running prefix becomes `None`.
3. **`Value`** is `Step::UseValue`. **`Placeholder`** is `Step::UseValue` of its bound value, or
   `Step::Evaluate` of a bound link; unbound, it is `missing_argument`.
4. **`Query`**: parsed and handled as a text-form run.
5. **`Filename`**: `Step::Filename`, the last element only.

**Expansion of a pipeline command** (in either builder's action arm): resolve the call against
the command's own arguments (`from_action`, or by name from a pipeline element), so arity and
missing-argument errors name the pipeline command. Turn each resolved argument into a binding:
`Value`, `Link`, `State(level)`. Build the body with a child `PipelineBuilder` that has those
bindings, `level + 1`, and the expansion stack plus this key. A key already on the stack, or a
depth over 16, is `Error::pipeline_cycle`. Fold the body's volatility, payload and expiration into
the caller, with the command's own flags, as an alias does, and push `Step::PipelineCommand`. The
body's own step prefixes are the calling prefix followed by the body's text form, so a text-form
body shares its intermediates with direct queries.

**Outside a definition** (a recipe's pipeline, or `Pipeline::to_plan`) there are no bindings, so a
placeholder is an error at planning (AC-11): "Unresolved placeholder 'p' in pipeline element 3".

**Cut and freeze.** `Plan::cut_predecessor` treats `Step::PipelineCommand` as one action and never
cuts inside it. `finalize_plan` freezes a body's relative operands against the caller's working
key, as for a command's default link. The dependency scan (`DependencyWalk`,
`schedule_plan_dependencies_from`) descends into the body and records the pipeline command's
metadata key, as it records an alias's.

## Recipes

Compatible with today's syntax. `Recipe` gains one optional field:

```rust
#[serde(skip_serializing_if = "Option::is_none", default)]
pub pipeline: Option<Pipeline>,
```

| `query` | `pipeline` | Plans as |
|---|---|---|
| set | absent | today, unchanged |
| set | set | the query's steps, then the pipeline, then the query's filename. The query still names the asset, so `Recipe::key`, `filename`, `data_format`, `store_to_key` and every recipe provider listing are unchanged |
| empty | set | the pipeline alone; its trailing `{filename}` names the asset |

`arguments` and `links` keep their meaning, an override of the plan's last action, which with a
pipeline is the pipeline's last action. A placeholder in a recipe pipeline is an error (AC-11).
`has_arguments` counts a pipeline without a text form, so such a recipe gets no query identity as
an unkeyed recipe, exactly as overrides do today. A keyed recipe stays managed under its key.

## Variadic arguments

This section settles the rules that Phase 1 left to Phase 2 (questions 8, 17 and 18). A
**variadic** argument is one declared `multiple`; it consumes every remaining positional parameter.

| # | Case | Rule |
|---|---|---|
| V1 | Positional `parameters` | Fill slots in order. Once the variadic slot is reached, every remaining source is one element, as in a query (`is_in-region-a-b`). Several placeholders into one variadic: `parameters: [{value: region}, {placeholder: a}, {placeholder: b}]` |
| V2 | Named `arguments` | Fill slots the positional ones did not reach. A name for a slot already filled positionally is a `ParameterError` (Python's rule). A variadic given by name takes `{multiple: […]}`, a list value, or a placeholder bound to a list |
| V3 | A list for a variadic | Its entries are the elements. A list *element* is accepted only where the element type allows it (`any`, or the tuple elements of `COMMAND-COMPOSITE-VARIADIC-ARGUMENTS`); write it `{multiple: [{value: [a, b]}]}` |
| V4 | Splicing | A placeholder bound to a list, inside `{multiple}` or in a positional variadic tail, is spliced in place: `[{value: a}, {placeholder: cols}, {value: b}]` gives `a`, the elements of `cols`, `b`. A scalar binding is one element |
| V5 | A list for a single argument | An `any` argument receives the whole `Value::Array`. A scalar-typed argument is a `ConversionError`, as `convert_override` refuses a list today; there is no silent JSON text. The pipeline loses its text form from that step |
| V6 | Empty list | For a variadic: no elements, the variadic's default. For `any`: the value `[]`, which is not the default |
| V7 | Links | `{link}` or `{pipeline}` inside a variadic is one element; an array result does not expand (`LINK-IN-VARIADIC-DOES-NOT-EXPAND`) |
| V8 | A pipeline command's own variadic | Must be its last parameter-consuming argument. Called from a query, it takes the remaining parameters; from a pipeline, V1–V4 apply. Its placeholder may fill a variadic slot (V4) or an `any` slot (V5), and anything else is an `invalid_pipeline` issue |
| V9 | `ns` | A string or a list of identifiers; not a source, so no placeholder |

Deferred: `{array: […]}` and `{object: {…}}` constructors that build a value from sources. Each
source would have to be materialized first, which runs into
`VARIADIC-KEY-OR-QUERY-LINK-FAILS-TO-MATERIALIZE`.

## Integration Points

- `liquers-core`: `pipeline.rs` (new); `command_metadata.rs` (variant, checks, export);
  `commands.rs` (`register_pipeline`); `plan.rs` (`PlanAssembly`, `PipelineBuilder`, the
  `Pipeline` arm, two `Step` and two `ParameterValue` variants, cut and dependency scan);
  `interpreter.rs` (the new steps and parameters, the frame stack); `recipes.rs` (`pipeline`,
  making `convert_override` crate-visible); `validate/` (`--pipeline <file>`, a text form in the
  output). Every exhaustive `match` over `Step`, `ParameterValue` or `CommandDefinition` gains
  arms.
- `liquers-lib`: the registry exporter writes `!Pipeline`; no command changes.
- `liquers-py`, `liquers-web`: arms for the new variants in their plan and metadata bridges. No new
  API (Phase 1 non-goal).
- `liquers-mcp`: none; a pipeline command is listed and called as any command.

## Error Handling

| Failure | Constructor |
|---|---|
| Malformed pipeline document (unknown element key, filename not last) | `Error::general_error` from the custom `Deserialize`, naming the element index; as an `IssueReport` entry at registration |
| Undeclared placeholder, `injected` argument, unknown step argument, conflicting slot types | `Issue::CommandRegistry` (`Error`); `register_pipeline` returns new `Error::invalid_pipeline(key, report)` (`ParameterError`) |
| Cycle, or depth over 16 | new `Error::pipeline_cycle(chain)` (`ParameterError`) |
| Unresolved placeholder outside a definition; missing argument | `Error::missing_argument` with the element index |
| Positional and named for one slot; unknown or `injected` name | `ParameterError`, as for overrides (`not_in_last_action` reworded for elements) |
| Value that does not convert (V5) | `Error::conversion_error` |
| Error inside a body | the step's error plus new `Error::with_pipeline(key, element)`, " (in pipeline command 'sales/top_in', element 3 'sort')", like `with_alias` |

## Relevant Commands

None new. Namespaces in the examples: `pl`. A pipeline command lives in whatever namespace it is
registered under.

## Documentation Architecture

`affects_docs`: `reference/PIPELINE_COMMANDS.md` (new; internal; `core/commands`, `core/plan`:
pipeline shape, `Source`s, pipeline commands, planning, caching, variadic rules V1–V9);
`reference/COMMAND_ALIASES.md` (aliases versus pipeline commands); `reference/RECIPES_AND_PLANS.md`
(`Recipe::pipeline`, `Step::UseValue`, `Step::PipelineCommand`, the end of the P1 gap
"overrides … last action"); `reference/PROJECT_OVERVIEW.md` §Recipes (query encoding unchanged);
`reference/COMMAND_DECLARATION.md` §4.1 (`definition: !Pipeline`);
`guides/COMMAND_REGISTRATION_GUIDE.md` (registering a pipeline command);
`guides/COMMAND_DESIGN_GUIDE.md` §Aliases (alias, pipeline command or Rust). Each gets a History
row and a `reviewed:` bump. `specs/README.md` keeps linking the design until the reference exists.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | about 10 in `liquers-core`; match arms in `liquers-lib`, `liquers-py`, `liquers-web` |
| Crates and workflows affected | core, lib (export), py, web; `check-build-matrix.sh`; `registry_export` tests |
| Existing tests likely to change | none by behaviour. Moving `PlanBuilder` state into `PlanAssembly` is a refactor that the plan tests (`plan.rs`, `tests/command_alias.rs`) must keep green unchanged |
| New validation | text-form equality for every query in the parser corpus (`to_pipeline` then `to_plan` equals `PlanBuilder`, AC-24); expansion, `{state}` across nested levels, cycles, V1–V9, recipe compatibility |
| Compatibility / data / concurrency / performance / security | no query, recipe or plan changes meaning; serialized plans gain variants (inspection format, no guarantee). Query parsing is untouched. A value is never re-lexed, so it cannot add a step |
| Recovery | additive; reverting removes the variants and the field |
| Certainty and open questions | high for the delegation and override reuse. The `PlanAssembly` extraction is the largest change to existing code, and the `{state}` frame stack is new; Phase 3 must prove both |

## Decisions for the gate

1. **Nested `Step::PipelineCommand`** rather than flattened steps. Recommended (Overview).
2. **Text-form runs delegate to `PlanBuilder`.** Recommended: AC-10 and AC-24 then hold by
   construction.
3. **Recipe with both fields:** query, then pipeline, then the query's filename. Recommended: no
   recipe provider or key changes. Alternative: forbid both fields together.
4. **Bare scalars and arrays as `{value}` shorthand.** Recommended: yes, as registry defaults
   already allow.
5. **Positional and named under Python's rule** (V1, V2). Recommended. Alternative: two element
   kinds, never mixed.
6. **`PlanLink`** for a nested pipeline with no text form, evaluated unmanaged. Recommended.
   Alternative: refuse nested pipelines without a text form.

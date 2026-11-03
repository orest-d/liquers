# Phase 2: Solution & Architecture — Pipelines

Rewritten on 2026-11-03 for the Phase 1 approved that day, and revised the same evening after the maintainer's first Phase 2 review: `PipelinePlanBuilder` (renamed) plans everything, with `PlanBuilder` delegation as a switch; positions on every element and argument; the chained `ActionEditor`; `ParameterValue::Input` replacing the level-indexed `State`. That Phase 1 merges the query builder
(AC-25–AC-38) and adds realm scopes and `Value::Pipeline` (AC-39, AC-40). It also settles Q20–Q30.
This rewrite replaces the draft of 2026-11-01. Supporting analyses:
[`pipeline-structure-and-sources.md`](./pipeline-structure-and-sources.md) (struct, realms,
state elements),
[`recipe-and-pipeline-structures.md`](./recipe-and-pipeline-structures.md) (the recipe, the
hierarchy, inline versus evaluate) and [`pipeline-and-plan.md`](./pipeline-and-plan.md) (pipeline
versus plan).

## Overview

A **pipeline** is the structured, JSON-representable twin of a query. Representable pipelines and
queries convert both ways (AC-25–AC-30). A pipeline can also carry what a query cannot: JSON
arguments for any step, named arguments, placeholders, a value or a query result as the state,
and nested pipelines. Everything lives in the new module **`liquers_core::pipeline`**:

| File | Holds |
|---|---|
| `pipeline/mod.rs` | `Pipeline`, `PipelineElement`, `ActionElement`, `Source`, `Operand`, `Read`; serde; `bind`, `placeholders` |
| `pipeline/convert.rs` | `TryFrom<&Query> for Pipeline`, `to_query_with`, `Recipe::to_pipeline`, representability (`NotRepresentable`), the canonical form, namespace placement |
| `pipeline/editor.rs` | `PipelineEditor`, `ActionEditor`, `QueryEditor` |
| `pipeline/plan.rs` | `PipelinePlanBuilder` (`Pipeline::to_plan`), sharing the crate-private `PlanAssembly` with `PlanBuilder` |

The structures form the hierarchy decided for Q27 and Q30:

- **steps**, the body of a pipeline command: a `Pipeline` with no filename;
- **`Pipeline`**, query-equivalent: realm, rooted, header parameters, elements and filename;
- **`Recipe`**, the definition of an asset: an optional pipeline that runs before an optional
  query, plus `arguments`, `links`, `cwd`, title, description and the asset declarations (Q26).

Planning goes through the new **`PipelinePlanBuilder`** (named so it is not confused with the
editors: it builds a plan, not a pipeline). It is meant to **replace `PlanBuilder`** in the end
(revised 2026-11-03):

- **Every plan entry point** plans through `PipelinePlanBuilder`. A query is first converted with
  `TryFrom<&Query>`, which keeps every position. This covers `make_plan`, recipes, aliases,
  pipeline commands and `liquers-validate`.
- **Delegation is a switch, not the design.** In `PlanningMode::Delegate`, a representable run is
  converted to its query and handed to the existing `PlanBuilder` on the shared assembly. In
  `PlanningMode::Native`, `PipelinePlanBuilder` plans every element itself.
- **Both modes stay available.** A test plans every query of the parser corpus and every test
  query both ways and requires equal plans (AC-10). The default is `Delegate` until that test is
  green on the whole corpus, then `Native`. Going back is a one-line change. When `Native` has been
  the default for a release, `PlanBuilder` retires (`PLAN-BUILDER-DUPLICATES-PIPELINE-PLANNING`,
  filed with this revision).
- What has no query spelling always goes through the native code: structured values, named
  arguments, state elements, placeholders and nested pipelines.
- A **pipeline command** (`CommandDefinition::Pipeline`) expands into a nested
  `Step::PipelineCommand` from either builder.
- A value becomes the current state through the new `Step::UseValue`.

**Rejected alternatives:**
- Removing `PlanBuilder` at once. It is kept behind `PlanningMode::Delegate` until the native path is proven equal.
- Flattening a pipeline command into the caller's plan: `{state}` and `cut_predecessor` would need
  runtime markers.
- A pipeline over core `Value` (Q25 chose `serde_json::Value`).
- A recipe enum or trait (Q26: option A within D).
- Modelling header levels (out of scope; refused, AC-40).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `RECIPE-OVERRIDE-ONLY-LAST-ACTION` | draft | P3 | Owned here: recipe pipelines reach any step (AC-15) | — | — | Close when AC-15 ships |
| `QUERY-BUILDER-TOOLING` | draft | P2 | Owned here since 2026-11-02 (editors, predicates, `check_encodable`) | — | — | Close when AC-31–AC-36 ship |
| `COMMAND-NAMES-Q-AND-V-ARE-NOT-RESERVED` | draft | P3 | An action named `q` or `v` has no query spelling (rule E1) | No | No | E1 refuses them; its fix is unaffected |
| `LINK-IN-VARIADIC-DOES-NOT-EXPAND` | draft | P3 | A link inside a variadic is one element | No | No | Same rule (V7) |
| `VARIADIC-KEY-OR-QUERY-LINK-FAILS-TO-MATERIALIZE` | not-eligible | P3 | Same limit for `{link}` in `{multiple}` | No | No | Inherits its fix |
| `VARIADIC-EMPTY-ELEMENT-CONVERSION` | eligible | P3 | Defines the empty-element rule (V6) | No | No | Follow its rule |
| `VALUE-NOT-CONVERTED-AUTOMATICALLY` | draft | P2 | A link's result is not converted to the slot's type | No | No | Inherits its fix |
| `COMMAND-COMPOSITE-VARIADIC-ARGUMENTS` | draft | P3 | Tuple elements would let a list be one variadic element | No | No | V3 |
| `CORE-MULTI-REALM-INTERPRETER` | accepted | — | A realm the interpreter cannot honour keeps today's warning | No | No | Pipelines record realms (AC-40) |
| `plan-default-namespaces` (PR #236) | in review | — | Elements without `ns` and namespace placement use its chain | No | No | Uses the chain at HEAD |
| `resource-name-unicode` | in review | — | What a key segment is; both call `escape::is_resource_name` | No | No | Whichever lands second needs no change |

## Interfaces

### Pipeline types (`pipeline/mod.rs`)

Sync, pure data: `Serialize`, `Deserialize`, `Clone`, `Debug`. `PartialEq` ignores `pos`.

```rust
pub struct Pipeline {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub realm: String,                          // segment header name, resource or transform (AC-40)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub header_parameters: Vec<String>,         // carried as they are (P3)
    #[serde(default, skip_serializing_if = "is_false")]
    pub rooted: bool,                           // a leading `/`
    pub elements: Vec<PipelineElement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,               // top level only (P2)
}

pub struct Located<T> {                         // T plus where it came from (AC-31a)
    pub item: T,                                // serialized flat: `{action: …, pos: …}`
    pub pos: Option<Position>,                  // absent when written by hand or edited
}
pub type PipelineElement = Located<ElementKind>;
pub type Argument = Located<Source>;            // a bare scalar is still `{value: …}` with no pos

pub enum ElementKind {
    Text(String),                               // a bare string: query text, inlined (Q28)
    Action(ActionElement),                      // {action, ns?, parameters?, arguments?}
    Value(Operand<serde_json::Value>),          // {value: …}                 → UseValue
    Resource { resource: Operand<String>, read: Read },  // {resource, read?} → GetAsset* / GetResource*
    KeyValue(Operand<String>),                  // {key_value: …}             → UseKeyValue
    Cwd(Operand<String>),                       // {cwd: …}                   → SetCwd
    Evaluate(Operand<QueryOrPipeline>),         // {evaluate: …}              → Evaluate / nested plan
    QueryValue(Operand<QueryOrPipeline>),       // {query_value: …}           → UseQueryValue
    Quote,                                      // {quote: true}              → UseQueryValue of the prefix
    Volatile,                                   // {volatile: true}           → as `v`
    Pipeline(Pipeline),                         // {pipeline: {...}}: a realm change (AC-40)
}

pub enum Operand<T> { Literal(T), Placeholder(String) }   // a placeholder is `{placeholder: name}`
pub enum Read { Asset, Binary, Metadata, Recipe, Directory, Stored, StoredMetadata, StoredDirectory }
pub enum QueryOrPipeline { Query(String), Pipeline(Pipeline) }

pub struct ActionElement {
    pub action: String,
    pub ns: Option<Namespaces>,                 // a string or a list
    pub parameters: Vec<Argument>,              // positional, consumed first
    pub arguments: BTreeMap<String, Argument>,  // by name
}

pub enum Source {                               // externally tagged; a bare scalar or array = Value
    Value(serde_json::Value),
    Placeholder(String),
    Link(String),                               // query text
    Pipeline(Pipeline),                         // nested
    State(bool),                                // the pipeline command's input
    Multiple(Vec<Source>),                      // elements of a variadic argument
}

impl Pipeline {
    pub fn placeholders(&self) -> BTreeSet<String>;
    pub fn bind(&self, values: &BTreeMap<String, serde_json::Value>) -> Result<Pipeline, Error>; // AC-38
    pub fn to_plan(&self, cmr: &CommandMetadataRegistry) -> Result<Plan, Error>;
    pub fn to_query_with(&self, cmr: &CommandMetadataRegistry) -> Result<Query, Error>;  // NotRepresentable
}
```

**Positions.** `TryFrom<&Query>` keeps the position of every element and every parameter, which
the parser already records (`ActionRequest`, `ActionParameter`, segment headers). The planner
copies them into `Step::Action::position` and `ParameterValue::ParameterValue(…, Position)`, so a
plan made from a pipeline reports the same positions as one made from its query. An element or
argument without a position (written by hand, or edited) plans with `Position::unknown()`.
Equality and the canonical form ignore positions.

Deserialization is custom. An element is told apart by its single key, and an unknown key is an
error naming the key and the element index. A bare string is `Text`. `read` accepts the header
instructions' short spellings and writes the long ones. `serde_json` is built with
`float_roundtrip`. NaN and ±infinity use the non-finite token convention of `value.rs`
(`to_json` / `from_json`).

### Conversions (`pipeline/convert.rs`)

```rust
impl TryFrom<&Query> for Pipeline { type Error = Error; }      // syntactic: text parameters, positions kept
impl Recipe { pub fn to_pipeline(&self) -> Result<Pipeline, Error>; }   // AC-30
impl TryFrom<&PipelineEditor> for Query { type Error = Error; }
impl From<&QueryEditor> for Query {}                             // infallible (AC-28, AC-32)
impl TryFrom<&ParameterValue> for Pipeline { type Error = Error; }   // link-like variants (AC-18)
```

`TryFrom<&Query>` needs no registry and fails only for a query that `check_encodable` refuses, or
one with a header level other than `-` (AC-25, AC-40). `PipelineEditor::from_query(&q, cmr)` goes
through the registry: each text parameter takes its argument's declared type (S1), so a typed
scalar keeps its type. Every pipeline-to-query conversion needs the registry (Q20, N1).

**Representability** follows the Phase 1 rule table (P1–P4, E1–E8, S1–S7, N1–N4) in element
order. The first failure is
`Error::not_representable(rule: &'static str, path: ElementPath, detail: String)`. Its
`ElementPath` names the nested-pipeline indices, the element and the argument (by position or
name), and it carries `pos` when known.

**Canonical form**, produced by `to_query_with` and by every editor write-back:
- named arguments become positional (N2);
- namespaces are placed by the three-step algorithm of Phase 1;
- consecutive segments of one realm merge, and a nested pipeline with its parent's realm is
  inlined;
- `read` is written in its long spelling;
- a scalar for an `any` or `text` argument becomes text.

Idempotence (AC-27) follows, because every rule maps to a fixed point: a canonical query parses
to a pipeline that converts back to the same query.

**Recipe conversion.** The recipe's `pipeline` comes first, then the query's elements, with
`arguments` and `links` as named arguments of the query's last action. Then `cwd` is a leading
`Cwd`, `volatile` is a `Volatile`, and the filename comes from the query or the pipeline.
`title`, `description` and the asset declarations stay on the recipe.

### Editors (`pipeline/editor.rs`)

```rust
pub struct PipelineEditor {
    pipeline: Pipeline,
    cmr: Arc<CommandMetadataRegistry>,
    origin: Option<Query>,                     // the query it was opened from (AC-31a)
}
impl PipelineEditor {
    pub fn new(cmr: Arc<CommandMetadataRegistry>) -> Self;
    pub fn from_query(q: &Query, cmr: Arc<CommandMetadataRegistry>) -> Result<Self, Error>;
    pub fn from_pipeline(p: Pipeline, cmr: Arc<CommandMetadataRegistry>) -> Result<Self, Error>;
    // structure
    pub fn push(&mut self, e: PipelineElement) -> Result<&mut Self, Error>;
    pub fn insert(&mut self, at: usize, e: PipelineElement) -> Result<&mut Self, Error>;
    pub fn replace(&mut self, at: usize, e: PipelineElement) -> Result<&mut Self, Error>;
    pub fn remove(&mut self, at: usize) -> Result<&mut Self, Error>;
    pub fn move_to(&mut self, from: usize, to: usize) -> Result<&mut Self, Error>;
    pub fn set_realm(&mut self, realm: &str) -> Result<&mut Self, Error>;
    pub fn set_filename(&mut self, name: Option<&str>) -> Result<&mut Self, Error>;
    // shortcuts
    pub fn resource(&mut self, key: &str, read: Read) -> Result<&mut Self, Error>;
    pub fn action(&mut self, name: &str) -> Result<ActionEditor<'_>, Error>;    // appends, returns its editor
    pub fn edit_action(&mut self, at: usize) -> Result<ActionEditor<'_>, Error>;
    pub fn default_ns(&mut self, ns: Option<Namespaces>) -> &mut Self;          // for later `action` calls
    // registry access (AC-37)
    pub fn command_metadata(&self, at: usize) -> Result<&CommandMetadata, Error>;
    pub fn command_key(&self, at: usize) -> Result<CommandKey, Error>;
    pub fn argument_info(&self, at: usize, arg: ArgRef) -> Result<&ArgumentInfo, Error>;
    pub fn unset_arguments(&self, at: usize) -> Result<Vec<&ArgumentInfo>, Error>;
    pub fn resolved_namespaces(&self, at: usize) -> Result<Vec<String>, Error>;
    pub fn pipeline(&self) -> &Pipeline;
}
pub struct ActionEditor<'a> { /* borrows the editor and one action */ }
impl ActionEditor<'_> {
    pub fn ns(&mut self, ns: impl Into<Namespaces>) -> Result<&mut Self, Error>;
    pub fn set(&mut self, arg: impl Into<ArgRef>, v: impl Into<serde_json::Value>) -> Result<&mut Self, Error>;
    pub fn placeholder(&mut self, arg: impl Into<ArgRef>, name: &str) -> Result<&mut Self, Error>;
    pub fn link(&mut self, arg: impl Into<ArgRef>, query: &str) -> Result<&mut Self, Error>;
    pub fn pipeline(&mut self, arg: impl Into<ArgRef>, p: Pipeline) -> Result<&mut Self, Error>;
    pub fn state(&mut self, arg: impl Into<ArgRef>) -> Result<&mut Self, Error>;
    pub fn push(&mut self, v: impl Into<serde_json::Value>) -> Result<&mut Self, Error>; // next positional
    pub fn unset(&mut self, arg: impl Into<ArgRef>) -> Result<&mut Self, Error>;
    pub fn rename(&mut self, action: &str) -> Result<&mut Self, Error>;
    pub fn metadata(&self) -> Result<&CommandMetadata, Error>;           // AC-37, for this action
    pub fn argument_info(&self, arg: impl Into<ArgRef>) -> Result<&ArgumentInfo, Error>;
}
pub struct QueryEditor(PipelineEditor);
impl QueryEditor {
    pub fn new(cmr: Arc<CommandMetadataRegistry>) -> Self;
    pub fn from_query(q: &Query, cmr: Arc<CommandMetadataRegistry>) -> Result<Self, Error>;
    pub fn query(&self) -> Query;                // infallible
    // every PipelineEditor edit, each applied to a copy and kept only if `to_query_with` succeeds
}
```

The chain the maintainer asked for works as written, because `action` returns an
`ActionEditor` borrowing the editor, and each of its methods returns `Result<&mut Self, Error>`:

```rust
editor.action("new_action")?.ns("myns")?.set("a", 1)?.placeholder("b", "bplaceholder")?
      .link("c", "-R/text.txt")?;
```

**Namespaces and realms.** The realm is the pipeline's, shared by all of its actions. A nested
pipeline element changes it. `ns` is per action. `action(name)` checks only that `name` is an
identifier and appends the action with the editor's `default_ns`. The command is looked up when it
is first needed: by `ns`, which re-resolves it; by `set`, `placeholder`, `link` and the other
argument methods; and by `metadata`. So `.ns("myns")` can follow `.action(…)` even when the
default namespaces do not contain the command. An action left unresolved is reported by
`query()`, `to_plan` and `validate()`.

Every edit validates before it applies, and a refused edit leaves the editor unchanged (AC-31).
The checks are:
- the token predicates;
- command and namespace lookup through the registry;
- argument names, positions, `injected` arguments and duplicates;
- type conversion of a literal against the argument's `ArgumentType`;
- placeholders: a placeholder must be declared, and an editor opened outside a command definition
  refuses any placeholder (AC-11).

`QueryEditor` adds representability. An error in an edit carries `pos` and the query. For an
element changed since the editor was opened, the editor first re-encodes and re-parses the query to
get a fresh position, and attaches that query (AC-31a).

**Registry ownership.** `CommandRegistry` owns its `CommandMetadataRegistry` by value. The editors
need an `Arc`. Environments gain `fn command_metadata_snapshot(&self) -> Arc<CommandMetadataRegistry>`.
It is cached and replaced when a command is registered, so an editor sees the registry as it was
when the editor was opened. Gate decision 3.

### Token predicates and `check_encodable` (`escape.rs`, `query.rs`)

```rust
pub fn is_identifier(s: &str) -> bool;       // action, namespace
pub fn is_realm_name(s: &str) -> bool;
pub fn is_resource_name(s: &str) -> bool;    // key segment
pub fn is_header_value(s: &str) -> bool;
pub fn is_filename(s: &str) -> bool;
impl Query { pub fn check_encodable(&self) -> Result<(), Error>; }
```

These are the private character classes of `parse.rs` (`is_identifier_start`, …) made public as
whole-token predicates. The parser, `check_encodable`, the editors and
`liquers-template/src/query/instantiate.rs` all call them (AC-33, AC-34).

### Commands and defaults (`command_metadata.rs`, `commands.rs`)

```rust
pub enum CommandDefinition { Registered, Alias { … }, Pipeline { pipeline: Pipeline } }
pub enum CommandParameterValue { Value(Value), Query(Query), Pipeline(Pipeline), None }  // `{pipeline: …}`
impl<E: Environment> CommandRegistry<E> {
    pub fn register_pipeline(&mut self, key: CommandKey, pipeline: Pipeline,
        arguments: Vec<ArgumentInfo>) -> Result<&mut CommandMetadata, Error>;
}
```

A pipeline command's body has no filename. Its realm is the command's realm, and a nested element
may change it. Its placeholders are its declared `ArgumentInfo`s. The type of each placeholder
operand must fit its element: a key for `resource`, `key_value` and `cwd`; a query for `evaluate`
and `query_value`. `state_argument` is `None` when the first element sets the state (AC-5).
Omitted `argument_type`, `multiple` and `gui_info` are filled from the slot a placeholder first
fills (AC-3). The pipeline command has no `impl_version`, and its `metadata_version` covers the
pipeline.

The registry checks report each problem as an `Issue::CommandRegistry` (AC-6).
`CommandMetadata::check` finds:
- an undeclared placeholder or an `injected` argument;
- `{state}` in a source command;
- a filename in the body;
- an operand type that does not fit its element.

`CommandMetadataRegistry::check` finds:
- an unknown step command or argument;
- conflicting slot types for one placeholder;
- cycles.

### Values (`value.rs`)

`Value::Pipeline(Pipeline)`, with `TypeInfo::new("Pipeline").with_type_name("pipeline")`, default
filename `pipeline.json`, and formats `json` and `yaml`. It is also written as `txt` when it is
representable through the environment's registry, and refused otherwise.

`ValueInterface` gains two methods with default implementations, so no implementor changes:
- `try_into_pipeline`: a pipeline, a query (through `TryFrom<&Query>`), or JSON or YAML text;
- `from_pipeline`: the JSON form, or the native variant where one exists.

`ExtValue` delegates to its base (AC-39).

### Plan and interpreter (`plan.rs`, `pipeline/plan.rs`, `interpreter.rs`)

```rust
pub enum Step {
    // … existing …
    UseValue(Value),                                  // core Value, converted once from JSON
    PipelineCommand { realm: String, ns: String, name: String, position: Position,
        parameters: ResolvedParameterValues, query: Option<Query>, plan: Box<Plan> },
}
pub enum ParameterValue {
    // … existing …
    ParameterPipeline(String, Pipeline, Position),    // AC-18
    OverridePipeline(String, Pipeline),
    DefaultPipeline(String, Pipeline),
    Input(String),                                    // the innermost pipeline command's input (AC-4)
}
```

`PlanAssembly` (crate-private) holds what `PlanBuilder` keeps today:
- the plan under construction;
- `is_volatile`, `payload_required`, `expires` and `uncached_by`;
- the running prefix query, which becomes `None` after the first element that is not representable.

`apply_command_flags` and the step push move into it, and both builders call them.

The interpreter:
- `UseValue` returns its value.
- `PipelineCommand` runs its body through `apply_plan_state` on its input, which it pushes on a
  single frame, its input, which `ParameterValue::Input` reads. The frame is lexically scoped:
  a nested pipeline command's arguments, `{state}` included, are resolved by the outer step
  before the inner body runs. So a body only ever reads its own command's input, and no level
  index is needed.
- A pipeline argument is planned with `Pipeline::to_plan`:
  - when it is representable, it becomes `Evaluate(query)`, a shared asset;
  - otherwise it runs as a nested plan with an empty state. Its dependencies are recorded, but it
    has no asset of its own and is not cached (Q29).

### Recipes (`recipes.rs`)

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub pipeline: Option<Pipeline>,          // runs before `query` (Q26)
```

| `pipeline` | `query` | Plans as | Filename from |
|---|---|---|---|
| absent | set | today, unchanged | the query |
| set | set | the pipeline, then the query continuing from its state | the query; a pipeline filename is an error |
| set | empty | the pipeline | the pipeline |

The recipe is refused when:
- its pipeline has a placeholder;
- its query starts with a resource while a pipeline precedes it (the pipeline would run for
  nothing);
- it has `arguments` or `links` but no query.

`arguments` and `links` override the query's last action, as today. `Recipe::filename`,
`data_format`, `key` and every provider listing read the filename from the query or the
pipeline. A recipe whose pipeline is not representable has no query identity when it is unkeyed,
as a recipe with overrides has none today. Under a key it is managed as usual.

### `liquers-validate` (`validate/`, `cli` feature)

New inputs:
- `--pipeline <file|->`, JSON or YAML;
- `--recipe <file>`, which already reads a `recipes.yaml` and now accepts `pipeline`;
- `--build R:<key> A:<name> P:<param> …`, which spells a query as shell arguments and replaces
  the planned `liquers-encode` (Q24).

New outputs, on request:
- `--emit query|pipeline|plan`.

A pipeline that cannot be converted to a query exits 1 and names the rule. Exit codes stay 0, 1
and 2.

## Planning a pipeline

`PipelinePlanBuilder::build` walks the elements, keeping a stack of realms:

1. **A representable run of elements.** In `Delegate` mode it is converted with `to_query_with`
   and given to `PlanBuilder::process_query` on the shared assembly, prefixed by the running
   prefix query. In `Native` mode its elements are planned one by one, below. Either way, its
   steps and prefixes are the query's (AC-10, checked by the dual-mode test). `Text` elements are
   parsed into elements, with their positions, and join the run.
2. **`Action`** that is not representable: the command is looked up in the pipeline's realm, through
   the element's `ns`, then the default chain (AC-7). The positional part goes through
   `ResolvedParameterValues::from_action`, with placeholders allowed. Named arguments and
   structured values go through the override path (`override_value` / `override_link` /
   `convert_override`, made crate-visible) and the new pipeline variants. Placeholders are filled
   from the bindings, and any slot still unfilled is `missing_argument`.
3. **State elements** plan as their one step. A placeholder operand is substituted from the
   bindings first, and outside a definition it is an error (AC-11).
4. **`Pipeline`**, a realm change: its elements are planned in its realm. The interpreter is
   single-realm, so a realm it cannot honour keeps today's warning.
5. **Filename**: `Step::Filename` at the end of the top-level pipeline.

**Pipeline command expansion** happens in either builder's action arm:
- The call is resolved against the command's own arguments, so errors name the pipeline command.
- The resolved arguments become bindings (`Value`, `Link`, `Pipeline`, `Input`). A placeholder
  bound to the caller's `{state}` refers to the caller's input. It is resolved by the
  `PipelineCommand` step in the caller's frame and passed in as an ordinary argument value.
- The body is built with a child `PipelinePlanBuilder`, keeping an expansion stack. A
  repeated key, or a depth over 16, is `Error::pipeline_cycle`.
- Volatility, payload and expiration fold into the caller, as for an alias, and the builder
  pushes `Step::PipelineCommand`.
- `cut_predecessor` never cuts inside it.
- `finalize_plan` freezes the body's relative operands against the caller's working key.
- The dependency scan descends into the body and records the command's metadata key.

## Variadic arguments

Unchanged from the draft of 2026-11-01, except that the values are JSON. A **variadic** argument
is declared `multiple`.

| # | Case | Rule |
|---|---|---|
| V1 | Positional `parameters` | Fill slots in order; from the variadic slot on, every remaining source is one element |
| V2 | Named `arguments` | Fill slots the positional ones did not reach. A name for a slot already filled is a `ParameterError`. A variadic by name takes `{multiple: […]}`, an array, or a placeholder bound to an array |
| V3 | An array for a variadic | Its entries are the elements. A list *element* only where the element type allows it (`any`, or tuples of `COMMAND-COMPOSITE-VARIADIC-ARGUMENTS`), written `{multiple: [[a, b]]}` |
| V4 | Splicing | A placeholder bound to an array, inside `{multiple}` or a positional variadic tail, is spliced in place; a scalar binding is one element |
| V5 | An array for a single argument | An `any` argument receives the whole array; a scalar-typed one is a `ConversionError`. Not representable (S4) |
| V6 | Empty array | Variadic: no elements. `any`: the value `[]`, not the default |
| V7 | Links | `{link}` or `{pipeline}` in a variadic is one element (`LINK-IN-VARIADIC-DOES-NOT-EXPAND`) |
| V8 | A pipeline command's own variadic | Its last parameter-consuming argument; from a query it takes the remaining parameters, from a pipeline V1–V4 apply |
| V9 | `ns` | A string or a list of identifiers; never a placeholder |

## Integration Points

- **`liquers-core`**:
  - new: `pipeline/` (four files);
  - `escape.rs` (predicates) and `parse.rs` (uses them);
  - `query.rs` (`check_encodable`);
  - `command_metadata.rs` (variant, `CommandParameterValue::Pipeline`, checks, export);
  - `commands.rs` (`register_pipeline`, registry snapshot);
  - `plan.rs` (`PlanAssembly`, new `Step` and `ParameterValue` variants, the `Pipeline` arm, cut
    and dependency scan);
  - `interpreter.rs` (new steps, the input frame, pipeline arguments);
  - `recipes.rs` (`pipeline`, rules, filename);
  - `value.rs` (`Value::Pipeline`, `TypeInfo`, `ValueInterface` defaults);
  - `error.rs` (constructors);
  - `validate/` (inputs and emit).

  Every exhaustive `match` over `Step`, `ParameterValue`, `CommandDefinition`,
  `CommandParameterValue` or `Value` gains arms.
- **`liquers-lib`**:
  - the `ExtValue` delegation for `Value::Pipeline`;
  - the registry exporter writes `!Pipeline` definitions and pipeline defaults;
  - `check-build-matrix.sh`, because the `match` arms are spread over feature-gated code.
- **`liquers-template`**: its `instantiate.rs` validators delegate to the core predicates.
- **`liquers-web`**: `encode_param("")` returns `""` (AC-36); match arms. No pipeline API yet
  (`PIPELINES-NOT-IN-LANGUAGE-BINDINGS`).
- **`liquers-py`**: match arms only.
- **`liquers-mcp`**: none; a pipeline command is listed and called like any command.
- **Follow-ups**:
  - `MANIFEST-TEMPLATE-BATCH-ONLY-AT-THE-END-OF-THE-QUERY` uses `Pipeline::bind`;
  - `RECIPES-CANNOT-DESCRIBE-A-SELF-CONTAINED-ASSET-TREE` builds a `TreeEditor` on
    `PipelineEditor`.

## Error Handling

| Failure | Constructor |
|---|---|
| Malformed pipeline document (unknown key, a nested filename, a header level) | `Error::invalid_pipeline(detail)` (`ParameterError`), naming the element path |
| Not representable | `Error::not_representable(rule, path, detail)` (`ParameterError`) |
| Invalid token in an edit or in `check_encodable` | new `Error::invalid_token(value, class)` (`ParameterError`) |
| Unknown command or namespace; unknown, `injected` or duplicate argument; wrong type | existing `Error::action_not_registered`, `too_many_parameters` and `conversion_error`, plus new `Error::invalid_argument(command, argument, problem)`, all with `with_position` and `with_query` when known |
| Registration problems | `Issue::CommandRegistry`; `register_pipeline` returns `Error::invalid_pipeline` carrying the report |
| Cycle or depth over 16 | `Error::pipeline_cycle(chain)` |
| Unresolved placeholder outside a definition; missing argument | `Error::missing_argument`, with the element path |
| Recipe rule broken (dead pipeline, filename in both, overrides without a query, placeholder) | `Error::invalid_pipeline`, naming the recipe |
| Error inside a pipeline command | the step's error, plus `Error::with_pipeline(key, path)`, like `with_alias` |

## Relevant Commands

None new. The examples use `pl` (polars) and a hypothetical `sales` namespace. A pipeline command
lives in whatever namespace it is registered under.

## Documentation Architecture

| Document | Change |
|---|---|
| `reference/PIPELINES.md` (new) | the model and serde form, elements and sources, pipeline commands, recipe pipelines, planning and caching, representability rules, canonical form, editors, `Value::Pipeline`, variadic rules |
| `guides/PIPELINE_GUIDE.md` (new) | building and editing pipelines in Rust, converting queries and back, pipeline commands, recipe pipelines, handling a not-representable error |
| `reference/PROJECT_OVERVIEW.md` | §Query Language and §Recipes refer to pipelines; Technical Debt's raw-emission caveat |
| `reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md` | §Programmatic construction (`QueryEditor`, `check_encodable`); segment names are realms |
| `reference/COMMAND_ALIASES.md`, `reference/COMMAND_DECLARATION.md` §4.1 | pipeline commands; `definition: !Pipeline`; pipeline defaults |
| `reference/RECIPES_AND_PLANS.md` | `Recipe::pipeline`, new steps and parameters, the P1 "last action only" gap closed |
| `reference/VALUE_TYPE_SYSTEM.md`, `guides/TYPE_SYSTEM_GUIDE.md` | `Value::Pipeline` |
| `reference/REALMS.md` | pipelines as realm scopes; resource names are realms |
| `reference/TEMPLATE_LANGUAGE.md` | shared predicates |
| `guides/COMMAND_REGISTRATION_GUIDE.md` | a section on the pipeline alias: when to use it, declaring placeholders, `register_pipeline`, link to the reference |
| `guides/COMMAND_DESIGN_GUIDE.md` §Aliases | alias, pipeline command or Rust |
| `guides/QUERY_ESCAPING_GUIDE.md` | the editor first, then `liquers-validate --build` |
| `guides/LANGUAGE-INTEGRATION_GUIDE.md` | §Pipelines: what a binding must support |
| `CLAUDE.md`, `.claude/skills/liquers-validate/` | pipeline input and the emit options |

Each `reference/` and `guides/` change gets a History row and a `reviewed:` bump. `specs/README.md`
links the design until `PIPELINES.md` exists.

## Risks

| Assessment | Finding |
|---|---|
| Size | XL: about 15 files in core, arms in lib, py, web and template. Phase 4 should split it into landable slices: (1) types, serde, conversions, predicates, editors; (2) `Value::Pipeline`; (3) the planner, `UseValue` and recipes; (4) pipeline commands; (5) the validator and documents |
| Existing tests | none should change behaviour. The `PlanAssembly` extraction is the largest edit to existing code, and the plan tests and `tests/command_alias.rs` must stay green unchanged |
| New validation | for every query in the parser corpus: `TryFrom` then `to_query_with` reproduces its canonical form, and the plans are equal (AC-10, AC-26, AC-27); also the representability rules one by one, editors, `{state}` across nesting levels, cycles, V1–V9, recipe rules, realm merging |
| Compatibility | no query, recipe or plan changes meaning. Serialized plans and registry exports gain variants. `serde_json` gains `float_roundtrip`, which changes float parsing everywhere it is used, towards exactness |
| Concurrency | the registry snapshot is immutable; editors are single-owner |
| Security | a pipeline value is never re-lexed into query text, so it cannot add steps |
| Certainty | high for delegation and conversions. The input frame and the snapshot invalidation are new, and so is the native planning of every query element, which the dual-mode test checks, and Phase 3 must prove both |

## Decisions for the gate

1. **`PipelinePlanBuilder` for every plan, with `PlanningMode::{Delegate, Native}`**, a dual-mode
   equality test, and `Delegate` as the default until that test passes on the whole corpus.
   `PlanBuilder` then retires. Recommended (maintainer's direction, 2026-11-03).
2. **Nested `Step::PipelineCommand`** rather than flattened steps. Recommended.
3. **Registry snapshot**: `Arc<CommandMetadataRegistry>`, cached and replaced on registration.
   Recommended. The alternative, moving the registry into an `Arc` inside `CommandRegistry`, would
   make every registration copy-on-write.
4. **`serde_json` with `float_roundtrip`**, workspace-wide. Recommended. The alternative is a
   pipeline-local number type.
5. **`liquers-validate --build`** replacing `liquers-encode`, with `R:`/`A:`/`P:` prefixes.
   Recommended. The alternative is positional groups separated by `--`, as the superseded design
   had.
6. **Five landable slices** (Risks). Recommended, for Phase 4 to refine.

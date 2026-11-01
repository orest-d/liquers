# Phase 2: Solution & Architecture — Composite command definitions

Drafted together with Phase 1, before Phase 1's gate, so that the three uses can be reviewed
against one mechanism. It follows Phase 1's recommended answers; a different answer at the gate
reopens this phase.

## Overview

One mechanism, used three ways. A **placeholder** is a new kind of query parameter (`$name`) and a
new leading query element (`$name/…`). The **plan builder binds** placeholders from a binding map,
by name, while it resolves each action's parameters, so a bound value enters the plan as the
`ParameterValue` a written parameter or a link would have produced, with any JSON value allowed. A
**composite command** is registry metadata whose body is a placeholder query. The builder expands
it into a nested plan bound from the call's resolved arguments. A **recipe** and an **HTTP
request** pass their named values as bindings. When every binding has a text form, the builder
also records the substituted prefix query on each step, so caching and sharing behave as if the
values had been written in place.

Rejected alternatives:

- **Textual macro expansion** (substitute, re-encode, re-parse): a list, map or long text has no
  text form, and re-lexing values is the injection the template language's hygienic slots exist to
  prevent (`TEMPLATE_LANGUAGE.md` §2).
- **Reusing `liquers-template`'s `QueryTemplate`**: `liquers-core` cannot depend on it, and its
  `{{x}}` slots are rendered by Mustache, not bound by the planner.
- **Positional addresses** (`1.values`, `design/recipe-override-addressing/`): the caller must
  count actions, and the address breaks when the query is edited. A placeholder is written where the
  value goes. (Phase 1, question 2.)
- **Composite as a Rust executor** that evaluates its body with `Context::evaluate`: it would hide
  the body from planning, so no volatility, payload, expiration or dependency of the body would be
  known before running it.
- **Generalising `CommandDefinition::Alias`**: kept unchanged. It plans to a single `Step::Action`,
  it is pinned by `liquers-core/tests/command_alias.rs`, and an alias can be described as a
  one-command composite in the documentation without migrating it.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `RECIPE-OVERRIDE-ONLY-LAST-ACTION` | draft | P3 | Solved for queries that use placeholders; last-action overrides stay as they are | No | No | Close it as covered when this ships, if Phase 1 question 2 is accepted |
| `RECIPE-CANNOT-PARAMETERIZE-RESOURCE-KEY` | draft | P3 | Keys are a non-goal; `@key` remains its answer | No | No | None |
| `LINK-IN-VARIADIC-DOES-NOT-EXPAND` | draft | P3 | A link bound to a `multiple` slot gives one element, as a written link does | No | No | Same rule here; documented |
| `VARIADIC-KEY-OR-QUERY-LINK-FAILS-TO-MATERIALIZE` | not-eligible | P3 | Same limit for bound links inside a `multiple` slot | No | No | Inherits its fix |
| `VALUE-NOT-CONVERTED-AUTOMATICALLY` | draft | P2 | A bound link's result is not converted to the slot's type, as with written links | No | No | Inherits its fix |
| `plan-default-namespaces` (PR #236) | in review | — | AC-7 resolves body commands through its default namespace chain (per-realm defaults are deferred to `COMMAND-REGISTRY-NOT-ORGANIZED-BY-REALM`) | No | No | Uses whichever chain is at HEAD |

## Interfaces

**Query (`liquers-core/src/query.rs`, `parse.rs`).** Sync: pure syntax.

```rust
pub enum ActionParameter {
    String(String, Position),
    Link(Query, Position),
    /// `$name`: filled from a binding when the query is planned.
    Placeholder(String, Position),
}

pub enum QuerySegment {
    Resource(ResourceQuerySegment),
    Transform(TransformQuerySegment),
    /// `$name/…`: the query starts from the value bound to `name`. First segment only.
    Placeholder(PlaceholderSegment),
}
pub struct PlaceholderSegment { pub name: String, pub position: Position }

impl Query {
    /// Placeholder names in order of first occurrence, links included.
    pub fn placeholders(&self) -> Vec<String>;
}
```

`$` is already entity-encoded by the encoder (it is no unescaped parameter character), so a literal
`$` in a string parameter keeps its current encoding. A `$name` anywhere other than a whole
parameter or the first element is a `ParseError`.

**Bindings (`liquers-core/src/plan.rs`).** Serde, owned.

```rust
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Bindings(pub BTreeMap<String, Binding>);   // BTreeMap: deterministic order

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Binding {
    Value(Value),
    Link(Query),
    /// The input state of the composite at expansion depth `level`.
    State { level: u16 },
}

impl<'a> PlanBuilder<'a> {
    pub fn with_bindings(self, bindings: Bindings) -> Self;
}
```

`ResolvedParameterValues::from_action` resolves a `Placeholder` like the parameter it stands for:
`Value` → `ParameterValue::ParameterValue` (converted by the slot's `ArgumentType`, as
`convert_override` does today; a list into a `multiple` slot gives one element per entry, spliced
among written elements), `Link` → `ParameterLink`, `State` → the new
`ParameterValue::State(name, level)`, unbound → the slot's default, as an empty parameter. A
`PlaceholderSegment` plans as `Step::UseValue(Value)` (new), `Step::Evaluate(query)` for a link,
nothing for `$state`, and `missing_argument` when unbound.

**Text form (AC-10, AC-14).** A binding is *text-form* when it is a string, number, boolean, or a
list of those bound to a `multiple` slot. While every binding consumed so far is text-form, the
builder keeps `Step::Action::query` as the substituted prefix (each value encoded with
`encode_token`, list elements as separate parameters). The first other binding clears it for that
step and every later one, exactly as a recipe override does today. `Plan::text_form_query()`
returns the final step's query, if any. A **start placeholder bound to a value is never
text-form**, whatever the value: the query grammar has no literal-value element, so `Step::UseValue`
has no query text (a CSV string written first would read as an action). A request whose query
starts that way is unmanaged (AC-14). A start placeholder bound to a link is text-form when the
link is: it plans as `Step::Evaluate(link)`, whose text is the link query itself.

**Commands (`command_metadata.rs`, `commands.rs`).** Sync: registration.

```rust
pub enum CommandDefinition {
    Registered,
    Alias { command: CommandKey, head_parameters: Vec<CommandParameterValue> },
    /// Body as encoded query text, so the registry YAML stays readable.
    Composite { body: String },
}

impl CommandRegistry<E> {
    pub fn register_composite(&mut self, key: CommandKey, body: &str,
        arguments: Vec<ArgumentInfo>) -> Result<&mut CommandMetadata, Error>;
}
impl CommandMetadataRegistry {
    /// Parses and checks a composite's body (AC-6); also called by the planner.
    pub fn composite_body(&self, composite: &CommandMetadata) -> Result<Query, Error>;
}
```

`register_composite` parses the body, checks that every placeholder is an argument or the state
argument's name, infers the type, `multiple`, default and `gui_info` of an argument declared as
`ArgumentType::Any` with no default from the first slot it fills (Phase 1, question 4), and sets
`state_argument: None` when the body starts with a resource segment or a placeholder other than
`$state`. Like `register_alias`, it needs the body's commands to be registered first. A
deserialized composite is not inferred: it is planned as declared. An unused argument is a warning
written to stderr at registration and an `Info` in the plan.

YAML: `definition: !Composite { body: "ns-pl/from_csv/is_in-region-$regions/sort-$by-f/head-$n" }`.
A composite has no executor and no `impl_version`. Its `metadata_version` covers the body.

**Plan step (`plan.rs`).**

```rust
Step::Composite {
    realm: String, ns: String, name: String,       // the composite's key
    position: Position,
    parameters: ResolvedParameterValues,           // the composite's own arguments
    query: Option<Query>,                          // the calling prefix, as for Action
    plan: Box<Plan>,                               // the expanded body
}
```

## Planning

`PlanBuilder::process_action` gains a third arm for `CommandDefinition::Composite`:

1. Resolve the call against the composite's own arguments (`from_action`), so arity, defaults,
   `ParameterError` and `missing_argument` name the composite.
2. Turn them into the body's `Bindings`: each resolved argument becomes `Value` / `Link`, a
   `ParameterValue::State` passes on unchanged, and the state argument's name becomes
   `State { level: depth }`.
3. Build the body with a child `PlanBuilder`: the same registry, those bindings, the composite's
   realm, its own namespace prepended to the default namespace chain (AC-7: the caller's `ns` does
   not apply), and the expansion stack plus this key. A key already on the stack is
   `Error::composite_cycle`; a depth over 16 is the same error.
4. Fold the child plan's volatility, payload requirement and expiration into the parent, with the
   composite's own flags (as an alias does), and push `Step::Composite`. The child keeps its own
   prefixes (`query` per step), so a body prefix such as `-R/data/sales.csv/-/ns-pl/from_csv` is
   shared with direct queries (AC-10).

**Recipe overrides of a composite.** `Recipe::to_plan` splits its names: a name that is a
placeholder of the recipe's query becomes a binding (`with_bindings`), and the rest keep today's
meaning. When the last action is a composite, the remaining names are passed to the builder as that
action's bindings before expansion, because overriding `Step::Composite::parameters` after building
would leave the expanded body stale. For any other last action the post-build override loop is
unchanged.

**Cut and freeze.** `Plan::cut_predecessor` treats `Step::Composite` as one action: no boundary falls
inside its body. `finalize_plan` freezes the body's relative operands against the caller's working
key, the rule a command's default link follows.

## Integration Points

- `liquers-core/src/parse.rs`, `query.rs`: the two grammar forms, encoding, `placeholders()`; every
  exhaustive match over `ActionParameter` and `QuerySegment` (`plan.rs`, `validate/`).
- `command_metadata.rs`: the variant, `composite_body`, the dependency key; `commands.rs`:
  `register_composite`.
- `plan.rs`: `Bindings`, `ParameterValue::State`, `Step::Composite`, `Step::UseValue`, the
  composite arm, text-form prefixes; `recipes.rs`: binding split in `to_plan`, `text_form_query`.
- `interpreter.rs`: `do_step_state` runs `Step::Composite` through `apply_plan_state` and pushes its
  input on a per-evaluation frame stack that `ParameterValue::State(_, level)` reads;
  `schedule_plan_dependencies_from` and the dependency scan descend into the body and add the
  composite's metadata key; `Step::UseValue` returns its value.
- `liquers-core/src/validate/`: placeholders and composites in `liquers-validate` output; the
  registry exporter writes `!Composite`.
- `liquers-axum/src/api_core/overrides.rs` `OverrideRequest::to_recipe`: accept placeholder names
  of the query; when `Recipe::text_form_query` gives a query and no last-action override is
  present, serve that query through the managed path (AC-14). WebSocket `subscribe` uses the same
  function. No new route.
- `liquers-py`, `liquers-web`: the new enum variants pass through their query and metadata
  bridges; their exhaustive matches need an arm. No new binding API.

## Error Handling

| Failure | Constructor |
|---|---|
| Malformed `$`, or a placeholder segment that is not first | `ParseError` from `parse.rs`, positioned |
| Body does not parse, unknown placeholder, two slots disagree on an inferred type | new `Error::invalid_composite(key, problem)` (`ParameterError`) |
| Cycle or depth over 16 | new `Error::composite_cycle(chain)` (`ParameterError`) |
| Binding name that is neither a placeholder nor a last-action argument | existing `not_in_last_action`, message extended with "or a placeholder" |
| Unbound start placeholder; required composite argument missing | `Error::missing_argument` |
| Bound value that does not convert; list or map inside a link (question 5) | `Error::conversion_error` |
| Error raised in the body | the step's error plus `Error::with_composite(key, step)`: " (in composite 'sales/top_in', step 3 'sort')", like `with_alias` |

## Relevant Commands

No new command. Example composites only (`sales/top_in` in Phase 1). Namespaces involved: `pl`
in the examples; a composite lives in whatever namespace it is registered in.

## Documentation Architecture

`affects_docs`: `reference/COMPOSITE_COMMANDS.md` (new; internal; `core/commands`, `core/plan`),
`reference/COMMAND_ALIASES.md`, `reference/RECIPES_AND_PLANS.md`,
`reference/WEB_API_SPECIFICATION.md`, `reference/PROJECT_OVERVIEW.md`,
`reference/COMMAND_DECLARATION.md`, `guides/COMMAND_REGISTRATION_GUIDE.md`,
`guides/COMMAND_DESIGN_GUIDE.md`, `guides/WEB_API_GUIDE.md`. `specs/README.md` links the design
under command definitions now, and the reference once Phase 5 completes.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | about 15 in `liquers-core`, 2 in `liquers-axum`, match arms in `liquers-py` and `liquers-web` |
| Crates and workflows affected | core, axum, py, web; `check-build-matrix.sh` (new enum arms); the registry export |
| Existing tests likely to change | none by behaviour; exhaustive-match tests over `Step` and `QuerySegment` gain arms |
| New validation | parser round-trip of `$`, plan equality for text-form bindings (AC-10), dependency invalidation on body edit, cycle detection, HTTP managed-path selection |
| Compatibility / data / concurrency / performance / security | no existing query changes meaning; serialized plans gain two variants, older readers cannot load a plan using them. Binding never re-lexes a value, so a bound string cannot add a segment or an action. Parsing pays one extra branch only on `$` |
| Recovery | each crate's change is additive; reverting removes the variants |
| Certainty and open questions | high for the planner path, which reuses the alias and override machinery; the frame stack for `$state` across nested composites is the least tested idea, and Phase 3 must prove it |

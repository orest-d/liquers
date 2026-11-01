# Phase 1: High-Level Design — Composite commands and pipelines

## Purpose

Let a sequence of commands be defined without Rust as one command, using a **pipeline**: a
structured, directly serializable form of a query in which any argument of any step can be a
literal value, a placeholder, a link, a nested pipeline or the input state. Recipes use the same
pipeline to carry complex data that query text cannot express. This keeps the query the one
addressable unit (VISION) for reusable pipelines: a long or structured value is written once, in a
definition or a recipe, and every caller writes a short, cacheable query.

## Terminology

- **Pipeline**: an ordered list of **pipeline elements**, serialized as JSON, YAML or TOML. An
  element is an action (`{action, ns?, realm?, arguments}`), a resource read (`{resource, mode?}`),
  embedded query text (`{query}`), a value that becomes the current state (`{value}` or
  `{placeholder}`, planned as `Step::UseValue`), or a filename. Full shape:
  [`notation-and-integration.md`](./notation-and-integration.md) §2.
- **Source**: what an action argument is given, by name: `{value: …}` (any JSON), `{placeholder:
  name}`, `{link: "<query>"}`, `{pipeline: […]}` (a nested pipeline evaluated as a link), `{state:
  true}` (the input state of the enclosing composite), or `{multiple: […]}` (elements of a variadic
  argument).
- **Placeholder**: a named hole in a pipeline. It exists **only in pipelines**; query text gains no
  placeholder syntax (decided 2026-11-01).
- **Composite command**: `CommandDefinition::Composite { pipeline }`, a command whose arguments are
  its declared placeholders and whose body is a pipeline. Calling it runs the body on its input with
  the arguments bound to the placeholders. (This is unrelated to the *composite variadic elements*
  of `design/composite-variadic-arguments/`.)
- **Text form**: a pipeline whose sources are all scalars, lists of scalars for variadic arguments,
  or links has an equivalent query. It plans exactly as that query does.

## Problem Example

A pipeline every caller repeats today (validated with `liquers-validate`):

```
-R/data/sales.csv/-/ns-pl/from_csv/is_in-region-EU-US/sort-amount-f/head-5
```

An alias cannot name this: it runs three commands, and `pl/is_in` fixes `column = region`, which
is not a leading argument of any single target. **Should**, as a composite in the registry:

```yaml
- namespace: sales
  name: top_in
  arguments:
  - { name: by, argument_type: string, default: !Value amount }
  - { name: n, argument_type: int, default: !Value 10 }
  - { name: regions, argument_type: string, multiple: true }
  definition: !Composite
    pipeline:
    - { action: from_csv, ns: pl }
    - { action: is_in, ns: pl, arguments: { column: { value: region }, values: { placeholder: regions } } }
    - { action: sort, ns: pl, arguments: { column: { placeholder: by }, ascending: { value: false } } }
    - { action: head, ns: pl, arguments: { n: { placeholder: n } } }
```

With it, `-R/data/sales.csv/-/ns-sales/top_in-amount-5-EU-US/ns-pl/tail-2` mixes the two forms:
a query, the composite's pipeline, then the query again. It returns the same rows as the long
query, cached under its own query like any command's result.

As a **recipe**, the list goes to `is_in`, which is not the last action. That is impossible today,
because overrides reach only the last action:

```yaml
- query: -R/data/sales.csv/-/ns-pl/from_csv
  pipeline:
  - { action: is_in, ns: pl, arguments: { column: { value: region }, values: { value: [EU, US] } } }
  - { query: ns-pl/sort-amount-f/head-10 }
  - { filename: eu.csv }
```

## Scope and Acceptance Criteria

Composite commands in the registry (a):

- **AC-1** Composite runs its pipeline
  WHEN a registered composite is called from a query with positional parameters
  THEN the result equals running its pipeline on the composite's input, with each placeholder replaced by the matching argument
- **AC-2** Arguments in any order and position
  WHEN a composite's arguments are ordered differently from the steps they fill, or one placeholder is used twice
  THEN each use receives its argument
- **AC-3** Declared placeholders and defaults
  WHEN an argument is omitted
  THEN its declared default applies; with no default it is `ArgumentMissing` naming the composite argument; an omitted `argument_type`, `multiple` or `gui_info` is taken from the slot the placeholder fills
- **AC-4** Input state as an argument
  WHEN a step argument's source is `{state: true}`
  THEN that argument receives the composite's input state
- **AC-5** A value as the current state
  WHEN a pipeline element is `{value: …}` or `{placeholder: name}`
  THEN the following step receives that value as its state (`Step::UseValue`); a composite whose first element is one of these is a source command (`state_argument: None`)
- **AC-6** Registration validation
  WHEN a composite is registered or deserialized, and a placeholder is not a declared argument, an argument is `injected`, or an argument name is not an argument of its step's command
  THEN registration fails with `ParameterError`, and nothing is registered; an argument that no placeholder uses is a warning
- **AC-7** Hygiene and nesting
  WHEN a composite's steps name commands
  THEN they resolve through each element's own `ns` and `realm`, else the default namespaces, never the caller's `ns`; a pipeline may call aliases and composites, and a cycle is a planning error naming the cycle
- **AC-8** Caching, dependencies and errors
  WHEN a composite's result is computed
  THEN it is cached under the calling query; the composite's metadata is a dependency, so editing its pipeline invalidates the result; the steps' volatility, payload requirement and expiration apply; and an error in a step names the composite and the step

The pipeline form, everywhere:

- **AC-10** Text-form identity
  WHEN a pipeline is text-form
  THEN its plan, including each step's prefix query, equals the plan of its equivalent query, so its prefixes are shared with direct queries
- **AC-11** Unbound placeholder
  WHEN a placeholder of an argument has no binding
  THEN the argument's default applies, as for an omitted parameter; a value element (`{placeholder}`) with no binding is `ArgumentMissing`
- **AC-16** Queries and pipelines combine in one plan
  WHEN a query calls a composite and continues after it, or a pipeline contains `{query: …}` elements, or a recipe has a query and a pipeline
  THEN the planner processes query actions and pipeline elements into one plan, in order, each receiving the previous one's state
- **AC-17** Named arguments
  WHEN an action element gives arguments by name, in any order, leaving some out
  THEN each is matched to its `ArgumentInfo`; the omitted ones take their defaults; an unknown or `injected` name is a `ParameterError`
- **AC-18** Nested pipeline as an argument
  WHEN an argument's source is `{pipeline: […]}`
  THEN it is evaluated like a link and recorded as a dependency; a text-form nested pipeline is exactly the link with its query
- **AC-19** Variadic arguments
  WHEN a variadic argument gets a list, a `{multiple: […]}` of sources, or a placeholder bound to a list among literal elements
  THEN it receives one element per entry, with the list spliced in place; a list for a non-variadic slot, unless its type is `any`, is a `ParameterError`
- **AC-20** Injection and payload are forwarded
  WHEN a step's command has `injected` arguments or `payload: required`
  THEN the arguments are injected from the caller's context and the composite is `payload: required`, exactly as for a direct call
- **AC-21** Round trip and validation
  WHEN a composite or a pipeline is serialized (registry export, YAML, JSON) and read back, or given to `liquers-validate`
  THEN it is unchanged, and the validator plans it, reporting its text form when it has one

Recipes (b):

- **AC-15** Recipe pipeline
  WHEN a recipe has a `pipeline`, alone or after its `query`
  THEN the pipeline runs on the query's result (or from nothing); `arguments` and `links` bind the pipeline's placeholders; the asset name comes from the pipeline's `filename` element; a name that is no placeholder keeps today's meaning (an argument of the last action)
- **AC-22** Composites defined by recipes
  WHEN a `recipes.yaml` entry declares `command: <ns>/<name>` and `parameters` (`ArgumentInfo`s)
  THEN the environment registers it as a composite whose pipeline is the entry's query and pipeline, when the recipe provider is loaded

Retired:

- ~~**AC-9**~~ placeholders in recipe query text: query text has no placeholders (2026-11-01), so AC-15 replaces it.
- ~~**AC-12**, **AC-13**, **AC-14**~~ web API binding, request values as the start state, managed text forms: moved to [`WEB-API-CANNOT-RUN-PIPELINES-OR-RECIPE-ENDPOINTS`](../../issues/WEB-API-CANNOT-RUN-PIPELINES-OR-RECIPE-ENDPOINTS.md) (scope split, 2026-11-01).

Non-goals: placeholder syntax in query text (no change to query encoding); web API routes,
endpoint recipes, MCP serving and payload sources (the follow-up issue above); an `injected`
composite argument; replacing `CommandDefinition::Alias`; Python and JavaScript registration
helpers (both read the metadata form as it is).

**Web API in this design.** No route changes. HTTP overrides keep working against a composite: a
composite's arguments are its own `ArgumentInfo`s, so `?n=5` on `…/ns-sales/top_in` sets `n` as for
any command.

## Core Interactions

- Commands: `CommandDefinition::Composite`, registration, validation and export in the metadata registry.
- Plans: `PlanBuilder` takes pipeline elements as well as queries, and expands composites into
  them; new `Step::UseValue`; the interpreter supplies `{state: true}`.
- Assets / recipes: `Recipe` gains `pipeline`; the recipe provider registers composites (AC-22).
- Validation: `liquers-validate` accepts pipelines; `specs/command_registry.yaml` carries composites.
- Consumers: `liquers-mcp` serves a composite as a tool like any command.

## Crate Placement

`liquers-core` (`command_metadata.rs`, `commands.rs`, `plan.rs`, `interpreter.rs`, `recipes.rs`,
and a new `pipeline.rs` for the serde types): every consumer plans through core. No change to
`parse.rs` or to query encoding. No new dependency.

## Documentation Intent

- Reference: new `specs/reference/PIPELINES_AND_COMPOSITES.md` (pipeline shape, composites,
  planning, caching); extend `COMMAND_ALIASES.md` (relation to composites), `RECIPES_AND_PLANS.md`
  (recipe `pipeline`, mixed planning, `Step::UseValue`), `COMMAND_DECLARATION.md` §4.1
  (`definition`). `PROJECT_OVERVIEW.md` §Recipes gains `pipeline`; its query encoding is unchanged.
- Guide: extend `COMMAND_REGISTRATION_GUIDE.md` (registering a composite) and
  `COMMAND_DESIGN_GUIDE.md` §Aliases (alias, composite or Rust).
- Documents to update: the above, each with a History row and a `reviewed:` bump.

## Open Questions

Decided on 2026-11-01: no placeholders in query text (the pipeline is the only mechanism); the
pipeline is the structured form for composites and recipes, and the planner mixes it with queries
in any order (`Step::UseValue` for a value element); the web part is a separate follow-up.
Analysis: [`notation-and-integration.md`](./notation-and-integration.md).

2. **Overlap with `recipe-override-addressing`.** That design (not approved) lets recipe
   `arguments` reach an earlier action of a *query* by position (`1.values`: argument `values` of
   the second command) and fill a resource key (`@key`). A recipe that needs an earlier argument
   can now put that action in its pipeline instead. Recommended: drop that design's positional
   addressing (its AC-1/AC-2) and keep only `@key`. That design is yours to amend.
3. **Defining composites without Rust.** Recommended: AC-22 (`recipes.yaml` entries with `command:`).
4. **Argument declarations.** Recommended: every placeholder is a declared argument; only an
   omitted type, `multiple` or `gui_info` is taken from the slot (AC-3).
8. **Variadic rules.** Recommended: `notation-and-integration.md` §4, as in AC-19.
9. **Payload and injection.** Recommended: forward injected arguments and payload requirements
   (AC-20); refuse an `injected` composite argument; leave payload *values* as sources to the
   follow-up.
10. **Recipe order.** Recommended: query first, then pipeline, plus `{query: …}` elements (AC-15,
    AC-16).
13. **Positional `parameters` in an action element.** Recommended: allowed, but not mixed with
    `arguments` for the same slot (a written query converts to positional parameters when the
    registry does not know the command yet).
14. **Namespace inheritance.** Recommended: an element without `ns` uses the default namespaces,
    never an `ns` of the surrounding query (AC-7). Alternative: inherit the caller's last `ns`
    (shorter pipelines, but a composite's behaviour would depend on where it is called).

Questions 1, 5, 6, 7, 11 and 12 are settled or moved: 1 and 7 by the decisions above, 5 because
query text has no placeholders, and 6, 11 and 12 to the follow-up issue.

## Scope Changes

- **2026-11-01, maintainer review 1.** Added the pipeline notation, declarations, variadics,
  payload, recipe pipelines and web endpoints (`notation-and-integration.md`).
- **2026-11-01, maintainer decisions.** Query-text placeholders (`$name`, `{{name}}`) are dropped,
  and the pipeline is the only placeholder mechanism. The planner mixes queries and pipeline
  elements, and `Step::UseValue` makes a value the current state. The web API, endpoint recipes and
  payload sources move to `WEB-API-CANNOT-RUN-PIPELINES-OR-RECIPE-ENDPOINTS`. AC-9, AC-12–AC-14 are
  retired; AC-15–AC-22 are added. This design stays `L` (core only). Phase 2 is rewritten after
  this phase is approved.

## Design Dependencies

- `command-alias-contract` (complete) — origin; composites follow its validation and dependency rules.
- `recipe-override-addressing` — **overlaps** (question 2).
- `agent-command-proposal` — **overlaps**: it proposes alias commands; composites widen what it can propose.
- `plan-default-namespaces` (PR #236) — **requires** its lookup chain for AC-7.
- `WEB-API-CANNOT-RUN-PIPELINES-OR-RECIPE-ENDPOINTS` — **required-by**: the web follow-up builds on the pipeline types.

## References

- Issue: [`COMMAND-CANNOT-BE-DEFINED-AS-A-COMMAND-SEQUENCE`](../../issues/COMMAND-CANNOT-BE-DEFINED-AS-A-COMMAND-SEQUENCE.md) (gh#237)
- `specs/reference/COMMAND_ALIASES.md`, `specs/reference/RECIPES_AND_PLANS.md`

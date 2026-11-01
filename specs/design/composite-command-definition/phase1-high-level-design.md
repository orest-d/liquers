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
because overrides reach only the last action (the recipe's exact shape is decided in Phase 2):

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
  THEN each problem is an `Issue::CommandRegistry` in the registry's `IssueReport` (`CommandMetadata::check`); an error-severity issue refuses registration and nothing is registered; an argument that no placeholder uses is a warning
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
- **AC-11** Placeholders only in command definitions
  WHEN a pipeline that is not a command's definition (a recipe's pipeline, or one planned on its own) contains a placeholder
  THEN planning fails with an unresolved-placeholder error naming it; inside a definition, every placeholder is a declared argument, so it always has a value, a default or an `ArgumentMissing` error (AC-3)
- **AC-16** Queries and pipelines combine in one plan
  WHEN a query calls a composite and continues after it, or a pipeline contains `{query: …}` elements, or a recipe has a query and a pipeline
  THEN the planner processes query actions and pipeline elements into one plan, in order, each receiving the previous one's state
- **AC-17** Positional and named arguments
  WHEN an action element gives `parameters` (positional, the last ones filling a variadic) and/or `arguments` (by name, in any order, leaving some out)
  THEN the positional ones fill slots first and the named ones the rest; omitted ones take their defaults; an unknown or `injected` name, or a name for a slot already filled positionally, is a `ParameterError`
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

- **AC-23** A list for a single argument
  WHEN a list (`Value::Array`) is given to a non-variadic argument
  THEN an `any` argument receives the whole array, and a scalar-typed argument is a `ConversionError`; the pipeline is not text-form from that step on
- **AC-24** Pipelines express queries
  WHEN any query is converted to a pipeline (`Query::to_pipeline`, syntactic) and planned
  THEN the plan equals the query's plan, and converting the pipeline back gives the same query

Recipes (b):

- **AC-15** Recipe pipeline
  WHEN a recipe has a `pipeline`, in a form compatible with today's recipe syntax (exact shape: Phase 2)
  THEN it plans and runs as that pipeline, with literal values for any step's arguments, so a list, map or long text can reach an action other than the last; every existing recipe is unchanged
Retired:

- ~~**AC-22**~~ commands defined by `recipes.yaml` entries: recipes define assets, not commands (2026-11-01). Commands with pipelines are defined in the command metadata.
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
- Assets / recipes: `Recipe` gains `pipeline`, with literal values only (AC-15).
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

Decided on 2026-11-01:

- No placeholders in query text: the pipeline is the only mechanism.
- The pipeline is the structured form for composites and recipes, and the planner mixes it with
  queries (`Step::UseValue` for a value element). The web part is a separate follow-up.
- Every placeholder is a declared `ArgumentInfo` of the command, with the pipeline held in its
  `CommandDefinition`; problems are reported through `IssueReport`.
- Pipelines have nothing to do with injection or the payload: both are forwarded as for a direct
  call (AC-20).
- An element without `ns` uses the default namespaces, never the caller's `ns` (AC-7).
- `recipe-override-addressing`'s positional addressing (`1.values`) is dropped; pipelines replace
  it (`RECIPE-OVERRIDE-ONLY-LAST-ACTION` now belongs to this design).

Analysis: [`notation-and-integration.md`](./notation-and-integration.md) (shape, variadics,
payload) and [`pipeline-and-plan.md`](./pipeline-and-plan.md) (plans, recipes, lists, positional
arguments).

19. **Name.** Recommended: `CommandDefinition::Pipeline` ("pipeline alias") instead of
    *composite*, because this is an alias whose body is a pipeline.

Deferred to Phase 2 by the maintainer (2026-11-01):

- **Builder** (`pipeline-and-plan.md` §1, Q15 accepted in principle). Keep the existing
  `PlanBuilder` and add a pipeline builder (`PipelineBuilder`, `Pipeline::to_plan`). Phase 2 decides
  how the two share code and how a query and a pipeline combine in one plan.
- **Recipe syntax** (Q16). A recipe pipeline stays compatible with the existing recipe syntax; the
  exact shape and the filename rule are decided in Phase 2.
- **Variadic arguments** (Q8, Q17, Q18). A dedicated Phase 2 section covers variadics: lists for
  `multiple` and for single (`any`) arguments, splicing, empty lists, positional and named
  arguments, and several placeholders into one variadic.

Recipes do not define commands (Q3, decided). Whether recipes, or something like them, will define
web endpoints or commands is left to the follow-up issue.

## Scope Changes

- **2026-11-01, maintainer review 1.** Added the pipeline notation, declarations, variadics,
  payload, recipe pipelines and web endpoints (`notation-and-integration.md`).
- **2026-11-01, maintainer decisions.** Query-text placeholders (`$name`, `{{name}}`) are dropped,
  and the pipeline is the only placeholder mechanism. The planner mixes queries and pipeline
  elements, and `Step::UseValue` makes a value the current state. The web API, endpoint recipes and
  payload sources move to `WEB-API-CANNOT-RUN-PIPELINES-OR-RECIPE-ENDPOINTS`. AC-9, AC-12–AC-14 are
  retired; AC-15–AC-22 are added. This design stays `L` (core only). Phase 2 is rewritten after
  this phase is approved.
- **2026-11-01, maintainer review 3.** `RECIPE-OVERRIDE-ONLY-LAST-ACTION` moves here from
  `recipe-override-addressing`, whose positional addressing is dropped. Added AC-23 (a list for a
  single argument) and AC-24 (pipelines express queries); AC-6, AC-15 and AC-17 revised (an
  `IssueReport`, either query or pipeline, positional and named arguments). Questions 15–19 added.
- **2026-11-01, maintainer review 4.** Recipes define assets, not commands: AC-22 retired. Recipe
  pipelines carry literal values only, and a placeholder outside a command definition is a planning
  error (AC-11, AC-15 revised). The builder design, the recipe syntax and the variadic rules are
  decided in Phase 2.

## Design Dependencies

- `command-alias-contract` (complete) — origin; composites follow its validation and dependency rules.
- `recipe-override-addressing` — **overlaps**: its positional addressing is dropped in favour of pipelines (2026-11-01); it keeps the `@key` hole.
- `agent-command-proposal` — **overlaps**: it proposes alias commands; composites widen what it can propose.
- `plan-default-namespaces` (PR #236) — **requires** its lookup chain for AC-7.
- `WEB-API-CANNOT-RUN-PIPELINES-OR-RECIPE-ENDPOINTS` — **required-by**: the web follow-up builds on the pipeline types.

## References

- Issue: [`COMMAND-CANNOT-BE-DEFINED-AS-A-COMMAND-SEQUENCE`](../../issues/COMMAND-CANNOT-BE-DEFINED-AS-A-COMMAND-SEQUENCE.md) (gh#237)
- `specs/reference/COMMAND_ALIASES.md`, `specs/reference/RECIPES_AND_PLANS.md`

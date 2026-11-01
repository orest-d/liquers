# Phase 1: High-Level Design — Composite command definitions

## Purpose

Let a sequence of commands be defined, without Rust, as one command whose body is query text with
**placeholders**, and let the same placeholders serve recipes and the web API. It makes the query
the one addressable unit (VISION) also for reusable pipelines: a long or structured value is bound
once, in a definition or a recipe, and every caller writes a short, cacheable query.

## Terminology

- **Placeholder**: `$name` written as a whole action parameter (`head-$n`), anywhere in a query,
  in any action, also inside a link (`~X~-R/a/-/f-$x~E`). `$` is outside every token class of the
  grammar today (`ns-pl/is_in-region-$r` is a `ParseError` at HEAD, checked with
  `liquers-validate`), so no existing query changes meaning.
- **Start placeholder**: a `$name` written as the first element of a query (`$data/from_csv`). The
  query starts from the value bound to `name` instead of from nothing.
- **`$state`**: in a composite body, the composite's input state (`state` is the default name of
  its state argument; a renamed state argument renames the placeholder).
- **Binding**: the value (any JSON, or a link) given to a placeholder name.
- **Composite command**: `CommandDefinition::Composite` — a command with its own arguments and a
  **body** query; calling it runs the body on its input with the arguments bound to the body's
  placeholders. (Not to be confused with *composite variadic elements*, the tuple elements of
  `design/composite-variadic-arguments/`.)

## Problem Example

A pipeline every caller repeats today (validated with `liquers-validate`):

```
-R/data/sales.csv/-/ns-pl/from_csv/is_in-region-EU-US/sort-amount-f/head-5
```

An alias cannot name it: it has three commands, and `pl/is_in` fixes `column = region`, which is
not a leading argument of any single target. **Should**, as a composite in the registry:

```yaml
- namespace: sales
  name: top_in
  arguments:
  - { name: by, argument_type: string, default: !Value amount }
  - { name: n, argument_type: int, default: !Value 10 }
  - { name: regions, argument_type: string, multiple: true }
  definition: !Composite
    body: ns-pl/from_csv/is_in-region-$regions/sort-$by-f/head-$n
```

and `-R/data/sales.csv/-/ns-sales/top_in-amount-5-EU-US` returns the same frame, cached under that
query like any command's result.

The same body as a **recipe** works today only when the list goes to the last action. With
placeholders, `arguments: {regions: [EU, US], n: 5}` binds them wherever they stand. Over **HTTP**,
`POST /q/$csv/ns-pl/from_csv/head-$n` with body `{"csv": "<text>", "n": 5}` parses posted text, which
today must first be written to the store under a key.

## Scope and Acceptance Criteria

Registry (a):

- **AC-1** Composite runs its body
  WHEN a registered composite is called with positional parameters
  THEN the result equals the body with every placeholder replaced by the matching argument, applied to the composite's input
- **AC-2** Arguments in any order and position
  WHEN a composite's arguments are ordered differently from the body's placeholders, or one placeholder occurs twice
  THEN each occurrence receives its argument; a `multiple` argument fills a `multiple` slot element by element
- **AC-3** Defaults and missing values
  WHEN an argument is omitted
  THEN its declared default applies; with no default, the argument's ArgumentInfo default is inferred from the slot it fills; still none is `ArgumentMissing` naming the composite argument
- **AC-4** Input state as an argument
  WHEN the body uses `$state` as a parameter
  THEN that parameter receives the composite's input state
- **AC-5** Argument as the initial state
  WHEN the body starts with `$name/`
  THEN the body runs on the argument's value (a link's result for a link), the composite is a source command (`state_argument: None`), and its input is ignored
- **AC-6** Registration validation
  WHEN a composite is registered or deserialized and the body does not parse, a placeholder names no argument, or an argument is never used
  THEN registration fails with `ParameterError` (unused argument: a warning), and nothing is registered
- **AC-7** Hygiene and nesting
  WHEN a composite body names commands
  THEN they resolve in the composite's own realm and namespace, never the caller's `ns`; a body may call aliases and composites, and a cycle is a planning error naming the cycle
- **AC-8** Caching, dependencies and errors
  WHEN a composite's result is computed
  THEN it is cached under the calling query, the composite's metadata is a dependency (editing the body invalidates it), the body's volatility, payload requirement and expiration apply, and an error in the body names the composite and the step

Recipes (b):

- **AC-9** Placeholders anywhere in a recipe query
  WHEN a recipe query contains placeholders and `arguments` / `links` name them
  THEN each is bound, with any JSON value (list, map, long text), in any action; a name that is no placeholder keeps today's meaning (an argument of the last action)
- **AC-10** Text-form bindings keep the query identity
  WHEN every placeholder of a query is bound to a scalar or a list of scalars
  THEN the plan equals the plan of the query with the values written in place, including each step's prefix query, so prefixes are shared with direct queries
- **AC-11** Unbound placeholder
  WHEN a placeholder has no binding
  THEN it behaves as an empty parameter (the argument's default); for a start placeholder it is `ArgumentMissing`

Web API (c):

- **AC-12** Placeholders over HTTP and WebSocket
  WHEN a query on the Query API, `q/data`, `q/entry`, `q/submit` or WebSocket `subscribe` contains placeholders and the request names them (URL parameter, JSON body member, `_link_<name>`)
  THEN they are bound as in AC-9, with the conversion and error rules of `WEB_API_SPECIFICATION.md` §2.4
- **AC-13** Request value as the starting state
  WHEN a query starts with `$name/` and the request binds `name`
  THEN the query runs on that value
- **AC-14** Managed when the query has a text form
  WHEN every binding of a request is text-form (AC-10) and it carries no other override
  THEN it is served as the substituted query, as a managed asset where today's rules make it one; otherwise it is unmanaged, as overrides are today

Non-goals: placeholders in a resource key, a segment header, an action name or a filename (keys:
see `RECIPE-CANNOT-PARAMETERIZE-RESOURCE-KEY`); recipe-local composite definitions; a JSON literal
syntax inside query text; replacing `CommandDefinition::Alias`; Python and JavaScript registration
APIs (they get the metadata form for free, a decorator is later work).

## Core Interactions

- Query: one new parameter form and one new leading element in the parser and encoder.
- Commands: `CommandDefinition::Composite`, registration and validation in the metadata registry.
- Plans: expansion of composites and placeholder binding in `PlanBuilder`; one new step; the interpreter binds `$state`.
- Assets / recipes: `Recipe::to_plan` binds placeholders; identity and caching per AC-10, AC-14.
- Web/API: `liquers-axum` passes placeholder names through the existing override path.
- Consumers: `liquers-validate` and `specs/command_registry.yaml` carry composites; `liquers-mcp` serves a composite as a tool like any command.

## Crate Placement

`liquers-core` (`parse.rs`, `query.rs`, `command_metadata.rs`, `commands.rs`, `plan.rs`,
`interpreter.rs`, `recipes.rs`): every consumer plans through core, and core cannot depend on
`liquers-template`, whose slot machinery is therefore not reused. `liquers-axum` for the request
mapping. No new dependency.

## Documentation Intent

- Reference: new `specs/reference/COMPOSITE_COMMANDS.md` (definition, placeholders, planning,
  caching); extend `COMMAND_ALIASES.md` (relation to composites), `RECIPES_AND_PLANS.md` (recipe
  contract, planning), `WEB_API_SPECIFICATION.md` §2.4, `PROJECT_OVERVIEW.md` §Query Language
  (encoding changes, required by `CLAUDE.md`), `COMMAND_DECLARATION.md` §4.1 (`definition`).
- Guide: extend `COMMAND_REGISTRATION_GUIDE.md` (registering a composite) and
  `COMMAND_DESIGN_GUIDE.md` §Aliases (alias, composite or Rust); `WEB_API_GUIDE.md` (placeholders).
- Documents to update: the above, each with a History row and a `reviewed:` bump.

## Open Questions

1. **Placeholder syntax.** `$name` (recommended: free in the grammar, URL-safe, short) or the
   template language's `{{name}}` (one syntax across Liquers, but braces need percent-encoding in a
   URL and read as Mustache, which is evaluated differently).
2. **Overlap with `recipe-override-addressing`** (AC-1/AC-2 there: `1.values`, `is_in.values`).
   Placeholders name holes in the text instead of addressing positions. Recommended: placeholders
   replace ordinal and name addressing for recipes and HTTP, and that design keeps only the key hole
   (`@key`). Merging or superseding designs is the maintainer's decision.
3. **How composites are loaded without Rust.** Recommended: an `EnvironmentConfig` list of
   command-definition files (YAML, `Alias` and `Composite` only), read at environment build;
   `design/agent-command-proposal/`'s staging realm can then propose composites as well as aliases.
4. **Argument inference.** Recommended: an argument declared without a type takes `argument_type`,
   `multiple`, default and `gui_info` from the first slot it fills; two slots that disagree are a
   `ParameterError` unless the argument is declared.
5. **Inside a link, only text-form bindings?** Recommended: yes for this design (a link is a
   separate query with its own identity); a list or map inside a link is a `ConversionError`.
6. **A raw request body as the starting state** (`POST` of a CSV with `Content-Type: text/csv`).
   Recommended: later; this design binds a start placeholder from JSON or a link only.

## Design Dependencies

- `command-alias-contract` (complete) — origin; composites follow its validation and dependency rules.
- `axum-query-named-arguments` (complete) — origin of the HTTP override path that AC-12 extends.
- `recipe-override-addressing` — **overlaps** (question 2).
- `agent-command-proposal` — **overlaps**: proposes alias commands; composites widen what it can propose.
- `plan-default-namespaces` (PR #236) — **requires** its lookup chain for AC-7's namespace resolution.

## References

- Issue: [`COMMAND-CANNOT-BE-DEFINED-AS-A-COMMAND-SEQUENCE`](../../issues/COMMAND-CANNOT-BE-DEFINED-AS-A-COMMAND-SEQUENCE.md) (gh#237)
- `specs/reference/COMMAND_ALIASES.md`, `specs/guides/TEMPLATE_GUIDE.md` §3, `specs/reference/TEMPLATE_LANGUAGE.md` §2

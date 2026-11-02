# Phase 1: High-Level Design — Pipelines

## Purpose

Let a sequence of commands be defined without Rust as one command, using a **pipeline**: a
structured, directly serializable form of a query in which any argument of any step can be a
literal value, a placeholder, a link, a nested pipeline or the input state. Recipes use the same
pipeline to carry complex data that query text cannot express. This keeps the query the one
addressable unit (VISION) for reusable pipelines: a long or structured value is written once, in a
definition or a recipe, and every caller writes a short, cacheable query.

The pipeline is also the **structured form of queries and recipes** (merged 2026-11-02 with
`QUERY-BUILDER-TOOLING`). Every query and recipe converts to a pipeline, and a pipeline converts
back to a query whenever it is *representable*. A `PipelineEditor` edits pipelines; a thin
`QueryEditor` over it is the validating query builder that code assembling queries from data
needs, instead of concatenating text.

## Terminology

- **Pipeline**: an ordered list of **pipeline elements**, serialized as JSON, YAML or TOML. An
  element is an action (`{action, ns?, realm?, arguments}`), a resource read (`{resource, mode?}`),
  embedded query text (`{query}`), a value that becomes the current state (`{value}` or
  `{placeholder}`, planned as `Step::UseValue`), or a filename. Full shape:
  [`notation-and-integration.md`](./notation-and-integration.md) §2.
- **Source**: what an action argument is given, by name: `{value: …}` (any JSON), `{placeholder:
  name}`, `{link: "<query>"}`, `{pipeline: […]}` (a nested pipeline evaluated as a link), `{state:
  true}` (the input state of the enclosing pipeline command), or `{multiple: […]}` (elements of a variadic
  argument).
- **Placeholder**: a named hole in a pipeline. It exists **only in pipelines**; query text gains no
  placeholder syntax (decided 2026-11-01).
- **Pipeline command**: `CommandDefinition::Pipeline { pipeline }`, a command whose arguments are
  its declared placeholders and whose body is a pipeline. Calling it runs the body on its input with
  the arguments bound to the placeholders. (This is unrelated to the *composite variadic elements*
  of `design/composite-variadic-arguments/`.)
- **Representable**: a pipeline that converts to a query (its *text form*). The exact rules are
  in §Representability rules below; in short, every element and source has a query spelling: a
  scalar, a list of scalars for a variadic, a link, or a representable nested pipeline. A
  representable pipeline plans exactly as its query.
- **Canonical form**: what a conversion produces. Converting a query to a pipeline and back gives
  the query's canonical encoding, and converting again changes nothing (idempotence).
- **`PipelineEditor`** edits a pipeline with validated steps. **`QueryEditor`** is a thin wrapper
  that refuses any edit after which the pipeline would no longer be representable.

## Problem Example

A pipeline every caller repeats today (validated with `liquers-validate`):

```
-R/data/sales.csv/-/ns-pl/from_csv/is_in-region-EU-US/sort-amount-f/head-5
```

An alias cannot name this: it runs three commands, and `pl/is_in` fixes `column = region`, which
is not a leading argument of any single target. **Should**, as a pipeline command in the registry:

```yaml
- namespace: sales
  name: top_in
  arguments:
  - { name: by, argument_type: string, default: !Value amount }
  - { name: n, argument_type: int, default: !Value 10 }
  - { name: regions, argument_type: string, multiple: true }
  definition: !Pipeline
    pipeline:
    - { action: from_csv, ns: pl }
    - { action: is_in, ns: pl, arguments: { column: { value: region }, values: { placeholder: regions } } }
    - { action: sort, ns: pl, arguments: { column: { placeholder: by }, ascending: { value: false } } }
    - { action: head, ns: pl, arguments: { n: { placeholder: n } } }
```

With it, `-R/data/sales.csv/-/ns-sales/top_in-amount-5-EU-US/ns-pl/tail-2` mixes the two forms:
a query, the pipeline command's pipeline, then the query again. It returns the same rows as the long
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

Pipeline commands in the registry (a):

- **AC-1** A pipeline command runs its pipeline
  WHEN a registered pipeline command is called from a query with positional parameters
  THEN the result equals running its pipeline on the pipeline command's input, with each placeholder replaced by the matching argument
- **AC-2** Arguments in any order and position
  WHEN a pipeline command's arguments are ordered differently from the steps they fill, or one placeholder is used twice
  THEN each use receives its argument
- **AC-3** Declared placeholders and defaults
  WHEN an argument is omitted
  THEN its declared default applies; with no default it is `ArgumentMissing` naming the pipeline command argument; an omitted `argument_type`, `multiple` or `gui_info` is taken from the slot the placeholder fills
- **AC-4** Input state as an argument
  WHEN a step argument's source is `{state: true}`
  THEN that argument receives the pipeline command's input state
- **AC-5** A value as the current state
  WHEN a pipeline element is `{value: …}` or `{placeholder: name}`
  THEN the following step receives that value as its state (`Step::UseValue`); a pipeline command whose first element is one of these is a source command (`state_argument: None`)
- **AC-6** Registration validation
  WHEN a pipeline command is registered or deserialized, and a placeholder is not a declared argument, an argument is `injected`, or an argument name is not an argument of its step's command
  THEN each problem is an `Issue::CommandRegistry` in the registry's `IssueReport` (`CommandMetadata::check`); an error-severity issue refuses registration and nothing is registered; an argument that no placeholder uses is a warning
- **AC-7** Hygiene and nesting
  WHEN a pipeline command's steps name commands
  THEN they resolve through each element's own `ns` and `realm`, else the default namespaces, never the caller's `ns`; a pipeline may call aliases and pipeline commands, and a cycle is a planning error naming the cycle
- **AC-8** Caching, dependencies and errors
  WHEN a pipeline command's result is computed
  THEN it is cached under the calling query; the pipeline command's metadata is a dependency, so editing its pipeline invalidates the result; the steps' volatility, payload requirement and expiration apply; and an error in a step names the pipeline command and the step

The pipeline form, everywhere:

- **AC-10** Text-form identity
  WHEN a pipeline is text-form
  THEN its plan, including each step's prefix query, equals the plan of its equivalent query, so its prefixes are shared with direct queries
- **AC-11** Placeholders only in command definitions
  WHEN a pipeline that is not a command's definition (a recipe's pipeline, or one planned on its own) contains a placeholder
  THEN planning fails with an unresolved-placeholder error naming it; inside a definition, every placeholder is a declared argument, so it always has a value, a default or an `ArgumentMissing` error (AC-3)
- **AC-16** Queries and pipelines combine in one plan
  WHEN a query calls a pipeline command and continues after it, or a pipeline contains `{query: …}` elements, or a recipe has a query and a pipeline
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
  THEN the arguments are injected from the caller's context and the pipeline command is `payload: required`, exactly as for a direct call
- **AC-21** Round trip and validation
  WHEN a pipeline command or a pipeline is serialized (registry export, YAML, JSON) and read back, or given to `liquers-validate`
  THEN it is unchanged, and the validator plans it, reporting its text form when it has one

- **AC-23** A list for a single argument
  WHEN a list (`Value::Array`) is given to a non-variadic argument
  THEN an `any` argument receives the whole array, and a scalar-typed argument is a `ConversionError`; the pipeline is not text-form from that step on
- **AC-24** Pipelines express queries
  WHEN any query is converted to a pipeline (`Query::to_pipeline`, syntactic) and planned
  THEN the plan equals the query's plan, and converting the pipeline back gives the same query

Conversion, representability and editing (merged scope, 2026-11-02):

- **AC-25** Every query is a pipeline
  WHEN any query that `parse_query` accepts is opened in an editor (`PipelineEditor::from_query(&query, cmr)`) or converted syntactically (`Pipeline::try_from(&query)`)
  THEN the conversion succeeds; with the registry every scalar parameter takes its argument's declared type (Q21), without it every parameter stays text. It fails only for a hand-built `Query` that `check_encodable` refuses (AC-33)
- **AC-26** Query round trip
  WHEN a query is converted to a pipeline and back
  THEN the result is the query's canonical form, which plans exactly as the query; for a query already in canonical form it is the same query, and its encoding is equal
- **AC-27** Idempotence
  WHEN a pipeline is converted to a query and back through the registry, then to a query and back again
  THEN the second round changes nothing, as pipeline and as query; a typed scalar keeps its type and its exact value (an `f64` is written in its shortest round-trip form)
- **AC-28** A pipeline from a query is representable
  WHEN a pipeline originates from a query and has been edited only through `QueryEditor`
  THEN it converts to a query: `Query::from(&query_editor)` is infallible
- **AC-29** Non-representability is reported exactly
  WHEN a pipeline that is not representable is converted to a query
  THEN the error names the first offending element and argument and the rule it breaks (§Representability rules, by rule id), and nothing else is converted
- **AC-30** Every recipe is a pipeline
  WHEN a recipe is converted (`Recipe::to_pipeline`)
  THEN its query, its `pipeline`, its `arguments` and `links` (as named arguments of the last action), its `cwd` and its `volatile` become one pipeline that plans as `Recipe::to_plan` does; its asset declarations (`title`, `description`, `expires`, `stored`, `cached`) stay on the recipe
- **AC-31** Editing a pipeline
  WHEN a `PipelineEditor` appends, inserts, replaces, moves or removes an element, or sets or removes an argument, namespace or realm of an action
  THEN the edit is applied, or it is refused with a `ParameterError` naming the value and its position kind (for example a key segment `my file.csv`), and the pipeline is unchanged
- **AC-32** The query editor keeps a query
  WHEN a `QueryEditor` is asked for an edit that would make its pipeline non-representable (a list for a single argument, a placeholder, a value as state)
  THEN it refuses the edit with the representability reason and stays unchanged; `QueryEditor::query()` and `Query::from(&query_editor)` always succeed
- **AC-33** Hand-built queries can be checked
  WHEN `Query::check_encodable` is called on a programmatically assembled query
  THEN it is `Ok` exactly when `parse_query(&q.encode())` reproduces `q`, and otherwise names the first offending token
- **AC-34** One definition of each token class
  WHEN the parser, the editors, `check_encodable` or `liquers-template` validate an action name, namespace, key segment, header value or filename
  THEN they call the same `liquers_core::escape` predicates
- **AC-35** The validation tool handles pipelines
  WHEN `liquers-validate` is given a pipeline (JSON or YAML), a query, a recipe file, or a query spelled as separate shell arguments (the former `liquers-encode`, Q24)
  THEN it validates it against the registry and, on request, prints the conversion: query → pipeline, pipeline → query, recipe → pipeline, shell arguments → query. Exit codes stay 0 / 1 / 2, and a not-representable pipeline is a 1 naming the rule
- **AC-36** An empty parameter is encodable
  WHEN `encodeParam("")` is called in `liquers-web`, or an editor is given an empty parameter
  THEN the result is the empty token, which parses back as one empty parameter
- **AC-37** The editors expose the registry
  WHEN code holds a `PipelineEditor` or `QueryEditor` (each holds an `Arc<CommandMetadataRegistry>`)
  THEN it can ask for an action's resolved `CommandMetadata` and `CommandKey`, an argument's `ArgumentInfo` by name or position, the arguments still unset, and the namespaces an action resolves in; an edit naming an unknown command or argument is refused
- **AC-38** A host binds placeholders
  WHEN code outside a command definition holds a pipeline with placeholders and a map of values (`Pipeline::bind`)
  THEN each placeholder is replaced by its value, an unbound placeholder is an error naming it, and the result plans as any pipeline. This is the hook the records-manifest follow-up uses (`MANIFEST-TEMPLATE-BATCH-ONLY-AT-THE-END-OF-THE-QUERY`); a recipe pipeline still takes literal values only (AC-11)

Recipes (b):

- **AC-15** Recipe pipeline
  WHEN a recipe has a `pipeline`, in a form compatible with today's recipe syntax (exact shape: Phase 2)
  THEN it plans and runs as that pipeline, with literal values for any step's arguments, so a list, map or long text can reach an action other than the last; every existing recipe is unchanged
Retired:

- ~~**AC-22**~~ commands defined by `recipes.yaml` entries: recipes define assets, not commands (2026-11-01). Commands with pipelines are defined in the command metadata.
- ~~**AC-9**~~ placeholders in recipe query text: query text has no placeholders (2026-11-01), so AC-15 replaces it.
- ~~**AC-12**, **AC-13**, **AC-14**~~ web API binding, request values as the start state, managed text forms: moved to [`WEB-API-CANNOT-RUN-PIPELINES-OR-RECIPE-ENDPOINTS`](../../issues/WEB-API-CANNOT-RUN-PIPELINES-OR-RECIPE-ENDPOINTS.md) (scope split, 2026-11-01).

Non-goals: Python and JavaScript wrappers for the editors (`QUERY-BUILDER-NOT-IN-BINDINGS`,
`design/query-builder-in-bindings/`, which moves from `QueryBuilder` to `QueryEditor`); placeholder
syntax in query text (no change to query encoding); web API routes,
endpoint recipes, MCP serving and payload sources (the follow-up issue above); an `injected`
pipeline command argument; replacing `CommandDefinition::Alias`; Python and JavaScript registration
helpers (both read the metadata form as it is).

**Web API in this design.** No route changes. HTTP overrides keep working against a pipeline command: a
pipeline command's arguments are its own `ArgumentInfo`s, so `?n=5` on `…/ns-sales/top_in` sets `n` as for
any command.

## Representability rules

A pipeline is **representable** when every rule below holds. `Query::try_from(&pipeline)` checks
them in element order, and the first rule that fails is the error (AC-29): `NotRepresentable` with
the element index, the argument (by position or name) and the rule id. Token classes are those of
DOC_02 §Lexical forms, tested by the shared `escape` predicates (AC-34). String parameters are
never a reason: `encode_token` escapes any string.

**Whole pipeline**

| Id | Rule |
|---|---|
| P1 | No `value` or `placeholder` element: a value as the current state has no query spelling. |
| P2 | A `filename` element is last (also a validity rule for every pipeline). |
| P3 | `rooted` (a leading `/`) and an explicit resource-segment header name are carried as they are. |

**Elements**

| Id | Element | Query spelling | Representable when |
|---|---|---|---|
| E1 | `action` | `name-p1-p2` | `action` is an identifier and not `ns`, `q` or `v`; every source obeys S1–S7 |
| E2 | `action` with `ns` | `ns-a-b/name-…`, placed by the ns placement rule below | each namespace is an identifier |
| E3 | `action` with `realm` | a new segment `-realm/name-…` | `realm` is a realm name |
| E4 | `resource` | `-R/a/b` or `-R-mode/a/b` | every key segment is a resource name; `mode` is a header value |
| E5 | `cwd`, `key` | `-R-cwd/…`, `-R-key/…` | as E4 |
| E6 | `query` | its text, spliced at a segment boundary | it parses and obeys these rules after splicing |
| E7 | `quote`, `volatile` | `q`, `v` | always |
| E8 | `filename` | the last token | it is a filename |

**Sources of an action argument**

| Id | Source | Query spelling | Representable when |
|---|---|---|---|
| S1 | scalar `value` (string, integer, float, bool) | its text; an `f64` in shortest round-trip form | always. Read back with the registry it takes the argument's declared type, so it keeps its type; for an argument of type `any` or `text`, a non-string scalar comes back as text (the one canonical-form change) |
| S2 | `value` that is an array of scalars | consecutive parameters | the argument is the command's `multiple` argument and the last one written |
| S3 | `multiple` of sources | consecutive parameters | as S2, and each element obeys S1, S5 or S6 |
| S4 | `value` that is null, an object, or an array for a single argument | — | never |
| S5 | `link` | `~X~…~E` | the text parses and is representable |
| S6 | `pipeline` (nested) | `~X~…~E` | the nested pipeline is representable |
| S7 | `placeholder`, `state` | — | never (they exist only inside a command definition, AC-11) |

**Named arguments**

| Id | Rule |
|---|---|
| N1 | A query has positional parameters only, so converting a pipeline to a query always needs the registry (`pipeline.to_query_with(&cmr)`, or an editor, which holds it). |
| N2 | Named arguments are placed by the command's parameter order; a gap before one is filled with that parameter's default, as text. |
| N3 | A gap at a required parameter (no default), or at an `injected` one, is not representable. |
| N4 | An argument given both by position and by name is invalid for any pipeline, not only for a query. |

**Namespace placement** (Q21). A pipeline action names its namespaces, or none for the default
namespaces; a query's `ns` lasts to the end of its segment, with the lookup chain of
`plan-default-namespaces`. The target is that every action resolves to the same `CommandKey` as
its pipeline element. Within each transform segment, the conversion tries in order and keeps the
first that resolves every action correctly:

1. no `ns` at all;
2. one `ns` at the start of the segment, listing every namespace the segment's actions name, unique,
   in order of first use;
3. from left to right, an `ns` before each action that does not yet resolve correctly, re-checking
   the actions after it each time.

Step 3 always succeeds, since an `ns` naming exactly an action's namespaces resolves it, so
namespaces are never a reason for non-representability. A pipeline action that names no namespace
but follows an `ns` gets an `ns` restating the defaults' first match only if it would otherwise
resolve wrongly.

**Guarantees.** `Pipeline::try_from(&query)` produces only representable elements (rooted flag,
resources, header instructions, `ns`/`q`/`v`, actions with positional scalar and link sources,
filename), so AC-28 holds by construction and `QueryEditor` preserves it by checking each edit
against these rules. The canonical form (Q21) normalizes only what has one spelling in a query:
N2's names, the namespace placement above, S1's scalars for an `any` or `text` argument, empty
segments and source positions.

## Core Interactions

- Pipeline (new module `liquers_core::pipeline`): the types and their serde form; fallible
  conversion to and from `Query` (`TryFrom` both ways) and from `Recipe`; the representability
  check; `PipelineEditor` and the `QueryEditor` wrapper over it; the pipeline planner.
- Commands: `CommandDefinition::Pipeline`, registration, validation and export in the metadata
  registry.
- Plans: the existing `PlanBuilder` is kept for queries; the pipeline planner takes queries and
  pipeline elements in one plan and expands pipeline commands; new `Step::UseValue`; the
  interpreter supplies `{state: true}`.
- Assets / recipes: `Recipe` gains `pipeline`, with literal values only (AC-15), and
  `Recipe::to_pipeline` (AC-30).
- Query: `Query::check_encodable`; the token predicates in `liquers_core::escape`, used by
  `parse.rs`, the representability check and the editors (AC-33, AC-34).
- Validate: `liquers-validate` accepts pipelines, recipes and shell-argument queries and prints conversions (AC-35);
  `specs/command_registry.yaml` carries pipeline commands.
- Template: `liquers-template/src/query/instantiate.rs` validators delegate to the core predicates.
- Web: `liquers-web/src/encode.rs` `encode_param` accepts the empty string (AC-36). No route
  changes (the follow-up issue).
- Consumers: `liquers-mcp` serves a pipeline command as a tool like any command.

## Crate Placement

`liquers-core`, because every consumer plans through core and the editor must be usable from
every integration. The pipeline gets its own module, `liquers-core/src/pipeline/`:

| File | Holds |
|---|---|
| `mod.rs` | `Pipeline`, `PipelineElement`, `ActionElement`, `Source`; serde |
| `convert.rs` | query ↔ pipeline, recipe → pipeline, representability, canonical form |
| `editor.rs` | `PipelineEditor`, `ActionEditor`, `QueryEditor` |
| `plan.rs` | the pipeline planner (`Pipeline::to_plan`), sharing assembly code with `PlanBuilder` |

Also touched: `command_metadata.rs`, `commands.rs`, `plan.rs`, `interpreter.rs`, `recipes.rs`,
`escape.rs`, `parse.rs` (the shared predicates only; the grammar and the encoding are unchanged),
and `liquers-validate` (existing `cli` feature). No new dependency.

## Documentation Intent

- **New reference `specs/reference/PIPELINES.md`**: the pipeline model and serde form, sources,
  pipeline commands, recipe pipelines, planning and caching, the representability rules, the
  canonical form and the round trip, the editors.
- **New guide `specs/guides/PIPELINE_GUIDE.md`**: building and editing a pipeline in Rust, turning a
  query into a pipeline and back, writing a pipeline command, a recipe pipeline, what to do with a
  not-representable error.
- **Pipelines referenced wherever they apply**, each with a History row and a `reviewed:` bump:
  - query: `PROJECT_OVERVIEW.md` §Query Language (a query is a pipeline; encoding unchanged) and
    Technical Debt (the raw-emission caveat), `api/DOC_02_QUERY_LANGUAGE_REFERENCE.md`
    §Programmatic construction (`QueryEditor`, `check_encodable`), `QUERY_ESCAPING_GUIDE.md`
    (the editor first, then `liquers-validate`'s shell-argument form);
  - command metadata and registration: `COMMAND_ALIASES.md` (relation to pipeline commands),
    `COMMAND_DECLARATION.md` §4.1 (`definition`), `COMMAND_REGISTRATION_GUIDE.md` (a section on
    the pipeline alias: when to use it, how to declare its placeholders, how to register it, with
    a link to the reference), `COMMAND_DESIGN_GUIDE.md` §Aliases (alias, pipeline command or Rust);
  - plan and recipes: `RECIPES_AND_PLANS.md` (recipe `pipeline`, mixed planning, `Step::UseValue`)
    and `PROJECT_OVERVIEW.md` §Recipes;
  - templates: `TEMPLATE_LANGUAGE.md` (the shared predicates);
  - tooling: `CLAUDE.md` §Validating queries and the `liquers-validate` skill (pipelines, conversions);
  - web API: later, in `WEB-API-CANNOT-RUN-PIPELINES-OR-RECIPE-ENDPOINTS`.

## Open Questions

Decided on 2026-11-01:

- No placeholders in query text: the pipeline is the only mechanism.
- The pipeline is the structured form for pipeline commands and recipes, and the planner mixes it with
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

- The variant is `CommandDefinition::Pipeline` (a "pipeline command", an alias whose body is a
  pipeline), not *composite* (question 19, decided 2026-11-01).

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

Opened by the scope change of 2026-11-02 and reviewed by the maintainer the same day:

- **Q20 — the registry in conversions (decided).** A pipeline step may name its arguments, and a
  query needs them in the command's order, which only the command metadata registry knows. So
  converting a pipeline to a query always takes the registry: `pipeline.to_query_with(&cmr)`,
  `Query::try_from(&pipeline_editor)` and the infallible `Query::from(&query_editor)`. Both editors
  hold an `Arc<CommandMetadataRegistry>`, used for validation, argument order and namespace
  placement (AC-37). The registry is owned by value in the command executor today, so Phase 2
  decides between an `Arc` there and a snapshot taken when an editor is created.
- **Q21 — the canonical form (decided).** Typed scalars keep their type and exact value: the query
  side writes the text, and reading back through the registry restores the declared type (S1).
  Namespaces follow the three-step placement in §Representability rules: none if the defaults
  resolve everything; else one `ns` per segment with the unique namespaces in order of use; else
  an `ns` before each action that still resolves wrongly, left to right.
- **Q22 — editor style (decided):** `&mut self` methods returning `Result<&mut Self, Error>`.
- **Q23 — naming (decided):** only `QueryEditor`; `QueryEditor::new(cmr)` builds from scratch.
- **Q24 — `liquers-encode` (answered, recommendation changed).** It came from the superseded
  `query-builder` design: a small CLI that assembles a query from separate shell arguments
  (`liquers-encode -R data "my file.csv" -- filter "a b"`) and escapes each one correctly, so a
  script never hand-escapes a query. Recommended now: no separate binary. `liquers-validate` gains
  pipeline support (the maintainer's "validation tool enhanced for pipelines"), and the shell
  argument form becomes one of its inputs (AC-35).
- **Q25 — how an argument value is held (decided: B, `serde_json::Value`).** The maintainer chose
  it on 2026-11-02. The core `Value` would tie the pipeline to one value type, while an
  environment's `Value` only has to implement `ValueInterface`. A pipeline that is plain JSON, as a
  plan is, is what the validation tool and later tools need. Consequences for Phase 2:
  - Floats are exact when `serde_json`'s `float_roundtrip` feature is on. `arbitrary_precision` is
    not used, because Cargo feature unification would change `serde_json::Number` for every crate
    in the build.
  - NaN and ±infinity have no JSON number, so they use the existing non-finite token convention
    (`liquers-core/src/value.rs`, `to_json` / `from_json`).
  - The planner converts a JSON argument to the plan's parameter value, a cheap one-time step. A
    `value` element becomes the state through the environment's `ValueInterface`.
  - Rejected: A, the core `Value` (ties the pipeline to it, and its non-JSON variants would need
    their own serde rules); C, text only (loses the type).

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
- **2026-11-01, approval.** Question 19 decided: `CommandDefinition::Pipeline`; this document says
  *pipeline command* where it said *composite*. Phase 1 approved.
- **2026-11-01, maintainer review 4.** Recipes define assets, not commands: AC-22 retired. Recipe
  pipelines carry literal values only, and a placeholder outside a command definition is a planning
  error (AC-11, AC-15 revised). The builder design, the recipe syntax and the variadic rules are
  decided in Phase 2.
- **2026-11-02, scope extension.** The maintainer merged `QUERY-BUILDER-TOOLING` and the
  `query-builder` design (now superseded) into this one: the pipeline has fallible conversion to
  and from a query, lives in its own module, and has a `PipelineEditor` with a `QueryEditor`
  wrapper that refuses non-representable edits. Every query and recipe is a pipeline, the round
  trip is idempotent, and every pipeline from a query converts back. A pipeline reference and a
  pipeline guide are added, and the related documents refer to them. Added AC-25–AC-36 and
  questions 20–25. Phase 1 is reopened for approval, and the design grows from `L` to `XL`.
- **2026-11-02, maintainer review 5.** The registry is required to convert a pipeline to a query,
  and the editors hold it (Q20, AC-37). Typed scalars keep their type, and there is a three-step
  namespace placement (Q21). Q22 and Q23 accepted. `liquers-encode` is folded into
  `liquers-validate`, which gains pipeline support (Q24, AC-35). Q25 decided later the same day: `serde_json::Value` (option B). AC-38 (host binding of placeholders) is added for the records-manifest follow-up. Two
  follow-up issues were filed, to be designed right after this one:
  `MANIFEST-TEMPLATE-BATCH-ONLY-AT-THE-END-OF-THE-QUERY` and
  `RECIPES-CANNOT-DESCRIBE-A-SELF-CONTAINED-ASSET-TREE`.

## Design Dependencies

- `command-alias-contract` (complete) — origin; pipeline commands follow its validation and dependency rules.
- `recipe-override-addressing` — **overlaps**: its positional addressing is dropped in favour of pipelines (2026-11-01); it keeps the `@key` hole.
- `agent-command-proposal` — **overlaps**: it proposes alias commands; pipeline commands widen what it can propose.
- `plan-default-namespaces` (PR #236) — **requires** its lookup chain for AC-7.
- `WEB-API-CANNOT-RUN-PIPELINES-OR-RECIPE-ENDPOINTS` — **required-by**: the web follow-up builds on the pipeline types.
- `MANIFEST-TEMPLATE-BATCH-ONLY-AT-THE-END-OF-THE-QUERY` — **required-by**: manifest templates as pipelines, through `Pipeline::bind` (AC-38); designed right after this one.
- `RECIPES-CANNOT-DESCRIBE-A-SELF-CONTAINED-ASSET-TREE` — **required-by**: recipe trees reference nodes through recipe pipelines and edit them with a `TreeEditor` on `PipelineEditor`; designed right after this one.
- `query-builder` — **superseded by this design** (2026-11-02); its reasoning and Phase 1 decisions are adopted.
- `query-builder-in-bindings` — **required-by**: exposing the builder in Python and JavaScript now targets `QueryEditor`.
- `parameter-entity-escaping` (complete) — **origin** of the query-builder part: it made `encode_token` total.
- `resource-name-unicode` — **overlaps**: what a valid key segment is; both call `escape::is_resource_name`, so whichever lands second needs no change.
- `query-token-classes-ascii` (complete) — **origin**: it made the token classes ASCII, but its character predicates are private to `parse.rs`; this design makes them public token predicates in `escape.rs` (AC-34).

## References

- Issue: [`COMMAND-CANNOT-BE-DEFINED-AS-A-COMMAND-SEQUENCE`](../../issues/COMMAND-CANNOT-BE-DEFINED-AS-A-COMMAND-SEQUENCE.md) (gh#237)
- Issue: [`QUERY-BUILDER-TOOLING`](../../issues/QUERY-BUILDER-TOOLING.md), merged 2026-11-02; superseded design [`query-builder`](../query-builder/DESIGN.md)
- `specs/reference/COMMAND_ALIASES.md`, `specs/reference/RECIPES_AND_PLANS.md`, `specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md`

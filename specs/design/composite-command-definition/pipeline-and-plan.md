# Pipelines, plans and recipes — review analysis of 2026-11-01

This answers the maintainer's third review round on PR #238: how a pipeline relates to a `Plan`
and whether the two should be unified, whether a recipe's pipeline runs before or after its query
(and where the filename goes), what giving a list to a single argument means, and whether pipelines
need positional arguments. It builds on [`notation-and-integration.md`](./notation-and-integration.md)
§2 (the pipeline shape). Nothing here is decided until the maintainer chooses. The questions are in
Phase 1.

## 1. Pipeline and plan: the same list at two stages

A `Plan` (`liquers-core/src/plan.rs`) is the **output** of planning: an ordered `Vec<Step>`, plus
facts the planner derived (`is_volatile`, `payload_required`, `expires`, `dependencies`,
`predecessor`, `frozen_cwd`, `init_steps` diagnostics). A pipeline is an ordered list too. Element
by element, the two correspond:

| Query text | Pipeline element (proposed) | `Step` after planning |
|---|---|---|
| `-R/key` | `{resource: key}` | `GetAsset(Key)` |
| `-R-meta/key`, `-R-stored/key`, `-R-dir/key`, … | `{resource: key, mode: meta}` (every header instruction) | `GetAssetMetadata`, `GetResource`, `GetAssetDirectory`, … |
| `-R-cwd/key` | `{cwd: key}` | `SetCwd(Key)` |
| `-R-key/key` | `{key: key}` | `UseKeyValue(Key)` |
| `cmd-a-b` | `{action: cmd, ns?, realm?, parameters: […], arguments: {…}}` | `Action { realm, ns, action_name, parameters: ResolvedParameterValues, origin, query }` |
| `~X~q~E` (as a parameter) | `{link: q}` / `{pipeline: […]}` | `ParameterLink` / new `PlanLink` |
| *(none)* | `{value: …}` / `{placeholder: p}` | new `UseValue(Value)` |
| `…/q` | `{quote: true}` (quotes what precedes it) | `UseQueryValue(Query)` |
| `…/v` | `{volatile: true}` | none: sets the plan's volatility from that position |
| `ns-a-b` | per-element `ns` (no separate element) | none |
| `-realm/…` (segment header) | per-element `realm` | none |
| `name.ext` (last) | `{filename: name.ext}` (last) | `Filename` |
| *(an alias or composite call)* | `{action: …}` | `Action { origin: Alias }` / new `Composite { plan }` |
| text fragment | `{query: "…"}` | whatever the fragment plans to |

So **a pipeline is an unresolved plan**, and a query is a pipeline's text form. What planning adds
is resolution: names become command keys through the namespace chain, aliases and composites are
expanded, named arguments become positional `ResolvedParameterValues` with defaults, conversions and
enum mappings applied, and volatility, payload, expiration, dependencies and prefix queries are
derived. With the elements above, the pipeline can express everything a query can, plus what query
text cannot: structured values, values as state, and nested pipelines. Recipe-level declarations
(`title`, `description`, `cwd`, `volatile`, `expires`, `stored`, `cached`) stay in `Recipe` around
the pipeline, because they describe the asset rather than a step.

### Should they be unified?

| Option | What it means | For | Against |
|---|---|---|---|
| **U1: pipeline as the planner's input language** (recommended) | Query → `Pipeline` → `Plan`. `PlanBuilder` plans pipeline elements; a query is first converted to elements (`Query::to_pipeline`, purely syntactic: positional parameters, no registry). | One planning path: a query and its pipeline plan identically by construction (AC-10 holds by design, not by testing). Mixed plans (query, composite, query) are just a longer element list. The `Step` ↔ element table becomes a maintained contract. | Refactors `PlanBuilder::process_query` / `process_action` to take elements; the prefix-query bookkeeping (`Step::Action::query`, `Plan::predecessor`) must come from each element's text form. |
| U2: plans as the authoring format | Write `Plan`s by hand, with placeholders in them | No new type | A plan holds resolution results (positions, `ResolvedParameterValues`, origins, frozen operands) that an author cannot write. Its serialized form is explicitly an inspection format with no compatibility guarantee (`RECIPES_AND_PLANS.md` §Serialization), and it would have to become a stable one. |
| U3: two independent paths | The pipeline planned beside queries, with no shared code | Least refactoring | Two planners to keep equal; every query feature must be implemented twice; drift is certain. |

**Recommendation: U1.** The pipeline is the **stable, versioned authoring format**: it is what
recipes, registry entries and the follow-up web API store and send. The plan stays the unstable
inspection format it is now. Two consequences follow. A `Step` variant that an author can mean
(everything but `Info`, `Warning`, `Error` and the internal `Plan`) gets a pipeline element in the
same change that adds the step. And `liquers-validate` can print a query as a pipeline and back,
which tests the conversion on every query in the corpus.

## 2. A recipe's pipeline: first, last or both

| Option | For | Against |
|---|---|---|
| Query, then pipeline (Phase 1 at the last revision) | The query prefix is readable and cached as a shared boundary | **Filename:** the query must not end with one, so the name moves to the pipeline's last element or to the recipe, which is unintuitive. Cannot give the query a structured starting state |
| Pipeline, then query | The pipeline sets the starting state (a value, or a source command with an SQL text), and the query transforms it | A query that begins with a resource segment silently discards the pipeline's result. The prefix caches only if the pipeline is text-form |
| Both (an initialization pipeline, the query, a trailing pipeline) | Covers both cases | Three fields and two ordering rules; the filename problem remains |
| **One pipeline, with `{query: …}` elements** (recommended) | Covers all three orders: `[ {value…}, {query: "ns-pl/from_csv"}, {action…}, {filename…} ]`. The filename is the last element, exactly as it is the last element of a query. No ordering rule at all | `recipes.yaml` gains a second way to write a recipe |

**Recommendation:** a recipe has **either** `query` **or** `pipeline`; `query: q` means the same as
`pipeline: [ {query: q} ]`. The filename is the pipeline's last element, or ends its last `{query}`
fragment. As in a query, a filename anywhere else is an error. `Recipe::key`, `filename`,
`data_format` and `store_to_key` read the pipeline's text form, or its last element when there is
none. `arguments` and `links` bind the pipeline's placeholders. A name that is not a placeholder
keeps today's meaning, an override of the last action, so every existing recipe is unchanged.

## 3. A list for a single argument (`Value::Array`)

An action argument given `{value: [a, b]}`, or a placeholder bound to a list, means one of two
things depending on the argument:

| Argument | Meaning |
|---|---|
| `multiple` | The list is the **elements** (spliced among written ones, `notation-and-integration.md` §4) |
| `any` (or a type that accepts arrays, such as a future `json`) | One value, the whole `Value::Array` |
| a scalar type (`string`, `int`, `float`, `bool`, enum) | `ConversionError`, as `convert_override` refuses a list today (`recipes.rs`): no silent JSON text |

Issues to settle:

1. **Ambiguity for `multiple`.** `{value: [[a, b], [c]]}` means two elements that are lists. One
   element that is itself a list is written `{multiple: [ {value: [a, b]} ]}`. A list element is
   accepted only where the element type allows it (`any`, or the tuple elements of
   `design/composite-variadic-arguments/`).
2. **Empty list.** For `multiple`: no elements (an empty variadic). For `any`: the value `[]`, which
   is **not** the default. These two must not be conflated, as `VARIADIC-EMPTY-ELEMENT-CONVERSION`
   already shows for variadics.
3. **No text form.** A list in a non-`multiple` slot cannot be written in query text. From that step
   on the pipeline is not text-form, so later intermediates are not shared as query prefixes. That
   costs some sharing, not correctness: a composite's result is still cached under its calling query.
4. **Identity and size.** A value inside a composite definition is part of its metadata, so it is
   in `metadata_version` (editing it invalidates results, correctly), and a large array makes the
   registry larger. Large data belongs in a link (`{link: "-R/data/regions.json"}`), not in the
   definition. The guide should say so.
5. **Links inside a list.** `{value: […]}` is literal JSON, so it can hold no link or placeholder.
   Building an array from sources (`{array: [ {placeholder: a}, {link: …} ]}`, likewise `{object:
   {…}}`) needs each source materialized first, and it meets the problem of
   `VARIADIC-KEY-OR-QUERY-LINK-FAILS-TO-MATERIALIZE`. Recommended: defer the constructors; a
   placeholder bound to a whole list covers the common case.
6. **The command side.** The executor receives the `Value::Array`; a Rust command must declare an
   argument type that accepts it (`Value` / `any`), because a `Vec<T>` argument is a `multiple`
   one in `register_command!`. No change there.

## 4. Positional and named arguments

| Need | Named (`arguments: {…}`) | Positional (`parameters: […]`) |
|---|---|---|
| Skip defaulted arguments | natural | empty slots needed, as in a query (`search----20`) |
| Readability and resilience to argument reordering | yes | no |
| Several placeholders into one variadic | `values: {multiple: [ {placeholder: a}, {placeholder: b} ]}` | natural: `[ {value: region}, {placeholder: a}, {placeholder: b} ]` fills `column`, then the variadic `values`, as `is_in-region-…` does |
| Query → pipeline conversion without a registry (U1) | impossible: names are unknown | required |
| Commands whose argument names are not known | no | yes |

Both are needed. Two ways to offer them:

- **One element kind, Python's rule** (recommended): an action element may carry `parameters`
  (positional, consumed first, the last ones filling a variadic) and `arguments` (by name, for the
  slots the positional ones did not reach). A name that targets a slot already filled positionally
  is a `ParameterError`, like Python's "got multiple values for argument". This is a superset, and
  people already know the rule.
- **Two element kinds** (`{action, parameters}` and `{action, arguments}`, never mixed): simpler to
  validate, but a step that needs a positional variadic and one named option must be written
  entirely positionally.

## 5. The command definition and its checks

The command definition holds the pipeline: `CommandDefinition::Composite { pipeline: Pipeline }`.
The placeholders are its `arguments`, declared as ordinary `ArgumentInfo`s, and its
`state_argument` declares the input as usual. Every placeholder must be a declared argument.
Problems are reported as `Issue::CommandRegistry` entries of an `IssueReport`, through the
registry's existing `CommandMetadata::check` path (`liquers-core/src/command_metadata.rs`,
`issue_report.rs`). An `Error`-severity issue refuses registration; warnings (an unused argument, an
inferred type) are kept in the report.

Naming. The maintainer calls this kind of command an alias, so the variant could be named after
that instead of introducing *composite*: `CommandDefinition::Pipeline { pipeline }`, read as "a
pipeline alias". `Alias` remains the single-target, head-parameter form.

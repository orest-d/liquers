# DOC-08: Recipes and Plans

Status: Complete
Last reviewed: 2026-07-29

## Outcome

DOC-08 provides the verified analysis needed for an API-reference-level
description of recipe resolution, query planning, and plan execution.

The primary implementation references are
[`liquers-core/src/recipes.rs`](../../liquers-core/src/recipes.rs) and
[`liquers-core/src/plan.rs`](../../liquers-core/src/plan.rs). This document
defines:

- The boundary among a query, recipe, plan, and evaluated asset
- Recipe validation, named overrides, working-directory behavior, and derived keys
- The store layout and lookup contract of `DefaultRecipeProvider`
- `PlanBuilder` defaults for placeholders and predecessor expansion
- Synchronous planning versus asynchronous dependency finalization
- Planning diagnostics versus executable steps
- The role and authority of volatility, expiration, and dependency fields
- Which APIs are ordinary application entry points and which are framework hooks

## Authority and sources

Claims were verified in this order:

1. [`liquers-core/src/recipes.rs`](../../liquers-core/src/recipes.rs)
2. [`liquers-core/src/plan.rs`](../../liquers-core/src/plan.rs)
3. [`liquers-core/src/interpreter.rs`](../../liquers-core/src/interpreter.rs)
4. [`liquers-core/src/assets.rs`](../../liquers-core/src/assets.rs)
5. [`liquers-core/src/context.rs`](../../liquers-core/src/context.rs)
6. Core recipe, plan, asset, expiration, volatility, and dependency tests
7. [`specs/PROJECT_OVERVIEW.md`](../PROJECT_OVERVIEW.md) as supplementary
   conceptual material

Source and executable tests take precedence over the overview and older plans.

## Runtime relationship

```text
Query
  -> keyed recipe lookup (when the query identifies a recipe-backed asset)
  -> Environment::apply_recipe
       -> Recipe::to_plan
       -> interpreter::finalize_plan
       -> combine recipe and plan expiration
       -> interpreter::apply_plan
  -> State / AssetRef
  -> optional asset persistence
```

A `Query` is syntax. A `Recipe` adds human metadata, named parameter overrides,
logical working directory, volatility, expiration, and provider validation state.
A `Plan` contains resolved interpreter operations. An asset owns the runtime state,
metadata, waiting, persistence, and notification lifecycle.

Neither `Recipe::to_plan` nor `PlanBuilder::build` executes commands. Neither
returns an `AssetRef` or a result `State`.

## Recipe contract

`Recipe::new` parses the supplied query and stores its canonical encoding. Public
fields and Serde deserialization do not validate strings eagerly, so
`get_query`, `get_cwd`, `to_plan`, and methods derived from them remain fallible.

| Field | Operational meaning |
|---|---|
| `query` | Query compiled into a plan |
| `title`, `description` | Human-facing recipe metadata |
| `arguments` | JSON-value overrides by parameter name |
| `links` | Query-link overrides by parameter name |
| `cwd` | Logical `Key` for relative query and link resolution |
| `volatile` | Forces recipe volatility in addition to plan analysis |
| `has_circular_dependencies` | Provider validation result, not recomputed by `to_plan` |
| `circular_dependency_key` | Reported key associated with the detected cycle |
| `expires` | Recipe-level expiration combined with finalized plan expiration |

`Recipe::to_plan` enables placeholders, builds the query, and applies overrides to
the last action step only. An override whose name is not present on that action is
an error. Link strings are parsed during conversion.

`has_arguments` includes both value and link overrides. Consequently `key` returns
a key only when the recipe has no overrides and its query is a key query. When
`cwd` is present, that key is converted to its absolute logical form.

`store_to_key` is derived from `cwd` plus the query filename. It describes the
logical destination implied by the recipe; it does not write data.

## Working-directory rules

`cwd` is a Liquers logical key, not a filesystem path. `DefaultRecipeProvider`
assigns the directory containing `recipes.yaml` to recipes that it loads.
Execution installs the recipe working directory on `Context`, after which relative
keys and links can resolve against it.

`RecipeList::set_cwd` is all-or-error in intent but mutates in iteration order. It
sets missing values until it encounters an explicitly populated `cwd`, then
returns an error. A partially mutated list is therefore possible.

## Provider contract

`AsyncRecipeProvider` separates directory discovery, exact lookup, optional
lookup, and plan convenience operations.

| Method | Missing-recipe result |
|---|---|
| `recipe` | `Err` |
| `recipe_opt` | `Ok(None)` |
| `contains` | `Ok(false)` |
| `recipe_plan` | `Err` |
| `assets_with_recipes` | Empty list when the directory has none |

Provider and parsing failures can otherwise remain errors. `get_asset_info`
describes a recipe-backed asset and planning diagnostics; it does not prove that
an evaluated or persisted value exists.

`TrivialRecipeProvider` contains no recipes. `DefaultRecipeProvider` uses this
logical layout:

```text
<directory>/recipes.yaml
```

The YAML root is `RecipeList { recipes: Vec<Recipe> }`. Asset names come from each
recipe query's filename. Recipes without a valid filename are omitted from
directory listing. `get_recipes` maps any `get_bytes` failure to an empty list,
not only a missing file; malformed YAML from successfully read bytes is an error.

## Planning contract

`PlanBuilder::new` borrows a `CommandMetadataRegistry`, rejects placeholders, and
expands predecessor queries by default.

| Builder setting | Effect |
|---|---|
| Default predecessor expansion | Compiles predecessor operations into the same plan |
| `disable_expand_predecessors` | Emits `Step::Evaluate` boundaries |
| Default placeholder policy | Missing required values are planning errors |
| `with_placeholders_allowed` | Allows recipe overrides to fill unresolved parameters |

During `build`, the planner resolves command namespaces and aliases, parameters,
defaults, enum mappings, injected parameters, explicit links, command volatility,
and command expiration. The special `v` instruction marks a plan volatile without
creating an action step. The `q` instruction produces a query value and accepts no
arguments.

Recipe value and link overrides affect only the last `Step::Action`. They do not
provide general substitution across every action in a plan.

## Plan fields and execution

| Field | Meaning |
|---|---|
| `query` | Source query |
| `init_steps` | Planning `Info`, `Warning`, and `Error` diagnostics |
| `steps` | Ordered operations interpreted at runtime |
| `is_volatile` | Volatility estimate; authoritative after finalization |
| `expires` | Combined expiration estimate; authoritative after finalization |
| `error` | Structured planning or analysis error |
| `dependencies` | Static dependencies discovered during analysis |

`apply_plan` does not execute `init_steps`. They are copied into metadata by the
plan-to-metadata helpers. `Step::Error` in `steps` logs through `Context::error`;
it does not by itself return an execution error. `Plan::error` is the structured
planning failure channel.

Before sequential step execution, `apply_plan` schedules known keyed dependencies
so they can start concurrently. Steps themselves are then interpreted in order,
and each data-producing step replaces the current value. Context modifiers retain
the current value.

## Finalization and expiration

Synchronous build results are incomplete for environment-backed dependencies.
`interpreter::finalize_plan`:

1. Discovers dependencies through volatility analysis.
2. Incorporates dependency volatility.
3. Incorporates dependency recipe expiration.
4. Seeds the context's pending dependency records.
5. Registers plan dependency edges for keyed plans when the plan is nonvolatile.

Built-in `Environment::apply_recipe` implementations then combine finalized
`plan.expires` with `recipe.expires`, apply that expiration to the context, and
call `apply_plan`.

`interpreter::make_plan` is the dependency-aware helper for an ad-hoc query, but it
has no `Context`, so it performs volatility and expiration analysis without the
context seeding and dependency-manager registration performed by `finalize_plan`.

## Public versus framework APIs

Preferred application-facing APIs:

- `Recipe::new`, its builder-style overrides, and read-only derived information
- `RecipeList` for `recipes.yaml`
- Environment recipe-provider configuration
- `EnvRef::evaluate` and `evaluate_immediately`
- `interpreter::make_plan` for diagnostic inspection

Framework extension APIs:

- Implementing `AsyncRecipeProvider`
- Direct `PlanBuilder` policy configuration
- `Recipe::to_plan`, `finalize_plan`, and `apply_plan`
- Manual `Plan`, `Step`, and `ParameterValue` construction
- Plan splitting and metadata projection

Public visibility does not enforce this distinction.

## Conflicts and unresolved gaps

| Priority | Gap | Evidence and impact | Recommended action |
|---:|---|---|---|
| P0 | Complete recipe application is convention-only | `Recipe::to_plan`, `finalize_plan`, expiration combination, and `apply_plan` must be sequenced manually by each environment | Provide one shared recipe-application helper or a default `Environment` implementation |
| P0 | `RecipeList::set_cwd` can partially mutate before failing | It updates recipes in order and errors at the first existing `cwd` | Validate the whole list before mutation or document/return the partial result explicitly |
| P1 | Provider plan APIs have overlapping but different completeness | `recipe_plan`, `create_plan_with_init_metadata`, `make_plan`, and `finalize_plan` perform different subsets of analysis | Consolidate on one named planning pipeline and deprecate incomplete conveniences |
| P1 | `Plan::error` and executable `Step::Error` have different failure semantics | The former is structured planning state; the latter only logs at runtime | Rename or document the runtime diagnostic variant more explicitly |
| P1 | Recipe override scope is narrower than the data model suggests | Overrides search only the last action | Encode an explicit action target or rename fields/docs to make last-action scope unavoidable |
| P1 | Serialized `Plan` has no compatibility contract | Public Serde derives expose internal steps and required fields | Mark it runtime-internal or version a supported wire schema |
| P1 | Circular-dependency fields can be inconsistent | They are public provider-set fields and `to_plan` does not validate them | Replace them with one structured validation result or validate invariants |
| P1 | Direct recipe reads suppress every store error | `DefaultRecipeProvider::get_recipes` uses `Result::map_or`, mapping permission, transport, and other read failures to an empty list | Suppress only the store's not-found error and propagate all other failures |
| P2 | Default provider suppresses some malformed recipe entries in listings | Invalid/missing filenames are skipped by `assets_with_recipes` | Return entry diagnostics so configuration errors are discoverable |
| P2 | Recipe loading writes to stdout on explicit `cwd` | `set_cwd` prints before returning an error | Remove unconditional output and preserve context in the returned error |

## Verification

The reference is covered by existing recipe and plan unit tests, keyed recipe
asset tests, namespace-resolution tests, and expiration/dependency integration
tests.

Final verification performed for DOC-08:

- `cargo test -p liquers-core recipes::test --lib`: 6 passed
- `cargo test -p liquers-core plan::tests --lib`: 32 passed
- All local Markdown link targets in this analysis and the tracker exist
- `git diff --check`

The tests report pre-existing compiler warnings outside the DOC-08 documentation
scope. No Rust source file is changed by DOC-08.

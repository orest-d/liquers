---
title: Recipes and Plans Reference
kind: reference
audience: internal
area: [core/plan, core/assets, core/context]
reviewed: 2026-08-29
---
# DOC-08: Recipes and Plans

## Outcome

DOC-08 provides the verified analysis needed for an API-reference-level
description of recipe resolution, query planning, and plan execution.

The primary implementation references are
[`liquers-core/src/recipes.rs`](../../../liquers-core/src/recipes.rs) and
[`liquers-core/src/plan.rs`](../../../liquers-core/src/plan.rs). This document
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

1. [`liquers-core/src/recipes.rs`](../../../liquers-core/src/recipes.rs)
2. [`liquers-core/src/plan.rs`](../../../liquers-core/src/plan.rs)
3. [`liquers-core/src/interpreter.rs`](../../../liquers-core/src/interpreter.rs)
4. [`liquers-core/src/assets.rs`](../../../liquers-core/src/assets.rs)
5. [`liquers-core/src/context.rs`](../../../liquers-core/src/context.rs)
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
A `Plan` contains ordered interpreter operations whose key and query operands may
remain source-relative until analysis or execution. An asset owns the runtime state,
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
an error. Link strings are parsed during conversion. When `cwd` is present,
`to_plan` prepends one raw executable `Step::SetCwd` and adds one non-executable
`Step::Info` to `init_steps` with the exact text
`Recipe set CWD to '<encoded-key>'`. It does not rewrite relative query operands.

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

YAML authors must not specify `cwd`. `DefaultRecipeProvider` derives it from the
containing `recipes.yaml` and rejects a loaded recipe that already has the field.
The field remains public and deserializable because programmatic recipe creation
may set it; `Recipe::to_plan` validates that string as a `Key` and preserves it as
the plan prefix described above.

The prefix and query-authored `cwd` instructions compose in execution order. For
recipe CWD `a/b`, the raw query
`-R-cwd/../c/-/action-~X~-R/./hello.txt~E` first installs `a/b`, then resolves
`../c` to `a/c`, and finally evaluates the relative link as
`-R/a/c/hello.txt`. Recipe conversion deliberately keeps both `SetCwd` operands
and the link source-relative.

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

### Selecting a provider by name

`RecipeProviderChoice` names the two built-in providers so a configuration document can select
one as data. It is a field-free `Copy` enum; `provider()` returns `Arc<dyn AsyncRecipeProvider<E>>`
and `boxed_provider()` returns `Box<…>`, matching the two setter shapes in use. Both matches are
exhaustive, so a third built-in provider is a compile error rather than a silent fallback.

| Choice | Provider | Names accepted on input | Emitted |
|---|---|---|---|
| `RecipeProviderChoice::Default` | `DefaultRecipeProvider` | `default` | `default` |
| `RecipeProviderChoice::Trivial` | `TrivialRecipeProvider` | `trivial`, `none`, `no_recipes` | `trivial` |

`Default` is the `#[default]` variant: a document that says nothing about recipes gets working
recipes. That is the *document* default and is deliberately not the same as an environment
constructor's unconfigured default, which is per crate. `FromStr` accepts the same names as
Deserialize and reports an unknown one as an error; `Display` and serialization emit the canonical
name.

The set is closed and there is no registration hook. A host with its own `AsyncRecipeProvider`
still passes the value to the environment directly — custom providers vary too much to be named
here.

## Planning contract

`PlanBuilder::new` borrows a `CommandMetadataRegistry` and rejects placeholders.

| Builder setting | Effect |
|---|---|
| Default placeholder policy | Missing required values are planning errors |
| `with_placeholders_allowed` | Allows recipe overrides to fill unresolved parameters |

The builder **always expands** a predecessor into the same plan. Whether a plan is
later cut at a predecessor boundary is a separate, post-freeze decision — see
"Freezing" and "Predecessor boundaries" below. The builder records what such a cut
would need (`Plan::predecessor`, `Plan::predecessor_steps`, `Plan::volatility_source`)
without acting on it.

During `build`, the planner resolves command namespaces and aliases, parameters,
defaults, enum mappings, injected parameters, explicit links, command volatility,
payload requirements, and command expiration. A payload-required command or link
marks the plan as both payload-required and volatile. The `q` instruction
produces a query value and accepts no arguments.

The `v` instruction is intercepted by the builder before command metadata is
resolved, like `q` and `ns`. It takes no parameters and emits no step, so it is an
identity on the value — and it marks the **whole** plan volatile regardless of
where it appears. That last point is the one a reader is most likely to get wrong:
`a/b/v/c` is volatile throughout, not volatile from `v` onward, so `v`'s position
carries no information. It is therefore a `Declared` volatility source and a plan
containing it is never cut. Making it positional — which would let an author's
declared volatility boundary and the cache boundary coincide — is
`V-INSTRUCTION-IS-WHOLE-PLAN-NOT-POSITIONAL`.

Recipe value and link overrides affect only the last `Step::Action`. They do not
provide general substitution across every action in a plan.

`PlanBuilder` is syntax- and command-metadata-driven; it does not choose an entry
CWD or rewrite relative operands, because it has no environment and no execution
context to take one from. Operands stay source-relative until `finalize_plan`
freezes them against the entry key the `Context` actually holds. That ordering is
what lets a query-authored `SetCwd` take effect before the operands it governs are
resolved.

## Freezing

### What it is

`Plan::freeze_cwd(entry: Option<Key>) -> Result<(Key, bool), Error>` walks
`Plan::steps` **in execution order** with a single `CwdCursor` and rewrites every
CWD-relative operand into absolute form: step keys, `Step::Evaluate` and
`Step::UseQueryValue` queries, link queries inside action parameters, and nested
`Step::Plan`s recursively. It records the key it resolved against in
`Plan::frozen_cwd` and returns the key in effect after the last step, together with
a flag saying whether the logical-root fallback was actually used.

After freezing, the plan is self-contained: nothing in it depends on a working key
any more.

### What problem it solves

Before freezing, three separate passes each re-derived the same walk with their own
cursor — dependency discovery (`find_dependencies`), dependency pre-scheduling
(`schedule_plan_dependencies_from`), and runtime step execution. All three had to
agree, and nothing enforced that they did. Any operand form one pass handled
differently from another produced analysis that did not describe execution:
dependencies registered under one key and fetched under another, a cycle check that
did not see the edge the interpreter would take.

Freezing collapses the three into one. The later passes observe operands that are
already absolute, so their cursors become identities rather than a second opinion.

### When it runs

Inside `finalize_plan`, **before** dependency analysis and expiration, using
`Context::get_cwd_key()` as the entry. That placement matters twice over: it is
after the recipe CWD prefix and any recipe overrides are in place, and it is before
anything reads the plan's dependencies.

`finalize_plan` is already required between `recipe.to_plan()` and `apply_plan()`
in every `Environment::apply_recipe` implementation, so freezing is inherited by
all of them — including implementations outside `liquers-core` — without a second
contract to remember.

Freezing is never folded into `build()`. Plans built for analysis
(`Query::is_volatile`, `requires_payload`) and by the validation CLI have no
environment and no entry key; freezing those against a defaulted root would
silently anchor operands that will later run somewhere else.

### Mechanics

| Element | Rule |
|---|---|
| Key-bearing steps | Resolved against the cursor |
| `Step::SetCwd` | Advances the cursor **and** rewrites its own operand; kept afterwards as provenance |
| `Step::Evaluate`, `Step::UseQueryValue` | Resolved as scoped queries |
| Action link parameters | Resolved against a **clone** of the cursor, so a link's own `-R-cwd` cannot move the enclosing plan |
| Nested `Step::Plan` | **Shares** the cursor, so its final key reaches later outer steps |
| Absolute query's own resource step | Resolved against logical root; its index is read once, before any rewriting |
| `Filename`, `Info`, `Warning`, `Error` | Untouched |

The `Step` match is exhaustive with no default arm, so a new step variant is a
compile error here rather than a silently unfrozen operand.

Two further properties are contractual:

- **Idempotent.** Freezing an already-frozen plan against the same key is a no-op,
  because a non-relative key is returned unchanged. Freezing against a *different*
  key is an error: it means a caller reused a finalized plan under another CWD,
  which `finalize_plan` already forbids. Rebuild from the source query or recipe
  instead.
- **Diagnostics are not scoped like keys.** A link scope protects the working key
  but not the root-fallback flag, which describes the resolution as a whole. The
  caller owns the single warning that follows.

## Predecessor boundaries

### Cutting, and how it differs from freezing

Freezing decides *what operands mean*. Cutting decides *where work happens*.

`Plan::cut_predecessor()` replaces the leading `predecessor_steps` with one
`Step::Evaluate(predecessor)` boundary, keeping any `Step::SetCwd` among them and
leaving the trailing action and filename in the parent. The predecessor then
becomes an asset in its own right instead of steps inlined into its consumer.

Cutting **requires a frozen plan**. Cutting an unfrozen one would produce a boundary
query that still depended on a working key — a query that cannot identify the asset
it names, which is the defect freezing exists to remove.

Volatility, payload requirement, expiration and dependencies are *not* recomputed
by the cut. They were computed over the fully expanded plan, which is precisely why
the builder always expands and the cut happens afterwards.

### Why the default should make the predecessor available

An expanded plan computes its intermediates and throws them away. Cutting makes
each one a first-class asset, which buys three things the framework already knows
how to do but currently cannot apply to an intermediate:

- **Dependency management.** A boundary is a real dependency edge. The intermediate
  gets its own version, its own dependents, and participates in cycle detection and
  invalidation instead of being invisible inside its consumer.
- **Caching and independent expiration.** Two queries sharing a prefix share the
  computation rather than repeating it, and an intermediate expires on its own
  schedule rather than forcing its consumer to recompute wholesale.
- **Parallel execution.** A dependency can be scheduled alongside its siblings. An
  inlined predecessor is necessarily sequential — it runs where it sits.

This **is** the default. `finalize_plan` cuts, after freezing and after the
analysis passes.

An earlier revision of this section deferred that decision, on the ground that the
memory-versus-recomputation trade is per query rather than global. That reasoning
was about cutting *everywhere*, which retains every intermediate and does look
wrong as a global default. One cut retains **one** intermediate, and it is the one
most likely to be shared. The memory counterweight belongs to an asset-manager
retention policy — `CORE-ASSET-GC` — rather than to the shape of a plan.

`CORE-PLAN-POLICY-AND-DEFAULTS` still owns the `cache`, `volatile flags` and
`inline flag` markers; its `expand_predecessors` half is answered here.

### Where a boundary goes

`Plan::cut_predecessor` cuts at the **outermost candidate prefix that can be
cached**. Everything below follows from one fact: *a boundary is a cache entry,
keyed by its query.* Anything that feeds the prefix but is **not** part of that
key makes the entry unsound, and anything that makes the entry worthless makes
the boundary pointless.

There are exactly **three conditions**, and they differ in where the answer lives.

| # | Condition | Why it blocks a boundary | Where the answer lives |
|---|---|---|---|
| 1 | **Volatility** | A boundary recomputed on every request buys none of the three things above, and costs an extra asset and an extra hop | In the plan — per candidate, or whole-plan via `Plan::volatility_source` |
| 2 | **Payload requirement** | A payload is not part of a cache key, so a value computed from one must never sit behind a boundary | In the plan — per candidate, via `Plan::payload_required` |
| 3 | **Input state** | Likewise not part of a cache key. A boundary is evaluated as its own asset, starting from `State::new()`, so a prefix that consumes a caller's state would silently receive nothing | **Not in the plan at all** — only the caller knows it |

Conditions 1 and 2 are decided **per candidate**, by building that candidate's own
plan. Condition 3 is decided **per application**, by the caller.

#### 1 and 2 — per candidate

`Plan::payload_required` and `Plan::is_volatile` answer "does this query need a
payload / is it volatile *anywhere*", which is the wrong question in both
directions. Used as a veto, it throws away the boundary in
`fetch/expensive/render_with_payload`, where everything behind the only candidate
is clean; used as a permit, it cuts straight across `fetch/personalize/render`.
So each candidate's own plan is consulted, and the walk stops at the first that
qualifies:

```
fetch/expensive/render          -> boundary at fetch/expensive
fetch/personalize/render        -> personalize requires a payload; boundary at fetch
fetch/vol_step/render           -> vol_step is volatile;          boundary at fetch
personalize/fetch/render        -> the condition reaches the head; no boundary
```

#### 3 — per application, and why it needs an expanded plan

`AssetManager::apply` and `apply_immediately` hand a caller's state to
`apply_plan`. If the prefix that consumes it has been moved behind a boundary,
the boundary runs as a separate asset from `State::new()` and the state is lost:

```
apply "wrap/wrap" to "x"    expanded -> [[x]]
                            cut      -> [[None]]
```

Forwarding the state into the boundary would not fix it — it would make it worse.
The boundary is cached by its query, so two callers applying different states to
the same prefix would share one entry. **A stateful application therefore requires
a fully expanded plan.**

Because only the caller knows the state, `finalize_plan` takes it and decides:

```rust
finalize_plan(envref, &mut plan, &context, &input_state).await?;
```

- `input_state.is_none()` → the plan is cut where conditions 1 and 2 allow.
- otherwise → the plan is left **fully expanded**, with a `Step::Info` recording
  why.

##### Obtaining a fully expanded plan

Three ways, in the order you are likely to want them:

1. **Apply to a state.** Pass the state to `finalize_plan`, as above. The plan is
   expanded automatically; this is the normal path and needs nothing special.
2. **Ask for it.** `finalize_plan_expanded(envref, &mut plan, &context)` runs the
   same dependency, volatility and expiration analysis and the same freezing, and
   never cuts. Use it when the plan is for *reading* rather than executing —
   explanation, analysis, a diff of what a query means — and when a comparison
   must not be derived from the cutting path (an oracle built from that path
   cannot detect that path regressing).
3. **Do not finalize at all.** `PlanBuilder::build` alone always expands; the cut
   is a later pass. `liquers-validate` relies on this, which is why query
   validation shows the expanded form whatever the evaluation default is.

**Whole-plan volatility declines before the walk starts.** `Plan::volatility_source`
distinguishes the two kinds:

| Source | Means | Effect on a boundary |
|---|---|---|
| `Positional` | A volatile command, or a link to a volatile query. Volatility is a property *of that command*, so everything ahead of it is pure. | A boundary may be cut in front of it. |
| `Declared` | The `v` instruction, a recipe's `volatile: true`, or a recipe `expires:` that is itself volatile. A statement about the whole plan, carrying no position. | Nothing here is cacheable; the plan is not cut at all. |

A `Declared` source appears in no candidate's query, so the walk could not see it
— which is why `Recipe::to_plan` records it and the check comes first.

Every level the walk passes over, and the decline, appends a planning
`Step::Info`, so a plan that was not cut is distinguishable from one that had no
predecessor:

```
Predecessor boundary expanded at 'fetch/personalize': it requires an evaluation payload
Predecessor boundary expanded at 'prefix/vol_step': it is volatile
Predecessor boundary not cut: the plan is declared volatile, so none of it may be cached
```

Two candidates are never chosen: one whose remainder is a trailing filename rather
than an action — cutting there would leave the parent nothing but a `Filename`
step, and a recipe's overrides nothing to patch — and one covering every step,
which would replace the whole plan with a boundary that recomputes it.

### Pitfalls

Every item below was observed, not anticipated.

| Pitfall | What goes wrong |
|---|---|
| A trailing filename is not an action | `Query::predecessor` splits a filename off as the remainder. Recording the predecessor at every recursion level lets the outermost overwrite the inner one with the whole action chain, so a cut swallows the last action and a recipe's overrides have nothing to patch. Record only when the remainder is a real action. |
| A step-range recorded before a prefix is inserted | `Recipe::to_plan` inserts `SetCwd` at index 0 *after* building. A stale `predecessor_steps` then splits in the wrong place and keeps the predecessor's own action, so it runs twice — once in the boundary asset, once inline. |
| A default link is invisible to the cache key | A default lives in command metadata, not query text. An absolute default is reproduced by metadata everywhere, but a **relative** one resolves differently per directory, so it must be promoted into the query. Promotion appends, which is only correct when every earlier argument slot is already written; at a gap it must be skipped rather than bound to the wrong argument. |
| A boundary hides the diagnosis | A dependency failure reported as "did not produce a value" discards the cause, which then lives only in the sub-asset's log. The dependent must surface the cause itself — and must not re-wrap an error that already carries its command name and position. |
| An undeclared payload | A command reading the payload without `payload: required` works inlined and silently receives none across a boundary. This is the documented "declare it, or lose it" rule, not a cutting defect, but a cut is where it first bites. `injected` does **not** imply the requirement: injection may be satisfied from the environment alone. |
| A boundary query frozen before the prologue | Sibling of the row above it, and the same prepended `SetCwd`. The step *count* was compensated; the *cursor* was not, so the recorded predecessor was resolved against the entry CWD and the boundary query — the only thing a cut carries — lost its folder. Silent: it produced a wrong value as readily as a `KeyNotFound`. Fixed by `Plan::prologue_steps`. |
| A recipe-level flag is not in the query | `volatile:` and `expires:` live in the `recipes.yaml` entry, not in the query text, so unlike a volatile *command* they do not travel into a boundary. Measured: the prefix of a `volatile: true` recipe ran once across two evaluations where expanded it ran twice — the parent dutifully recomputing around a cached boundary. Fixed by folding them onto the plan as `VolatilitySource::Declared`. |
| A cut swallows the caller's input state | `apply` and `apply_immediately` supply a state; a boundary runs as its own asset from `State::new()`. Measured: `wrap/wrap` applied to `"x"` yielded `[[None]]`. Forwarding the state would be worse — the boundary is cached by query, so callers with different states would share one entry. A stateful application needs a fully expanded plan, which `finalize_plan` produces when it is given one. |
| `v` emits no step | `a/b` and `a/b/v` report the same step count, so a candidate cannot be identified by index alone; and in `a/b/v` the outermost non-volatile prefix is the *entire* plan. Both are unreachable while `Declared` declines first, and both would return if `v` ever became positional (`V-INSTRUCTION-IS-WHOLE-PLAN-NOT-POSITIONAL`). |

## Plan fields and execution

| Field | Meaning |
|---|---|
| `query` | Source query |
| `init_steps` | Planning `Info`, `Warning`, and `Error` diagnostics |
| `steps` | Ordered operations interpreted at runtime |
| `is_volatile` | Volatility estimate; authoritative after finalization |
| `payload_required` | Whether execution requires an evaluation payload; derived during planning |
| `expires` | Combined expiration estimate; authoritative after finalization |
| `error` | Structured planning or analysis error |
| `dependencies` | Static dependencies discovered during analysis |
| `frozen_cwd` | The working key this plan was frozen against, once frozen |
| `predecessor`, `predecessor_steps` | The boundary the builder recorded and never cut |
| `prologue_steps` | Leading steps not emitted by the builder for `query` — a recipe's CWD prefix |
| `volatility_source` | Whether volatility permits a boundary in front of it (`Positional`) or forbids one anywhere (`Declared`) |

`apply_plan` does not execute `init_steps`. They are copied into metadata by the
plan-to-metadata helpers. `Step::Error` in `steps` logs through `Context::error`;
it does not by itself return an execution error. `Plan::error` is the structured
planning failure channel.

Before sequential step execution, `apply_plan` schedules known keyed dependencies
so they can start concurrently. Steps themselves are then interpreted in order,
and each data-producing step replaces the current value. Context modifiers retain
the current value. `apply_plan` rejects a payload-required plan when its context has
no payload.

Every key-bearing executable step and every query/link operand is resolved when it
is analyzed or consumed. `SetCwd` resolves and installs its operand before later
steps. Nested plans share the live CWD and can change it for following outer steps;
linked queries inherit an entry snapshot but keep their own internal CWD changes
scoped. An absolute source query is aligned with its own generated resource step,
so a recipe prefix is not mistaken for that source step and does not override the
outer query's logical-root meaning.

Static dependency discovery, pre-scheduling, runtime lookup, cycle registration,
and asset caching use the same resolved key identity. Planning cursors operate on
copies and do not mutate the raw plan or prematurely advance the live context.

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

Keyed recipes cannot require an evaluation payload: keys identify globally shared
assets, while a payload belongs to one evaluation. Payload-required nested queries
instead inherit the current context payload and execute as volatile, unshared
inline assets.

`interpreter::make_plan` is the dependency-aware helper for an ad-hoc query, but it
has no `Context`, so it performs volatility and expiration analysis without the
context seeding and dependency-manager registration performed by `finalize_plan`.

## Serialization and future plan rewriting

Recipe and plan JSON/YAML preserve source-relative query text, ordered raw
`SetCwd` steps, links, `QuerySource`, and source positions. Runtime-only cursor
state and root-fallback bookkeeping are not serialized. This is a current data
contract, not a versioned stable wire-format guarantee.

`Plan::frozen_cwd`, `predecessor` and `predecessor_steps` are `serde(default)`, so a
plan serialized before freezing existed still deserializes — and reads as *not
frozen* rather than as frozen against the root.

A plan serialized **after** `finalize_plan` carries absolute operands and is
therefore specific to the CWD it was frozen against. This is not a new restriction:
`finalize_plan` already forbids re-finalizing a plan under another CWD, and callers
rebuild from the source query or recipe. Freezing makes that visible in the data
rather than implicit in the contract.

There is still no plan optimizer or substitution pass. Freezing deliberately keeps
`SetCwd` steps: removing them is an optimizer's job, and because callers can inspect
and serialize `Plan::steps` and `init_steps`, such a pass must treat their ordering
and diagnostics as observable and preserve execution, dependency identity, source
provenance, and the recipe CWD `Info` diagnostic.

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
tests. CWD-specific coverage includes provider and programmatic provenance, raw
plan serialization, ordered dependency traversal, interpreter execution, nested
link/plan scope, absolute-source alignment, and root-fallback concurrency.

Review verification on 2026-08-09:

- `cargo test -p liquers-core --lib`: 446 passed
- `cargo test -p liquers-core --doc`: 5 passed, 2 intentionally ignored
- `cargo doc -p liquers-core --no-deps`: completed with three known private-item
  link warnings
- All relative Markdown links in `specs/reference/api/` resolve
- `git diff --check` passes

The tests report pre-existing compiler warnings outside the DOC-08 documentation
scope. Applying this reference to source-level Rustdoc changes documentation only;
runtime behavior is unchanged.

## History

| Date | Change | Source |
|---|---|---|
| 2026-08-29 | Documented `RecipeProviderChoice`: the named selection of the two built-in providers, the `trivial` aliases `none` and `no_recipes`, the document default, and why the set is closed. | RECIPE-PROVIDER-BY-NAME |
| 2026-08-26 | Cutting at the outermost cacheable predecessor is now the **default**. Added "Where a boundary goes" — the three conditions (volatility, payload, input state), which are per candidate and which per application, and how to obtain a fully expanded plan. Superseded the paragraph deferring that decision; five new pitfall rows; `frozen_cwd`, `predecessor`, `prologue_steps` and `volatility_source` in the plan fields; a paragraph on `v`'s whole-plan scope. | PREDECESSOR-CUT-EQUIVALENCE |
| 2026-08-16 | Documented freezing — what it is, the three-cursor problem it solves, when it runs, its mechanics and scope rules — and predecessor boundaries: how cutting differs from freezing, the dependency, caching and parallelism case for making a predecessor available, and five observed pitfalls. Removed `disable_expand_predecessors` from the planning contract. | PLAN-CWD-FREEZE |
| 2026-08-11 | Documented provider and programmatic recipe CWD provenance, raw plan prefixes and diagnostics, ordered runtime resolution, serialization, identity, and optimizer constraints. | phase-5 |
| 2026-08-09 | Applied the verified recipe and planning contracts to comprehensive module and public-API Rustdoc in `plan.rs` and `recipes.rs`. | DOC-08 |
| 2026-08-09 | Reviewed recipe resolution, plan building, payload requirements, finalization, and execution against HEAD; documented `Plan::payload_required` and corrected links. | PAYLOAD-INHERITANCE |
| 2026-07-29 | Verified recipe resolution, planning, finalization, and execution against the implementation and focused tests. | DOC-08 |

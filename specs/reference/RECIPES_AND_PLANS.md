---
title: Recipes and Plans Reference
kind: reference
audience: internal
area: [core/plan, core/query, core/assets, core/context]
reviewed: 2026-10-27
---
# Recipes and Plans

## Scope

This document is the reference for recipe resolution, query planning and plan execution in
`liquers-core`: what a `Recipe`, a recipe provider, a `Plan` and plan finalization guarantee, and
which steps are environment hooks. It covers:

- The boundary among a query, recipe, plan, and evaluated asset
- Recipe validation, named overrides, working-directory behavior, and derived keys
- The store layout and lookup contract of `DefaultRecipeProvider`, and provider composition
- `PlanBuilder` defaults for placeholders and predecessor expansion
- Planner instructions (`ns`, `q`, `v`, resource header instructions) versus commands
- Synchronous planning versus asynchronous dependency finalization
- Planning diagnostics versus executable steps
- The role and authority of volatility, expiration, and dependency fields
- Which APIs are ordinary application entry points and which are framework hooks

## Sources

The implementation is [`liquers-core/src/recipes.rs`](../../liquers-core/src/recipes.rs),
[`plan.rs`](../../liquers-core/src/plan.rs),
[`interpreter.rs`](../../liquers-core/src/interpreter.rs) (finalization and execution),
[`cache_strategy.rs`](../../liquers-core/src/cache_strategy.rs),
[`context.rs`](../../liquers-core/src/context.rs) (`Environment::apply_recipe`),
[`environment_builder.rs`](../../liquers-core/src/environment_builder.rs) and
[`environment_config.rs`](../../liquers-core/src/environment_config.rs) (provider and `assets:`
configuration), and [`liquers-records/src/provider.rs`](../../liquers-records/src/provider.rs)
(`ManifestRecipeProvider`). Where this document and the code disagree, the code and its tests are
right and this document is the bug. The query syntax itself is in
[`api/DOC_02_QUERY_LANGUAGE_REFERENCE.md`](api/DOC_02_QUERY_LANGUAGE_REFERENCE.md); realms are in
[`REALMS.md`](REALMS.md).

## Runtime relationship

```text
Query
  -> keyed recipe lookup (when the query identifies a recipe-backed asset;
                          the asset manager checks it with Recipe::to_plan_for_key)
  -> Environment::apply_recipe
       -> Recipe::to_plan
       -> interpreter::finalize_plan (freeze CWD, analyse dependencies,
                                      seed/register dependencies, cut predecessor boundary)
       -> set plan.expires on the context
       -> interpreter::apply_plan
  -> State / AssetRef
  -> optional asset persistence
```

A non-keyed query reaches the same path wrapped in an ad-hoc `Recipe` (`From<Query>`); the asset
manager first plans it with `interpreter::make_plan` to decide volatility and registration
(`assets.rs`).

A `Query` is syntax. A `Recipe` adds human metadata, named parameter overrides, a logical working
directory, volatility, expiration and caching declarations. A `Plan` contains ordered interpreter
operations whose key and query operands may remain source-relative until finalization freezes
them. An asset owns the runtime state, metadata, waiting, persistence, and notification lifecycle.

Neither `Recipe::to_plan` nor `PlanBuilder::build` executes commands. Neither returns an
`AssetRef` or a result `State`.

## Recipe contract

`Recipe::new` parses the supplied query and stores its canonical encoding. Public fields and Serde
deserialization do not validate strings eagerly, so `get_query`, `get_cwd`, `to_plan`, and methods
derived from them remain fallible.

| Field | Operational meaning |
|---|---|
| `query` | Query compiled into a plan |
| `title`, `description` | Human-facing recipe metadata |
| `arguments` | JSON-value overrides by parameter name |
| `links` | Query-link overrides by parameter name |
| `cwd` | Logical `Key` for relative query and link resolution |
| `volatile` | Declares the whole plan volatile; `to_plan` records it as `VolatilitySource::Declared`, which forbids every predecessor cut. A plan reading this key as state becomes volatile at finalization (a link to it contributes only this flag, not its upstream's) |
| `has_circular_dependencies`, `circular_dependency_key` | Reserved; no built-in provider sets them and nothing reads them. Cycle detection happens in dependency analysis (§Finalization and expiration). See [`RECIPE-CIRCULAR-DEPENDENCY-FIELDS-ARE-DEAD`](../issues/RECIPE-CIRCULAR-DEPENDENCY-FIELDS-ARE-DEAD.md) |
| `expires` | Recipe-level expiration, folded into `plan.expires` by `to_plan`; an expiration that is itself volatile (`immediately`, or a combination containing it) counts as `volatile: true`. Written as a string, e.g. `in 5 min`, `eod`, `at 18:00`, `on Monday`, `2026-03-01`, or a comma-separated combination (`Expires::from_str`, `expiration.rs`) |
| `stored` | `Option<bool>`, absent = `true`. `false`: the produced value is never written to the store, not even as metadata; an existing stored copy is still read and preferred. Read through `stored()` |
| `cached` | `Option<CacheStrategy>` — the **caching strategy** of an evaluation of this recipe: `all` (the keyed result and every intermediate it creates are registered), `result` (only the result; a missing intermediate is computed unregistered), `none` (nothing new is registered; the keyed asset still stays the key's dependency-graph node). `true`/`false` are read as `all`/`none`; absent, `null` or `default` means the manager's `assets.recipe_cache_strategy` (keyed) or `assets.query_cache_strategy` (non-keyed). An intermediate that already exists is reused under every strategy. Read through `effective_cache_strategy(default)`. Neither this nor `stored` makes the recipe volatile — see §Predecessor boundaries, "What is kept" |

A `recipes.yaml` entry, as `DefaultRecipeProvider` reads it (the asset name, `top.csv`, comes from
the query's filename):

```yaml
recipes:
  - query: "-R/data/x.csv/-/ns-pl/head-10/top.csv"
    title: "Top rows"
    expires: "eod"
    cached: result
```

`Recipe::to_plan` enables placeholders, builds the query, and applies overrides to the last action
step only, in **sorted name order** (so warnings and the first reported error do not depend on
`HashMap` order). Each name must be an argument of the command the query named for that action
(`Plan::last_action_command`): for an alias, the alias's own arguments — its head slots carry the
target's names and are fixed, so they cannot be overridden. A name that is not such an argument,
an `injected` argument, or a query with no action is a `ParameterError`. A required argument no
override fills remains a placeholder: planning accepts it, and the command fails when it runs
("Unresolved placeholder in argument …", `commands.rs`).

A value override is converted to its argument's type first. A matching JSON type passes unchanged.
A mismatched one that converts — a numeric or boolean string for an `int`, `float` or `bool`
argument (parsed as a path parameter would be), a number or boolean for a `string` argument (its
JSON text), an integral float such as `3.0` for an `int` — is converted, and a `Step::Warning`
naming the argument, the given and the expected type goes to `init_steps`, from where it reaches
the asset's log. A `null` is accepted only for the optional types. One that does not convert is a
`ConversionError`. An enum alias string expands to its value (a query alternative becomes a link
override, with a `Step::Warning`); a non-string value, or a string that is no alias, passes
unchanged, and inside a `multiple` argument a link alias is a `ConversionError`. A `multiple`
argument converts element-wise. `any`, `none` and global-enum arguments take any value unchanged.
Because an override patches that action, its recorded prefix query (`Step::Action::query`) no
longer describes what runs and is cleared; earlier actions keep theirs.

`to_plan` also folds the recipe's `volatile` and `expires` into the plan (above), because they are
not in its query and nothing downstream could recover them.

A recipe looked up by key must be planned with `Recipe::to_plan_for_key`, which rejects one
requiring an evaluation payload ("Recipe for key '…' requires an evaluation payload, but keyed
recipes cannot receive one…"): keys are a payload boundary (§Finalization and expiration). The
asset manager and the dependency walk use it; `apply_recipe` then rebuilds the plan with plain
`to_plan`.

`Recipe::data_format` is the query's filename extension, `None` without a filename, and
`Recipe::get_asset_info` declares exactly that. An unnamed query therefore declares **no** format:
the value's own default applies when it is serialized
([`VALUE_TYPE_SYSTEM.md`](VALUE_TYPE_SYSTEM.md#the-encoding-axis)). A keyed asset whose recipe query
has no filename takes its format from the key's filename. Link strings are parsed during
conversion. When `cwd` is present, `to_plan` prepends one raw executable `Step::SetCwd` and adds one
non-executable `Step::Info` to `init_steps` with the exact text `Recipe set CWD to '<encoded-key>'`.
It does not rewrite relative query operands.

`has_arguments` includes both value and link overrides. Consequently `key` returns a key only when
the recipe has no overrides and its query is a key query. When `cwd` is present, that key is
converted to its absolute logical form.

`store_to_key` is `cwd` joined with the query filename, and `None` unless both are present. It
describes the logical destination implied by the recipe; it does not write data.

## Working-directory rules

`cwd` is a Liquers logical key, not a filesystem path. `DefaultRecipeProvider` assigns the
directory containing `recipes.yaml` to recipes that it loads. Execution installs the recipe working
directory on `Context`, after which relative keys and links can resolve against it.

YAML authors must not specify `cwd`. `DefaultRecipeProvider` derives it from the containing
`recipes.yaml` and refuses the whole file if any entry sets it: every lookup in that directory then
fails with `NotSupported` ("CWD can't be explicitly specified in a recipe", with the directory
key), and the file is not cached. The field remains public and deserializable because programmatic
recipe creation may set it — `ManifestRecipeProvider` sets each chunk recipe's `cwd` to the
manifest's folder — and `Recipe::to_plan` validates that string as a `Key` and preserves it as the
plan prefix described above. A command cannot read the working directory from its `Context`
(`get_cwd_key` is crate-private); pass it in as a default link (`dir: Key = query "-R-key/."`).

The prefix and query-authored `cwd` instructions compose in execution order. For recipe CWD `a/b`,
the raw query `-R-cwd/../c/-/action-~X~-R/./hello.txt~E` first installs `a/b`, then resolves `../c`
to `a/c`, and finally evaluates the relative link as `-R/a/c/hello.txt`. Recipe conversion
deliberately keeps both `SetCwd` operands and the link source-relative.

`RecipeList::set_cwd` sets missing values in iteration order until it meets an explicitly populated
`cwd`, then writes a diagnostic to stderr and returns an error, so a partially mutated list is
possible
([`RECIPE-LIST-SET-CWD-MUTATES-BEFORE-FAILING`](../issues/RECIPE-LIST-SET-CWD-MUTATES-BEFORE-FAILING.md)).

## Provider contract

`AsyncRecipeProvider` separates directory discovery, exact lookup, optional lookup, and plan
convenience operations.

| Method | Missing-recipe result |
|---|---|
| `recipe` | `Err` |
| `recipe_opt` | `Ok(None)` |
| `contains` | `Ok(false)` |
| `can_make` | `Ok(false)` |
| `recipe_plan` | `Err` |
| `assets_with_recipes` | Empty list when the directory has none |

**Listed vs producible.** A provider answers two questions. `contains(key)` asks whether `key` is
among what its folder **shows** (what `assets_with_recipes` lists for `key.parent()`).
`can_make(key)` asks whether the provider can **produce** `key`, listed or not: it defaults to
`recipe_opt(key).is_some()`, errors from `recipe_opt` propagate, and `can_make ⊇ contains` must
hold. A provider that makes keys on demand (template-generated names) therefore needs no override
to answer correctly: it lists nothing, `contains` is `false`, `can_make` is `true`.
`directory_changed(dir)` (default: nothing) is called by the asset manager after every write or
removal it mediates, with the written key's parent and with each ancestor level whose listing it
refreshes; a provider that caches directory-derived state drops it for `dir` and its subtree.

Provider and parsing failures can otherwise remain errors. `get_asset_info` describes a
recipe-backed asset and planning diagnostics; it does not prove that an evaluated or persisted value
exists.

`TrivialRecipeProvider` contains no recipes. `DefaultRecipeProvider` uses this logical layout:

```text
<directory>/recipes.yaml
```

The YAML root is `RecipeList { recipes: Vec<Recipe> }`. Asset names come from each recipe query's
filename. Recipes without a valid filename are omitted from directory listing. The directory read
maps **every** `get_bytes` error to an empty list, not only a missing file
([`RECIPES-YAML-READ-ERRORS-READ-AS-NO-RECIPES`](../issues/RECIPES-YAML-READ-ERRORS-READ-AS-NO-RECIPES.md));
malformed YAML from successfully read bytes is an error. `recipe_opt` asks `has_recipes` first, so a
store `contains` error is an error — except that `has_recipes` answers `false` when the store
refuses the `recipes.yaml` key as `KeyNotSupported` (a store router does so for any key no member
covers, such as an `http` store serving a fixed key list), so `-R/` queries under such a folder
resolve without a recipe instead of failing.

`DefaultRecipeProvider` is constructed with `DefaultRecipeProvider::new()` (or `Default`): it caches
each directory's parsed recipes. Every lookup still reads the stored `recipes.yaml` bytes and
compares them with the cached copy, so a change — including one made outside Liquers — is seen at
the next lookup; a hit saves the YAML parse, and a recipe is found by name in O(1). When two recipes
in one file share a filename, the first wins, as in `RecipeList::get`. A malformed file, and an
empty read, are not cached.

### Composing providers: `RecipeProviderChain`

`RecipeProviderChain` consults several providers in order and is itself an `AsyncRecipeProvider`:

| Method | Chain behaviour |
|---|---|
| `recipe_opt` | The first provider answering `Some`; an `Err` from an earlier provider propagates without consulting the rest |
| `contains` | `true` if any provider's `contains` is |
| `can_make` | `true` if any provider's **`recipe_opt`** answers `Some` |
| `directory_changed` | forwarded to every provider, in order |
| `has_recipes` | `true` if any provider's does |
| `assets_with_recipes` | The union, in provider order, without duplicates |
| `recipe`, `recipe_plan`, `get_asset_info` | Delegated whole to the provider that has the key; otherwise the same not-found error `DefaultRecipeProvider` returns |

A chain is built in code: `EnvironmentBuilder::with_appended_recipe_provider` (composed at
`build()` after the configured or kind-default base) or
`GenericEnvironment::with_appended_recipe_provider` (wraps the current provider). `liquers-lib`
with its `records` feature defaults to `[DefaultRecipeProvider, ManifestRecipeProvider]`
(`LibKind::default_recipe_provider`); the second is a generative provider synthesizing recipes for
keyed record chunks named by a `*.manifest.yaml`; it lists only explicit chunks (`contains`) and
produces template chunks (`can_make`, through the default). A build that replaces the base provider
(`with_recipe_provider`, `with_recipe_provider_choice`, or a configuration document's `recipes:`
key) loses the manifest provider unless it also calls
`RecordsRecipeProvider::with_records_recipe_provider()`. An `Err` from the manifest provider (for
example, a malformed manifest) propagates through the chain for every key in that folder that
`recipes.yaml` does not define. See
[`ENVIRONMENT_CONFIG.md`](ENVIRONMENT_CONFIG.md) §The recipe provider chain.

### Selecting a provider by name

`RecipeProviderChoice` names the two built-in providers so a configuration document can select one
as data (`recipes: default|trivial`, `EnvironmentConfig.recipes`). It is a field-free `Copy` enum;
`provider()` returns `Arc<dyn AsyncRecipeProvider<E>>` and `boxed_provider()` returns `Box<…>`,
matching the two setter shapes in use. Both matches are exhaustive, so a third built-in provider is a
compile error rather than a silent fallback.

| Choice | Provider | Names accepted on input | Emitted |
|---|---|---|---|
| `RecipeProviderChoice::Default` | `DefaultRecipeProvider` | `default` | `default` |
| `RecipeProviderChoice::Trivial` | `TrivialRecipeProvider` | `trivial`, `none`, `no_recipes` | `trivial` |

`Default` is the `#[default]` variant: a document that says nothing about recipes gets working
recipes. That is the *document* default and is deliberately not the same as an environment
constructor's unconfigured default, which is per asset-manager kind: `Trivial` for the
`liquers-core` kinds, `Default` (plus the manifest provider with `records`) for `liquers-lib`'s
`LibKind`. `FromStr` accepts the same names as Deserialize and reports an unknown one as an error;
`Display` and serialization emit the canonical name.

The set is closed and there is no registration hook. A host with its own `AsyncRecipeProvider`
still passes the value to the environment directly — custom providers vary too much to be named
here — or appends it to a chain (§Composing providers). No "chain" choice exists either: a choice is
data and cannot name a provider living in another crate.

## Planning contract

`PlanBuilder::new` borrows a `CommandMetadataRegistry` and rejects placeholders.

| Builder setting | Effect |
|---|---|
| Default placeholder policy | Missing required values are planning errors |
| `with_placeholders_allowed` | Allows recipe overrides to fill unresolved parameters |

The builder **always expands** a predecessor into the same plan. Whether a plan is later cut at a
predecessor boundary is a separate, post-freeze decision — see "Freezing" and "Predecessor
boundaries" below. The builder records what such a cut would need (`Plan::predecessor`,
`Plan::predecessor_steps`, `Plan::volatility_source`) without acting on it.

During `build`, the planner handles planner instructions (next section), resolves command
namespaces and aliases, parameters, defaults, enum mappings, injected parameters, explicit links,
command volatility, payload requirements, and command expiration. A payload-required command or link
marks the plan payload-required; such a command is also volatile, because `register_command!`'s
`payload: required` sets `volatile` (a hand-built `CommandMetadata` can break that invariant).

### Planner instructions

A **planner instruction** is a part of a query that the plan builder consumes itself. It shapes the
plan — which namespaces commands are looked up in, where volatility starts, whether the query is
quoted as a value, which kind of read a resource segment performs — but it is never looked up in the
command registry and never becomes a `Step::Action`. A **command** is the opposite: it is resolved
through command metadata and runs as a `Step::Action` when the interpreter reaches it.

There are four planner instructions. Three of them — `ns`, `q` and `v` — are written exactly like
actions, so the distinction is one of planning, not of syntax: the parser produces an ordinary
action and the plan builder recognises it by name. The fourth is the resource header instruction,
the parameter of a resource segment's header (`-R-meta/…`).

| Instruction | Written | Plan effect | Parameters |
|---|---|---|---|
| `ns` | an action anywhere in a transform segment | Selects namespaces; emits no step | Any number of strings |
| `q` | an action | Quotes the query before it as a value: `Step::UseQueryValue` | None |
| `v` | an action | Makes the plan volatile from this position on; emits no step | None |
| resource header instruction | `-R-<instr>/key` | Chooses the step that reads the key | At most one (none = `GetAsset`) |

The action-shaped instructions are recognised by name before any command lookup, so a registered
command called `ns`, `q` or `v` can never be reached. `CommandMetadata::check` reports a command
named `ns` as an error ("Command name 'ns' is reserved"); `q` and `v` are not reported yet
([`COMMAND-NAMES-Q-AND-V-ARE-NOT-RESERVED`](../issues/COMMAND-NAMES-Q-AND-V-ARE-NOT-RESERVED.md)).

The planner also consumes two pieces of query *syntax* that are not instructions, since the parser
already gives them their own structure: a **transform segment header** (`-<realm>/…`) chooses the
realm the segment's actions are looked up in, and a **filename** (the last element, with an
extension) becomes `Step::Filename`, naming the result and its format. Both are described at the
end of this section.

**`ns`.** `ns-a-b` names namespaces searched, in order, before the registry's
`default_namespaces`. Only the last `ns` before an action in the same transform segment applies — a
later `ns` replaces an earlier one rather than adding to it — and it does not carry across a `-/`
segment boundary. In `ns-pl/head-3/ns-img/x`, `head` is looked up in `pl` and `x` in `img`, not in
`pl`. `ns` emits no step and leaves `uncached_by` unchanged; a non-string parameter is
`NotSupported` ("Only string parameters are supported in ns").

**`q`.** `q` makes the query so far a value. `a/b/q` plans as `[UseQueryValue(a/b)]`, and in
`a/b/q/c` the action `c` receives the query `a/b` as its input
(`-R/data/test.csv/-/command1-arg/q/command2-arg` is
`[UseQueryValue(-R/data/test.csv/-/command1-arg), Action(command2)]`, pinned by
`test_q_instruction_plan`). A trailing filename stays a separate `Step::Filename`
(`upper/q/out.txt`). The quoted part is **not planned**: its commands are neither resolved nor
validated (`nosuch/q` plans without error), and it contributes no volatility, payload requirement,
expiration or dependency, and no predecessor is recorded inside it, so a boundary can only fall
after the `q`. An `ns` inside the quoted part still applies to the actions after `q` in the same
segment (`ns-pl/head/q/upper` looks `upper` up in `pl`), by the `ns` rule above. Its operands are
frozen like any other query operand. `q` clears `uncached_by`. A trailing `q` with a parameter is
`NotSupported` ("The 'q' instruction does not accept any arguments"); a `q` in the middle of a query
currently ignores its parameters
([`MID-QUERY-Q-INSTRUCTION-DISCARDS-ARGUMENTS`](../issues/MID-QUERY-Q-INSTRUCTION-DISCARDS-ARGUMENTS.md)).

**`v`.** `v` takes no parameters (`v-x` is `TooManyParameters`, "instruction 'v'") and emits no
step, so it is an identity on the value — and it is **positional**: everything from `v` onward is
volatile, while the prefix ahead of it stays pure. `a/b/v/c` is volatile, and its `a/b` is cut as a
cached boundary; `c` runs on every request. A trailing `v` is the same rule with nothing after it:
`a/b/v` is `[Evaluate(a/b)]`, which serves the cached `a/b` as a volatile, unmanaged asset. `v` at
the head leaves no pure prefix, so `v/a/b/c` is volatile throughout and cuts nothing — write it
there to recompute everything. A recipe's `volatile: true`, which carries no position, makes the
whole plan volatile. `v` keeps `uncached_by`. Unlike `ns` and `q`, which `process_query` intercepts,
`v` is intercepted in `process_action`, before command metadata is resolved.

**Resource header instructions.** The parameter of a resource segment's header, if any, selects the
step that reads its key:

| Header instruction | Step |
|---|---|
| none, `data`, `value` | `GetAsset` |
| `b`, `bin`, `binary` | `GetAssetBinary` |
| `meta`, `metadata` | `GetAssetMetadata` |
| `dir`, `directory` | `GetAssetDirectory` |
| `sdir`, `store_directory` | `GetResourceDirectory` |
| `r`, `recipe` | `GetAssetRecipe` |
| `stored`, `stored_binary`, `stored_bin`, `sbin` | `GetResource` |
| `stored_meta`, `stored_metadata` | `GetResourceMetadata` |
| `cwd` | `SetCwd` (installs the key as working directory; see §Working-directory rules) |
| `key` | `UseKeyValue` (the key itself as the value; frozen, not a dependency, not a bare key read) |

An unknown instruction is `NotSupported` with the list of valid ones; a second parameter is
`TooManyParameters`. The header *name* (`-Rname-meta/…`) is reserved: it is warned about ("Resource
header name is ignored") and otherwise ignored. A resource step clears `uncached_by`.

**Transform segment headers** (syntax, not an instruction). Every action of a transform segment is looked up in the realm its
header names (`-<realm>/…`; none, or an empty name, is realm `""`); see [`REALMS.md`](REALMS.md).

**Filenames** (syntax, not an instruction). A last element with an extension (`upper/out.txt`)
becomes `Step::Filename`. It sets the asset's filename and, unless one was declared, the data
format named by its extension; it changes no value and keeps `uncached_by`.

### Volatility, caching and aliases

A resource step — `-R-stored/…`, `-R-stored_meta/…` or `-R-sdir/…`, planned as `GetResource`,
`GetResourceMetadata` or `GetResourceDirectory` and read straight from the store — never makes a
plan volatile. Store data is not tracked by volatility: a resource's changes reach its dependants
through its version and dependency records. `Plan::is_volatile` — set by the builder,
`Recipe::to_plan` and finalization — is the only static volatility authority; the crate-private
`IsVolatile` answers a `Query`, `Recipe` or `Plan` from it and has no step-level implementation. At
run time `apply_plan` marks the context volatile from it.

A command registered with `cached: false` sets `Plan::uncached_by` when it is the **last** action:
the next action, a resource step or `q` clears it, and a filename, `ns` or `v` keeps it. It is not
contagious — `a/u` is uncached, `u/a` is not.

A command alias is resolved here, completely: the builder validates it against its target, applies
the target's volatility, payload requirement and expiration as well as the alias's, and emits one
`Step::Action` **for the target** whose `origin` is `ActionOrigin::Alias { command }` (`origin` is
`Direct` for every other action). The alias's own parameters keep the alias's names, so recipe
overrides by name use them. A diagnostic naming the alias goes to `init_steps`. See
[`COMMAND_ALIASES.md`](COMMAND_ALIASES.md).

Recipe value and link overrides affect only the last `Step::Action`. They do not provide general
substitution across every action in a plan
([`RECIPE-OVERRIDE-ONLY-LAST-ACTION`](../issues/RECIPE-OVERRIDE-ONLY-LAST-ACTION.md)).

`PlanBuilder` is syntax- and command-metadata-driven; it does not choose an entry CWD or rewrite
relative operands, because it has no environment and no execution context to take one from.
Operands stay source-relative until `finalize_plan` freezes them against the entry key the `Context`
actually holds. That ordering is what lets a query-authored `SetCwd` take effect before the operands
it governs are resolved.

## Freezing

### What it is

`Plan::freeze_cwd(entry: Option<Key>) -> Result<(Key, bool), Error>` walks `Plan::steps` **in
execution order** with a single `CwdCursor` and rewrites every CWD-relative operand into absolute
form: step keys, `Step::Evaluate` and `Step::UseQueryValue` queries, link queries inside action
parameters, and nested `Step::Plan`s recursively. It records the key it resolved against in
`Plan::frozen_cwd` and returns the key in effect after the last step, together with a flag saying
whether the logical-root fallback was actually used.

After freezing, the plan is self-contained: nothing in it depends on a working key any more.

### What problem it solves

Without freezing, three separate passes would each re-derive the same walk with their own cursor —
dependency discovery (`find_dependencies`), dependency pre-scheduling
(`schedule_plan_dependencies_from`), and runtime step execution. All three would have to agree, and
nothing would enforce that they did. Any operand form one pass handled differently from another
would produce analysis that did not describe execution: dependencies registered under one key and
fetched under another, a cycle check that did not see the edge the interpreter would take.

Freezing collapses the three into one. The later passes observe operands that are already absolute,
so their cursors become identities rather than a second opinion.

### When it runs

Inside `finalize_plan`, **before** dependency analysis and expiration, using `Context::get_cwd_key()`
as the entry. That placement matters twice over: it is after the recipe CWD prefix and any recipe
overrides are in place, and it is before anything reads the plan's dependencies.

`finalize_plan` is called between `recipe.to_plan()` and `apply_plan()` in
`GenericEnvironment::apply_recipe` (every built-in environment is an alias of it), and is part of the
contract for any custom implementation, so freezing needs no second contract to remember.

Freezing is never folded into `build()`. Plans built for analysis (`Query::is_volatile`,
`requires_payload`) have no `Context`, hence no entry key, and the validation CLI has no environment
either; freezing those against a defaulted root would silently anchor operands that will later run
somewhere else.

### Mechanics

| Element | Rule |
|---|---|
| Key-bearing steps (including `UseKeyValue`) | Resolved against the cursor |
| `Step::SetCwd` | Advances the cursor **and** rewrites its own operand; kept afterwards as provenance |
| `Step::Evaluate`, `Step::UseQueryValue` | Resolved as scoped queries |
| Action link parameters | Resolved against a **clone** of the cursor, so a link's own `-R-cwd` cannot move the enclosing plan |
| Nested `Step::Plan` | **Shares** the cursor, so its final key reaches later outer steps |
| Absolute query's own resource step | Resolved against logical root; its index is read once, before any rewriting |
| `Plan::predecessor`, each `Step::Action::query` | Resolved against the cursor after the prologue (a recipe's `SetCwd`), so each names the asset a cut there would evaluate |
| `Filename`, `Info`, `Warning`, `Error` | Untouched |

The `Step` match is exhaustive with no default arm, so a new step variant is a compile error here
rather than a silently unfrozen operand.

Two further properties are contractual:

- **Idempotent.** Freezing an already-frozen plan against the same key is a no-op, because a
  non-relative key is returned unchanged. Freezing against a *different* key is an error: it means a
  caller reused a finalized plan under another CWD, which `finalize_plan` already forbids. Rebuild
  from the source query or recipe instead.
- **Diagnostics are not scoped like keys.** A link scope protects the working key but not the
  root-fallback flag, which describes the resolution as a whole. The caller owns the single warning
  that follows.

## Predecessor boundaries

### Cutting, and how it differs from freezing

Freezing decides *what operands mean*. Cutting decides *where work happens*.

`Plan::cut_predecessor(&mut self, cmr) -> Result<bool, Error>` replaces the steps of the chosen
candidate — the recorded predecessor, or a shorter prefix the walk steps back to — with one
`Step::Evaluate(<candidate>)` boundary, keeping any `Step::SetCwd` among them and leaving the
trailing action and filename in the parent. The predecessor then becomes an asset in its own right
instead of steps inlined into its consumer.

Cutting **requires a frozen plan** ("Plan must be frozen before its predecessor can be cut"). Cutting
an unfrozen one would produce a boundary query that still depended on a working key — a query that
cannot identify the asset it names, which is the defect freezing exists to remove.

Volatility, payload requirement, expiration and dependencies are *not* recomputed by the cut. They
were computed over the fully expanded plan, which is precisely why the builder always expands and
the cut happens afterwards.

### Why plans are cut by default

Cutting makes each intermediate a first-class asset, which gives it:

- **Dependency management.** A boundary is a real dependency edge. The intermediate gets its own
  version, its own dependents, and participates in cycle detection and invalidation instead of being
  invisible inside its consumer.
- **Caching and independent expiration.** Two queries sharing a prefix share the computation rather
  than repeating it, and an intermediate expires on its own schedule rather than forcing its consumer
  to recompute wholesale.
- **Parallel execution.** A dependency can be scheduled alongside its siblings — today the pre-pass
  schedules keyed dependencies (keys and key links) only; a boundary query is evaluated when its step
  runs. An inlined predecessor is necessarily sequential — it runs where it sits.

`finalize_plan` cuts, after freezing and after the analysis passes, unless `assets.cut_predecessors`
is `false` or an input state is supplied (condition 3 below).

**Cutting is recursive.** A boundary is evaluated as its own query asset, whose plan is finalized —
and cut — in turn, so a chain `a/b/c/d` becomes the assets `a/b/c`, `a/b` and `a`, each registered
under the default strategy. A long chain over a large value therefore keeps one copy per step unless
something limits it.

#### What is kept

Two instruments limit it, and neither changes the plan's value
([`design/plan-policy/`](../design/plan-policy/)):

- **A command's `cached: false`** says its output is cheap to recompute and not worth keeping. The
  walk steps back past a candidate ending with it (condition 4 below), so the command runs **inline**
  after the nearest kept prefix: `a/b/c/d` with `c` uncached is `Evaluate(a/b) c d`.
- **A caching strategy** (`none` \| `result` \| `all`) decides which *new* assets are registered when
  a boundary executes. It is a property of each asset: a recipe takes its `cached:` or the manager's
  `recipe_cache_strategy`, a top-level query takes `query_cache_strategy`, and a boundary, a link or
  a `context.evaluate` takes **its creator's**. An asset that already exists is reused under every
  strategy. The rules are in
  [`ASSETS.md`](ASSETS.md#when-an-asset-is-kept-for-reuse) §When an asset is kept for reuse.

**The plan never reads a strategy, nor the manager's state.** It is a function of its query, the
command metadata and one switch: `assets.cut_predecessors: false` skips the cut for every plan (a
debugging aid, to check whether cutting changes a result). A size limit on the cache is not a plan
concern — `CORE-ASSET-GC`.

### Where a boundary goes

`Plan::cut_predecessor` cuts at the **outermost candidate prefix that can be cached**. Everything
below follows from one fact: *a boundary is a cache entry, keyed by its query.* Anything that feeds
the prefix but is **not** part of that key makes the entry unsound, and anything that makes the
entry worthless makes the boundary pointless.

There are exactly **four conditions**, and they differ in where the answer lives.

| # | Condition | Why it blocks a boundary | Where the answer lives |
|---|---|---|---|
| 1 | **Volatility** | A boundary recomputed on every request buys none of the three things above, and costs an extra asset and an extra hop | In the plan — per candidate, or whole-plan via `Plan::volatility_source` |
| 2 | **Payload requirement** | A payload is not part of a cache key, so a value computed from one must never sit behind a boundary | In the plan — per candidate, via `Plan::payload_required` |
| 3 | **Input state** | Likewise not part of a cache key. A boundary is evaluated as its own asset, starting from `State::new()`, so a prefix that consumes a caller's state would silently receive nothing | **Not in the plan at all** — only the caller knows it |
| 4 | **Not cached** | The candidate's last command declares `cached: false`: its output is not worth an entry, so it runs inline instead | In the plan — per candidate, via `Plan::uncached_by` |

Conditions 1, 2 and 4 are decided **per candidate**, by building that candidate's own plan.
Condition 3 is decided **per application**, by the caller.

Candidates are built fresh, without dependency analysis. Volatility a plan inherits at finalization
from an upstream recipe (`volatile: true`, or an `immediately` expiry) sets `is_volatile` but no
`volatility_source`, and appears in no candidate's query. The dependency analysis therefore also
reports the first step that reads such a key, and `finalize_plan` cuts through
`cut_predecessor_bounded`, which steps back over every candidate containing that step ("it reads a
volatile dependency"). `-R/vol.txt/-/upper/lower` with a volatile `vol.txt` is not cut at all; in
`-R/stable.txt/-/upper/join-~X~-R/vol.txt~E/lower`, where `join` reads the link, the pure
`-R/stable.txt/-/upper` is still cut. The public `Plan::cut_predecessor` has no such bound.

A boundary is also declined where it would be worthless: when the prefix, apart from `SetCwd`, is a
single step reading a key (`GetAsset`, `GetAssetBinary`, `GetAssetMetadata`, `GetAssetRecipe`,
`GetAssetDirectory`, `GetResource`, `GetResourceMetadata`, `GetResourceDirectory`), with any
namespace declarations. The asset at that key is already cached under the key, so a boundary would
only add a keyless query asset — and hand the next command that asset's state instead of the keyed
asset's own. `-R/data/x.manifest.yaml/-/ns-rec/materialize` therefore stays
`GetAsset(data/x.manifest.yaml), Action(rec/materialize)`; the decline is recorded as a planning
`Info`.

#### 1 and 2 — per candidate

`Plan::payload_required` and `Plan::is_volatile` answer "does this query need a payload / is it
volatile *anywhere*", which is the wrong question in both directions. Used as a veto, it throws away
the boundary in `fetch/expensive/render_with_payload`, where everything behind the only candidate is
clean; used as a permit, it cuts straight across `fetch/personalize/render`. So each candidate's own
plan is consulted, and the walk stops at the first that qualifies:

```
fetch/expensive/render          -> boundary at fetch/expensive
fetch/personalize/render        -> personalize requires a payload; boundary at fetch
fetch/vol_step/render           -> vol_step is volatile;          boundary at fetch
fetch/expensive/wide/render     -> wide declares cached: false;   boundary at fetch/expensive
fetch/expensive/v/render        -> volatile from v onward;        boundary at fetch/expensive
personalize/fetch/render        -> the condition reaches the head; no boundary
```

#### 3 — per application, and why it needs an expanded plan

`AssetManager::apply` and `apply_immediately` hand a caller's state to `apply_plan`. If the prefix
that consumes it has been moved behind a boundary, the boundary runs as a separate asset from
`State::new()` and the state is lost:

```
apply "wrap/wrap" to "x"    expanded -> [[x]]
                            cut      -> [[None]]
```

Forwarding the state into the boundary would not fix it — it would make it worse. The boundary is
cached by its query, so two callers applying different states to the same prefix would share one
entry. **A stateful application therefore requires a fully expanded plan.**

Because only the caller knows the state, `finalize_plan` takes it and decides:

```rust
finalize_plan(envref, &mut plan, &context, &input_state).await?;
```

- `input_state.is_none()` → the plan is cut where conditions 1, 2 and 4 allow.
- otherwise → the plan is left **fully expanded**, with a `Step::Info` recording why.

##### Obtaining a fully expanded plan

Three ways, in the order you are likely to want them:

1. **Apply to a state.** Pass the state to `finalize_plan`, as above. The plan is expanded
   automatically; this is the normal path and needs nothing special.
2. **Ask for it.** `finalize_plan_expanded(envref, &mut plan, &context)` runs the same dependency,
   volatility and expiration analysis and the same freezing, and never cuts. Use it when the plan is
   for *reading* rather than executing — explanation, analysis, a diff of what a query means — and
   when a comparison must not be derived from the cutting path (an oracle built from that path cannot
   detect that path regressing).
3. **Do not finalize at all.** `PlanBuilder::build` alone always expands; the cut is a later pass.
   `liquers-validate` relies on this, which is why query validation shows the expanded form whatever
   the evaluation default is.

**Whole-plan volatility declines before the walk starts.** `Plan::volatility_source` distinguishes
the two kinds:

| Source | Means | Effect on a boundary |
|---|---|---|
| `Positional` | A volatile command, a link to a volatile query, or the `v` instruction. Volatility starts at that point, so everything ahead of it is pure. | A boundary may be cut in front of it. |
| `Declared` | A recipe's `volatile: true`, or a recipe `expires:` that is itself volatile. A statement about the whole plan, carrying no position. | Nothing here is cacheable; the plan is not cut at all. |

A `Declared` source appears in no candidate's query, so the walk could not see it — which is why
`Recipe::to_plan` records it and the check comes first.

Every level the walk passes over appends a planning `Step::Info`, and so does each explicit decline,
so a plan that was not cut is distinguishable from one that had no predecessor:

```
Predecessor boundary expanded at 'fetch/personalize': it requires an evaluation payload
Predecessor boundary expanded at 'fetch/vol_step': it is volatile
Predecessor boundary expanded at 'fetch/expensive/wide': it is not cached (command '/root/wide' declares cached: false)
Predecessor boundary not cut at '-R/data/x.manifest.yaml': the prefix only reads a key
Predecessor boundary not cut: the plan is declared volatile, so none of it may be cached
Predecessor boundary not cut: cut_predecessors is false
Predecessor boundary not cut: the plan is applied to an input state, which is not part of a boundary's cache key
```

A walk that reaches a candidate with no inner predecessor (`personalize/fetch/render`) simply stops,
with no decline line of its own.

These are planning diagnostics in `Plan::init_steps`. Applying a plan appends them to the evaluating
asset's log once, before its first step runs — so a rejected asset still records how it was planned
— alongside the asset manager's own "Not cached for reuse: …" line. They are appended without a
metadata save of their own and written with the asset's ordinary save.

Two candidates are never chosen: one whose remainder is a trailing filename rather than an action —
cutting there would leave the parent nothing but a `Filename` step, and a recipe's overrides nothing
to patch — and, in a **stable** plan, one covering every step, which would replace the whole plan
with a boundary that recomputes it. In a volatile plan that candidate is right: only a trailing `v`
reaches it, and `Evaluate(a/b)` then names a different, cacheable asset.

### Pitfalls

Each row is an invariant the code relies on; the named test or field pins it.

| Pitfall | What goes wrong |
|---|---|
| A trailing filename is not an action | `Query::predecessor` splits a filename off as the remainder. Recording the predecessor at every recursion level lets the outermost overwrite the inner one with the whole action chain, so a cut swallows the last action and a recipe's overrides have nothing to patch. Record only when the remainder is a real action. |
| A step-range recorded before a prefix is inserted | `Recipe::to_plan` inserts `SetCwd` at index 0 *after* building. A stale `predecessor_steps` then splits in the wrong place and keeps the predecessor's own action, so it runs twice — once in the boundary asset, once inline. |
| A default link is invisible to the cache key | A default lives in command metadata, not query text. An absolute default is reproduced by metadata everywhere, but a **relative** one resolves differently per directory, so it must be promoted into the query. Promotion appends, which is only correct when every earlier argument slot is already written; at a gap it must be skipped rather than bound to the wrong argument. |
| A boundary hides the diagnosis | A dependency failure reported as "did not produce a value" discards the cause, which then lives only in the sub-asset's log. The dependent must surface the cause itself — and must not re-wrap an error that already carries its command name and position. |
| An undeclared payload | A command reading the payload without `payload: required` works inlined and silently receives none across a boundary. This is the documented "declare it, or lose it" rule, not a cutting defect, but a cut is where it first bites. `injected` does **not** imply the requirement: injection may be satisfied from the environment alone. |
| A boundary query frozen before the prologue | Sibling of the row above it, and the same prepended `SetCwd`. If only the step *count* is compensated and not the *cursor*, the recorded predecessor resolves against the entry CWD and the boundary query — the only thing a cut carries — loses its folder, silently producing a wrong value as readily as a `KeyNotFound`. `Plan::prologue_steps` prevents it. |
| A recipe-level flag is not in the query | `volatile:` and `expires:` live in the `recipes.yaml` entry, not in the query text, so unlike a volatile *command* they do not travel into a boundary; a cached boundary would make the prefix of a `volatile: true` recipe run once across two evaluations. `to_plan` folds them onto the plan as `VolatilitySource::Declared`. |
| A cut swallows the caller's input state | `apply` and `apply_immediately` supply a state; a boundary runs as its own asset from `State::new()`, so `wrap/wrap` applied to `"x"` would yield `[[None]]`. Forwarding the state would be worse — the boundary is cached by query, so callers with different states would share one entry. A stateful application needs a fully expanded plan, which `finalize_plan` produces when it is given one. |
| `v` emits no step | `a/b` and `a/b/v` report the same step count, and in `a/b/v` the outermost non-volatile prefix is the *entire* plan. Since `v` is positional both are reachable, and both are handled: the walk's step-count check compares a candidate with the cut index it would produce, which is the same for `a/b/v` and `a/b`, and the empty-tail guard admits equality only for a volatile plan. Pinned by `positional_v_cuts_before_itself` and `trailing_v_cuts_the_whole_prefix`. |
| A `bool` that defaults to `true` on a `Default` type | `CommandMetadata` and `AssetManagerOptions` implement `Default`, so a plain `cached: bool` would read "not cached" and a `cut_predecessors: bool` "never cut" for every value built in code. Both are `Option<bool>` read through an accessor; `default_options_cut_predecessors` pins the second. |

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
| `dependencies` | Static **direct** dependencies discovered during analysis: a keyed read ends the list, `Evaluate` and nested plans pass through ([`DEPENDENCIES_STATUS.md`](DEPENDENCIES_STATUS.md#what-a-dependency-record-holds)). An action contributes its command's metadata and implementation keys, plus the alias's metadata key when its `origin` is an alias |
| `frozen_cwd` | The working key this plan was frozen against, once frozen |
| `predecessor`, `predecessor_steps` | The predecessor the builder recorded, and the number of leading steps it occupies (after a cut: the kept `SetCwd`s plus the `Evaluate`) |
| `prologue_steps` | Leading steps not emitted by the builder for `query` — a recipe's CWD prefix |
| `volatility_source` | Whether volatility permits a boundary in front of it (`Positional`) or forbids one anywhere (`Declared`) |
| `uncached_by` | The command whose `cached: false` makes the last action's output not worth keeping; read by the cut walk and by the asset manager before registering a non-keyed query |

Each `Step::Action` also records `query`: the prefix of the plan's query that ends with it — the
query a boundary cut right after it would evaluate. It is built with relative default links promoted
and frozen with the same cursor as `predecessor`; an alias keeps the name as written. `None` for a
hand-built step and for the action a recipe override patches. Omitted from the serialized plan when
`None`, so plans written before the field existed load unchanged.

`apply_plan` does not execute `init_steps`; it appends them to the evaluating asset's log once,
before the first step (and before the payload check), without a save of their own.
`Plan::to_metadata_record` and `Metadata::from_plan` project them the same way for callers building
metadata from a plan. `Step::Error` in `steps` logs through `Context::error`, so the plan stops with
an `ExecutionError` carrying the first error's message. A run of consecutive `Step::Info`,
`Step::Warning` and `Step::Error` steps is logged in full first: the plan stops after the last of
them, before the next action or other step. An error-level entry in `init_steps` fails the plan the
same way, before any dependency is scheduled. `Plan::error` is the structured planning failure
channel.

`apply_plan` runs, in order:

1. resolves the rooted query's own resource step against the logical root;
2. appends `init_steps` to the asset's log, and stops with an `ExecutionError` if one of them is
   error-level;
3. if `plan.is_volatile`, marks the context volatile ("its plan is volatile");
4. if a payload is required, records that on the context and rejects a context with no payload
   ("Query '…' requires an evaluation payload, but the evaluation was started without one…");
5. pre-schedules known keyed dependencies so they can start concurrently, then drains the local
   queue once;
6. interprets the steps in order, stopping with an `ExecutionError` after a step (or a run of
   consecutive diagnostic steps) once an error has been logged.

Each step produces the state handed to the next (`interpreter::do_step_state`): a fetch hands on the
fetched state — a boundary hands on its asset's state unchanged — steps that produce no value hand on
their input, and an action's result carries the asset's record corrected to describe the prefix it
completes. The full table, and `is_applied` for a plan applied to an input state, are in
[Environment, context and evaluation §Metadata ownership during evaluation](api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md#metadata-ownership-during-evaluation).

Every key-bearing executable step and every query/link operand is resolved when it is analyzed or
consumed. `SetCwd` resolves and installs its operand before later steps. Nested plans share the live
CWD and can change it for following outer steps; linked queries inherit an entry snapshot but keep
their own internal CWD changes scoped. An absolute source query is aligned with its own generated
resource step, so a recipe prefix is not mistaken for that source step and does not override the
outer query's logical-root meaning.

Static dependency discovery, pre-scheduling, runtime lookup, cycle registration, and asset caching
use the same resolved key identity. Planning cursors operate on copies and do not mutate the raw plan
or prematurely advance the live context.

## Finalization and expiration

Synchronous build results are incomplete for environment-backed dependencies.
`interpreter::finalize_plan`:

0. Freezes the plan against `Context::get_cwd_key()` (§Freezing). When the root fallback was used, it
   logs the relative-without-CWD warning once.
1. Analyses dependencies once (`analyze_plan_dependencies`): one iterative, memoized walk over the
   reachable recipe graph, each recipe once per (key, caller CWD), with an O(1) cycle check. It
   records the direct dependencies in `plan.dependencies` and appends a "Dependency detected: …"
   `Info` for each. A recipe lookup error during the walk is treated as "no recipe". Cycles are
   detected per path, set as `Plan::error` ("Circular dependency detected: key … appears in
   dependency chain") and returned, so finalization fails; a link to a key already on the path is
   skipped.
2. Incorporates upstream declared volatility (a recipe with `volatile: true`): through state reads at
   any depth, through a link one level only.
3. Incorporates upstream recipe expiration, combined along every path.
4. When the plan is not volatile: seeds the context's dependency records with the direct dependencies
   at their current versions (`Version::unknown()` when the dependency manager has none), and, when
   the context has an owner key, registers the dependency edges.
5. Cuts the predecessor boundary, or records why it did not (§Predecessor boundaries).

`finalize_plan_expanded` stops after 4.

`GenericEnvironment::apply_recipe` then sets `plan.expires` (which already includes the recipe's own
`expires`) on the context and calls `apply_plan`. This is the only place a plan's expiration reaches
the asset: neither `finalize_plan` nor `apply_plan` sets it, so a custom implementation must do the
same
([`PLAN-EXPIRATION-REACHES-THE-CONTEXT-ONLY-THROUGH-APPLY-RECIPE`](../issues/PLAN-EXPIRATION-REACHES-THE-CONTEXT-ONLY-THROUGH-APPLY-RECIPE.md)).

Keyed recipes cannot require an evaluation payload: keys identify globally shared assets, while a
payload belongs to one evaluation (`Recipe::to_plan_for_key`). Payload-required nested queries
instead inherit the current context payload and execute as volatile, unshared inline assets.

`interpreter::make_plan` is the dependency-aware helper for an ad-hoc query. It has no `Context`, so
it performs volatility and expiration analysis without freezing, cutting, context seeding or
dependency registration: its plan is expanded and source-relative. The asset manager uses it to
decide a query's volatility and registration. The legacy free function `interpreter::evaluate`
executes such a plan directly and bypasses finalization.

## Serialization and future plan rewriting

Recipe and plan JSON/YAML preserve source-relative query text, ordered raw `SetCwd` steps, links,
`QuerySource`, and source positions. `Step::Action.origin` is omitted when `Direct` and defaults to
`Direct` when absent, so plans without aliases serialize as before and older plans load. Runtime-only
cursor state and root-fallback bookkeeping are not serialized. This is a current data contract, not
a versioned stable wire-format guarantee
([`PLAN-SERDE-HAS-NO-COMPATIBILITY-CONTRACT`](../issues/PLAN-SERDE-HAS-NO-COMPATIBILITY-CONTRACT.md)).

`query`, `steps` and `is_volatile` are required. Every other `Plan` field — `init_steps`,
`payload_required`, `expires`, `error`, `dependencies`, `frozen_cwd`, `predecessor`,
`predecessor_steps`, `prologue_steps`, `volatility_source`, `uncached_by` — is `serde(default)`, so a
plan serialized before freezing existed still deserializes and reads as *not frozen* rather than as
frozen against the root.

A plan serialized **after** `finalize_plan` carries absolute operands and is therefore specific to
the CWD it was frozen against. This is not a new restriction: `finalize_plan` already forbids
re-finalizing a plan under another CWD, and callers rebuild from the source query or recipe. Freezing
makes that visible in the data rather than implicit in the contract.

There is still no plan optimizer or substitution pass. Freezing deliberately keeps `SetCwd` steps:
removing them is an optimizer's job, and because callers can inspect and serialize `Plan::steps` and
`init_steps`, such a pass must treat their ordering and diagnostics as observable and preserve
execution, dependency identity, source provenance, and the recipe CWD `Info` diagnostic.

## Public versus framework APIs

Preferred application-facing APIs:

- `Recipe::new`, its builder-style overrides, and read-only derived information
- `RecipeList` for `recipes.yaml`
- Environment recipe-provider configuration: `RecipeProviderChoice`, and the `recipes:` and
  `assets:` keys of `EnvironmentConfig`
- `EnvRef::evaluate` and `evaluate_immediately`
- `interpreter::make_plan` for diagnostic inspection

Framework extension APIs:

- Implementing `AsyncRecipeProvider`; `RecipeProviderChain`, and
  `with_appended_recipe_provider` on `EnvironmentBuilder` and `GenericEnvironment`
- Direct `PlanBuilder` policy configuration
- `Recipe::to_plan`, `Recipe::to_plan_for_key`, `finalize_plan`, `finalize_plan_expanded`, and
  `apply_plan`
- `Plan::freeze_cwd` and `Plan::cut_predecessor`
- Manual `Plan`, `Step`, and `ParameterValue` construction
- Plan splitting and metadata projection
- Legacy: `interpreter::evaluate`, which bypasses finalization

`Context::get_cwd_key`, `IsVolatile` and `RequiresPayload` are crate-private. Public visibility does
not otherwise enforce this distinction.

## Known limitations

- Complete recipe application is a convention: `to_plan`, `finalize_plan`, setting the expiration and
  `apply_plan` must be sequenced by each environment —
  [`PLAN-EXPIRATION-REACHES-THE-CONTEXT-ONLY-THROUGH-APPLY-RECIPE`](../issues/PLAN-EXPIRATION-REACHES-THE-CONTEXT-ONLY-THROUGH-APPLY-RECIPE.md).
- `RecipeList::set_cwd` can mutate part of a list before failing —
  [`RECIPE-LIST-SET-CWD-MUTATES-BEFORE-FAILING`](../issues/RECIPE-LIST-SET-CWD-MUTATES-BEFORE-FAILING.md).
- Provider plan APIs (`recipe_plan`, the private `create_plan_with_init_metadata` used by
  `recipe_plan` and `get_asset_info`, `make_plan`, `finalize_plan`) perform different subsets of
  analysis —
  [`RECIPE-PLAN-ANALYSIS-RUNS-OUTSIDE-PLAN-BUILDING`](../issues/RECIPE-PLAN-ANALYSIS-RUNS-OUTSIDE-PLAN-BUILDING.md).
- Recipe overrides reach only the last action —
  [`RECIPE-OVERRIDE-ONLY-LAST-ACTION`](../issues/RECIPE-OVERRIDE-ONLY-LAST-ACTION.md).
- A serialized `Plan` has no compatibility contract —
  [`PLAN-SERDE-HAS-NO-COMPATIBILITY-CONTRACT`](../issues/PLAN-SERDE-HAS-NO-COMPATIBILITY-CONTRACT.md).
- The recipe circular-dependency fields are never set or read —
  [`RECIPE-CIRCULAR-DEPENDENCY-FIELDS-ARE-DEAD`](../issues/RECIPE-CIRCULAR-DEPENDENCY-FIELDS-ARE-DEAD.md).
- Every `recipes.yaml` read error reads as "no recipes" —
  [`RECIPES-YAML-READ-ERRORS-READ-AS-NO-RECIPES`](../issues/RECIPES-YAML-READ-ERRORS-READ-AS-NO-RECIPES.md).
- A mid-query `q` silently drops its parameters —
  [`MID-QUERY-Q-INSTRUCTION-DISCARDS-ARGUMENTS`](../issues/MID-QUERY-Q-INSTRUCTION-DISCARDS-ARGUMENTS.md).
- `DefaultRecipeProvider` skips a recipe with no valid filename in listings without a diagnostic —
  [`RECIPE-WITHOUT-FILENAME-SILENTLY-OMITTED-FROM-LISTING`](../issues/RECIPE-WITHOUT-FILENAME-SILENTLY-OMITTED-FROM-LISTING.md).
- The explicit-`cwd` error names the directory but not the offending recipe —
  [`RECIPE-CWD-ERROR-DOES-NOT-NAME-THE-RECIPE`](../issues/RECIPE-CWD-ERROR-DOES-NOT-NAME-THE-RECIPE.md).

## Tests that pin this contract

- `liquers-core/src/recipes.rs`: `recipe_to_plan_preserves_programmatic_cwd`,
  `recipe_prefix_info_is_exactly_once_and_precedes_query_steps`,
  `recipe_plan_round_trip_keeps_raw_operands_and_prefix`,
  `serialization_keeps_raw_cwd_links_positions_and_no_runtime_schema`, the `recipe_cache_*` tests and
  the provider chain tests.
- `liquers-core/src/plan.rs`: `positional_v_cuts_before_itself`, `trailing_v_cuts_the_whole_prefix`,
  `walk_steps_back_past_an_uncached_candidate`, the `test_q_instruction_*` tests,
  `special_instructions_enforce_their_own_arity`, `find_dependencies_nested_plan_propagates_cwd`.
- `liquers-core/src/environment_builder.rs`: `default_options_cut_predecessors`.
- [`liquers-core/tests/recipe_cwd_resolution.rs`](../../liquers-core/tests/recipe_cwd_resolution.rs).

## History

| Date | Change | Source |
|---|---|---|
| 2026-10-27 | §Where a boundary goes: inherited volatility bounds the cut `finalize_plan` makes, at the first step reading the volatile upstream; the Known limitations bullet is removed. | design `inherited-volatility-bounds-predecessor-cut` |
| 2026-10-22 | Carried over from DOC-08 on main: `Step::Error` and an error-level `init_steps` entry fail the plan with an `ExecutionError`, after a run of consecutive diagnostic steps is logged in full; the `apply_plan` order says where. | `issues/CONTEXT-ERROR-OUTCOME-DEPENDS-ON-SCHEDULING.md` |
| 2026-10-20 | Promoted from `reference/api/DOC_08_RECIPES_PLANS.md` to `reference/RECIPES_AND_PLANS.md`; series wording, the dated verification log and the priority table removed (Known limitations link issues instead). Added §Planner instructions (instruction vs command; `ns`, `q` including mid-query, `v`, resource header instructions, segment headers, filenames), `to_plan_for_key`, placeholders, the `apply_plan` order, `uncached_by`, Serde defaults, a `recipes.yaml` example. Corrected: `init_steps` are logged by `apply_plan`; a `recipes.yaml` with an explicit `cwd` is refused whole; `set_cwd` writes to stderr; `to_plan` already folds `volatile`/`expires`; expiration reaches the context only in `apply_recipe`; seeding is conditional on non-volatility; circular-dependency fields are unused; the pre-pass schedules keyed dependencies only. (Earlier, until `plan-policy`, `v` anywhere marked the whole plan volatile.) | DOC-08 review, promotion |
| 2026-10-17 | Planning contract: resource steps (`-R-stored/`, `-R-stored_meta/`, `-R-sdir/`) never make a plan volatile; `Plan::is_volatile` is the only volatility authority (the step-level `IsVolatile` impl was removed). | `design/step-volatility-impl-removal/` |
| 2026-10-12 | Recipe contract: overrides apply in sorted order, resolve against `Plan::last_action_command` (an alias's own arguments; head slots refused), convert a mismatched JSON type with a planning warning or fail with `ConversionError`, and report an unknown or injected name as `ParameterError`. | phase-5, `design/axum-query-named-arguments/` |
| 2026-10-10 | Predecessor boundaries: planning diagnostics (`init_steps`) now reach the evaluated asset's log, appended once when the plan is applied (`PLANNING-DIAGNOSTICS-NEVER-REACH-THE-ASSET-LOG`). | `issues/PLANNING-DIAGNOSTICS-NEVER-REACH-THE-ASSET-LOG.md` |
| 2026-10-10 | Reviewed against `design/plan-policy/`. Recipe contract: `cached` is a caching strategy (`none` \| `result` \| `all`, booleans read as `all`/`none`, absent or `default` = `recipe_cache_strategy`). Planning contract: `v` is positional (`a/b/v/c` cuts `a/b`, `a/b/v` is `Evaluate(a/b)`, `v/…` recomputes everything); `Plan::uncached_by`. Predecessor boundaries: cutting is recursive (corrects "one cut retains one intermediate"); new "What is kept" (the command flag, strategies following the origin, the plan independent of strategy and manager state, `cut_predecessors`); a fourth condition, not cached; `Declared` is recipe-level only; the stable-plan empty-tail rule; two pitfalls updated or added; planning diagnostics do not reach the asset log (`PLANNING-DIAGNOSTICS-NEVER-REACH-THE-ASSET-LOG`). | phase-5, `design/plan-policy/` |
| 2026-10-10 | Reviewed against `design/plan-step-state-metadata/`. Recipe contract: `Recipe::data_format` is `None` without a filename (no `bin`); an override clears the patched action's prefix query. Predecessor boundaries: no boundary over a bare key read. Plan fields: `Step::Action::query`. Execution: each step produces the state it hands on (table in DOC-04); the `key`-only adjustment and `fetched_key` are gone. | phase-5, `design/plan-step-state-metadata/` |
| 2026-10-10 | Planning contract: aliases are resolved by the builder into a target `Step::Action` with `origin: ActionOrigin::Alias`; `dependencies` includes the alias's metadata key; `origin` serialization. | phase-5, `design/command-alias-contract/` |
| 2026-10-08 | §Plan fields: `dependencies` is the direct list. §Finalization: one analysis pass (`analyze_plan_dependencies`) replaces the volatility and expiration passes; `DefaultRecipeProvider` caches parsed `recipes.yaml` per directory, checked against the stored bytes. | phase-5 (`design/dependency-chain-analysis-cost/`) |
| 2026-10-06 | §Provider contract: `contains` (listed) vs `can_make` (producible), `directory_changed`; chain table; manifest provider lists explicit chunks only. | phase-5 |
| 2026-09-27 | Reviewed against `design/record-streams/` Phase 5. Recipe contract gains `stored` and `cached`; added §Composing providers: `RecipeProviderChain` (delegation table, `contains` through `recipe_opt`), `with_appended_recipe_provider`, and `liquers-lib`'s `[DefaultRecipeProvider, ManifestRecipeProvider]` default; `has_recipes` answers `false` for a store's `KeyNotSupported`; plan execution records that the next step's state carries the fetched key as metadata `key`. | phase-5 |
| 2026-08-29 | Documented `RecipeProviderChoice`: the named selection of the two built-in providers, the `trivial` aliases `none` and `no_recipes`, the document default, and why the set is closed. | RECIPE-PROVIDER-BY-NAME |
| 2026-08-26 | Cutting at the outermost cacheable predecessor is now the **default**. Added "Where a boundary goes" — the three conditions (volatility, payload, input state), which are per candidate and which per application, and how to obtain a fully expanded plan. Superseded the paragraph deferring that decision; five new pitfall rows; `frozen_cwd`, `predecessor`, `prologue_steps` and `volatility_source` in the plan fields; a paragraph on `v`'s whole-plan scope. | PREDECESSOR-CUT-EQUIVALENCE |
| 2026-08-16 | Documented freezing — what it is, the three-cursor problem it solves, when it runs, its mechanics and scope rules — and predecessor boundaries: how cutting differs from freezing, the dependency, caching and parallelism case for making a predecessor available, and five observed pitfalls. Removed `disable_expand_predecessors` from the planning contract. | PLAN-CWD-FREEZE |
| 2026-08-11 | Documented provider and programmatic recipe CWD provenance, raw plan prefixes and diagnostics, ordered runtime resolution, serialization, identity, and optimizer constraints. | phase-5 |
| 2026-08-09 | Applied the verified recipe and planning contracts to comprehensive module and public-API Rustdoc in `plan.rs` and `recipes.rs`. | DOC-08 |
| 2026-08-09 | Reviewed recipe resolution, plan building, payload requirements, finalization, and execution against HEAD; documented `Plan::payload_required` and corrected links. | PAYLOAD-INHERITANCE |
| 2026-07-29 | Verified recipe resolution, planning, finalization, and execution against the implementation and focused tests. | DOC-08 |

---
title: Environment Configuration
kind: reference
audience: both
area: [core/context, core/store, core/assets]
reviewed: 2026-11-01
---
# Environment Configuration

`EnvironmentConfig` (`liquers-core/src/environment_config.rs`) describes an environment's services
in one serde document, so an application or a language binding can be set up from a file rather
than from Rust. It embeds `StoreRouterConfig` verbatim, so one document configures the environment
**and** its store.

For the task-oriented walkthrough see
[Building and Configuring an Environment](../guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md); this page is
the field-by-field reference.

## Scope: services, not commands

A configuration configures *services*. Commands are Rust functions registered by a macro and no
document can name one, so command registration stays in code. `EnvironmentBuilder` splits along
exactly that line — the `with_*` setters are the config-drivable half, the public
`command_registry` field is the code-only half.

## Format

```yaml
store:                          # StoreRouterConfig, verbatim
  stores:
    - type: filesystem
      prefix: data
      config:
        path: ${LIQUERS_DATA}
    - type: memory
      prefix: tmp
recipes:                        # absent / default | trivial | a list
  - recipes_yaml
  - manifest                    # liquers-lib with records
assets:
  job_capacity: 8               # queued managers only
  dependency_audit: on_load     # explicit | on_load
  verify_versions: on_read      # off | on_read
  external_change: user_input   # user_input | corrupted
  recipe_cache_strategy: all    # none | result | all
  query_cache_strategy: all     # none | result | all
  cut_predecessors: true        # false: never cut (debugging)
```

| Field | Type | Default when absent | Meaning |
|---|---|---|---|
| `store` | `StoreRouterConfig` | empty router | Store list and routing prefixes. See [Store Configuration](./STORE_CONFIG_FSD.md); the format is not restated here. |
| `recipes` | `RecipesConfig` | **`default`** | The recipe provider chain, as an ordered list of entries — see §The recipe provider chain. `default` is the manager kind's default list; `trivial` (aliases `none`, `no_recipes`) is the empty list `[]`. |
| `assets` | `AssetManagerOptions` | all unset | Per-manager settings, below. |

`assets` (`AssetManagerOptions`, `liquers-core/src/environment_builder.rs`). A field at its default
is omitted when the options are serialized.

| Key | Values | Default | Meaning |
|---|---|---|---|
| `job_capacity` | integer | the manager's own (4) | Queued manager's job-queue size; **must be at least 1**. |
| `dependency_audit` | `explicit` \| `on_load` | `explicit` | How much a restarted process checks before serving a stored value. `explicit` (trusting): a recorded dependency is compared with what the dependency manager already knows, and an unknown one is trusted; deeper checks run only when an audit is called (`trigger_dependency_audit*`, including the startup store audit `trigger_dependency_audit_store`). `on_load` (conservative): a recorded dependency the manager does not know is resolved from the store, together with everything it recorded, recursively; a mismatch, a stale upstream or a missing intermediate refuses the stored copy, so it is recomputed. See [Dependencies status §Consistency policies](./DEPENDENCIES_STATUS.md#consistency-policies) and [the consistency guide](../guides/DEPENDENCY_CONSISTENCY_GUIDE.md). |
| `verify_versions` | `off` \| `on_read` | `on_read` | Whether stored bytes are re-hashed against their recorded version where the manager already reads them (fast track, `*_any_status`, `verify_stored_versions`). `off` never hashes, so outside edits go unnoticed. |
| `external_change` | `user_input` \| `corrupted` | `user_input` | What a mismatch on a **recipe-backed** stored value means: `user_input` turns it into an `Override`; `corrupted` deletes the stored copy so the recipe recomputes it. A `Source` or `Override` is always kept as input. See [ASSETS §Content changed outside Liquers](./ASSETS.md#content-changed-outside-liquers). |

| `recipe_cache_strategy` | `none` \| `result` \| `all` | `all` | How much a **recipe** evaluation keeps for reuse when the recipe's own `cached:` is absent or `default`. `all`: the keyed result and every intermediate (predecessor boundary, link) it creates are registered. `result`: only the keyed result; a missing intermediate is computed unregistered. `none`: nothing new is registered. An intermediate that already exists is reused under every strategy. |
| `query_cache_strategy` | `none` \| `result` \| `all` | `all` | The same, for a top-level **ad-hoc query** (no key). A query whose last command declares `cached: false` is never registered, whatever this says. |
| `cut_predecessors` | `true` \| `false` | `true` | `false` never cuts a predecessor boundary, so plans run fully expanded and reuse no intermediate. A debugging aid, to check whether cutting changes a result. |

The strategy is a property of each asset, recorded when it is created: a keyed asset takes its
recipe's `cached:` or `recipe_cache_strategy`, a top-level query takes `query_cache_strategy`, and
every non-keyed **dependency** an asset creates (a predecessor boundary, a link parameter, a
`context.evaluate`) takes **its creator's** strategy. So the two settings are independent: an
internet-facing service can keep anonymous queries out of the cache while the recipes — the
approved computations — cache their intermediates, and those queries still reuse them:

```yaml
assets:
  recipe_cache_strategy: all    # approved recipes cache results and intermediates
  query_cache_strategy: none    # ad-hoc queries add nothing, but reuse what recipes cached
```

The strategies decide **whether** a value is kept, never **how much** memory the cache may use — a
size limit belongs to `CORE-ASSET-GC`. They change no value and no plan: whether a boundary is
reused or registered is decided when it executes. See
[RECIPES_AND_PLANS §Predecessor boundaries](./RECIPES_AND_PLANS.md#predecessor-boundaries) and
[ASSETS §When an asset is kept for reuse](./ASSETS.md#when-an-asset-is-kept-for-reuse).

All six settings apply to both built-in kinds (`Queued`, `Inline`). In code they are
`AssetManagerOptions::with_dependency_audit`, `with_verify_versions`, `with_external_change`,
`with_recipe_cache_strategy`, `with_query_cache_strategy` and `with_cut_predecessors`, and a
manager reports them through the `AssetManager` accessors of the same names (the trait provides
defaults equal to today's behaviour, so a custom manager need not implement them).

Every field has a serde default, so a document may configure one section and omit the rest, and a
field added later does not break an existing document. Unknown keys are currently **ignored**
(`deny_unknown_fields` is not set).

## Constructors

| Method | Notes |
|---|---|
| `from_yaml`, `from_json` | Always available |
| `from_toml` | Behind the `toml` feature, matching `StoreRouterConfig` |
| `to_yaml`, `to_json` | Round-trip |
| `expand_env_vars` | Expands `${VAR}` in the store section; called by `build()` |

## Applying it

```rust,ignore
let config = EnvironmentConfig::from_yaml(&yaml)?;

let mut builder = EnvironmentBuilder::<Value>::new()
    .with_config(config, Box::new(default_store_factory()));
register_my_commands(&mut builder.command_registry)?;
let envref = builder.build()?;
```

`with_config` is equivalent to `with_store_config` + `with_recipes` +
`with_asset_manager_options`, and reads in the same direction as every other setter, so a document
and hand-written configuration compose in either order — document first then overridden in code, or
the reverse.

Store construction and `${VAR}` expansion are **deferred to `build()`**. That is what keeps the
setters infallible and chainable; three failures surface at `build()` instead:

| Failure | Error |
|---|---|
| Store type no factory in the chain claims | Names the type, and lists the types the chain supports |
| `${VAR}` referencing an unset variable | The expander has no default-value syntax, so this is an error rather than an empty value |
| Both a store and a store configuration supplied | Rejected rather than resolved by a silent precedence rule |
| A `recipes` entry type no factory declares, one unavailable in this build, or an entry listed twice with the same arguments | Names the entry and its position; an unknown type lists the known ones, an unavailable one says why |
| `assets.job_capacity: 0` | Rejected. The queue starts work only while `running_count < capacity`, so zero would accept every evaluation and run none — a hang with no error, which is strictly worse than a rejected configuration |
| `assets.job_capacity` set against an inline kind | Rejected. `Inline` has no job queue, and silently ignoring the setting would hide the mistake |

Use `with_store_config_unexpanded` where there are no environment variables to expand — a browser
page — mirroring `StoreRouterBuilder::build_without_env_expansion`.

## Two deliberate omissions

**The asset-manager kind is not a field.** A string cannot select a type: `"queued"` and
`"inline"` produce two different concrete environment types, and `Environment` is not object-safe
(associated types, `Sized`), so they cannot be erased behind a `dyn`. The choice is a *build* fact
rather than a deployment one — wasm has no choice at all, and natively `Inline` exists for
deterministic testing rather than production tuning. `DefaultKind` gets it right on both targets.
An application that genuinely wants runtime selection monomorphizes its own tail with an explicit
match.

**The store factories are not a field.** Which backends exist is a build fact for the same reason:
`liquers-core` supplies memory and filesystem, `liquers-store` chains OpenDAL onto them, and
`liquers-web` chains its own. The factory reaches the builder as an argument, and the document
names store *types* the chain is expected to resolve.

## The `recipes` default is not the builder's default

This is the one field whose absence changes behaviour in a way worth stating twice.

| Situation | Chain, `liquers-core` kinds | Chain, `liquers-lib`'s `LibKind` |
|---|---|---|
| `EnvironmentBuilder::new()` with no recipe configuration | `[]` — resolves no recipes | the kind's default list |
| No `recipes:` key, or `recipes: default`, applied | `[recipes_yaml]` | `[recipes_yaml, manifest]` with `records`, `[recipes_yaml]` without |
| `recipes: trivial` or `recipes: []` | `[]` | `[]` |

The *document* default, `RecipesConfig::KindDefault`, is chosen on the grounds that a configuration
saying nothing about recipes most plausibly wants them to work. A core builder has no opinion and
resolves nothing, so applying even an empty configuration to it is an explicit act that changes how
`-R/` queries resolve. Pinned by
`environment_config::tests::an_absent_recipes_key_means_default_not_trivial`. Written back, the
kind default omits the key, so it reads back as itself, and `[]` stays `[]`.

## The recipe provider chain

Every environment's recipe provider is a `RecipeProviderChain` (`liquers-core/src/recipes.rs`),
configured by `recipes:` as an ordered list of **entries**
([`design/recipe-provider-chain-config/`](../design/recipe-provider-chain-config/)).

**Entries.** A bare name, or a map with `type:` and the entry type's own arguments. The two forms
are the same entry: `manifest` is `{type: manifest}`, and an entry without arguments is always
written back bare.

| Entry type | Where declared | Provider |
|---|---|---|
| `recipes_yaml` | `liquers-core` (`core_recipe_provider_factory`) | `DefaultRecipeProvider`: each folder's `recipes.yaml`, read through the store (a folder whose `recipes.yaml` key the store refuses as unsupported simply has none) |
| `manifest` | `liquers-lib` (`lib_recipe_provider_factory`) | `ManifestRecipeProvider`: chunk recipes from each folder's `*.manifest.yaml`. Declared unavailable without `liquers-lib`'s `records` feature |
| `default` | — | Not a provider: splices the kind's default list in place, so `[default, {type: reports, folder: reports}]` extends it with an integration's entry type |
| *integration types* | the integration (`EnvironmentBuilder::with_recipe_provider_factory`) | e.g. a Python factory registered with `Environment.register_recipe_provider_factory` (`reference/PYTHON_INTEGRATION.md` §10); the entry's other keys are its arguments |

`trivial` and its aliases are keywords for the whole value only; as a list entry they are refused.
An entry type may be listed several times with different arguments (one converter for `.png`,
one for `.jpg`); each entry is its own member. An entry **identical** to an earlier one — same
type, same arguments, directly or through `default` — is a configuration error, as is an
unknown type (the error names the entry, its position and the known types) or one unavailable in
the build (the error says why). These surface at `build()`.

**Factories.** Entry types are declared by `RecipeProviderFactory` implementations, modelled on
store factories: `RecipeProviderTypeInfo` (name, label, doc, arguments, availability),
`RecipeProviderTypeMap` for a set of named constructors, and `ChainedRecipeProviderFactory`, where an
earlier factory wins a name. A manager kind supplies its crate's
(`AssetManagerKind::recipe_provider_factory`) and its default list
(`AssetManagerKind::default_recipes`); an integration chains its own **before** them with
`EnvironmentBuilder::with_recipe_provider_factory`, so a document can place the integration's
provider anywhere in the list. `resolve_recipes(recipes, default_list, factory)` is the single
definition of a chain; the builder, `GenericEnvironment::try_with_recipes` and any validation tool
call it.

**Every member is asked.** A key is owned by the one member that has a recipe for it, whatever the
order of the list:

| Method | Behaviour |
|---|---|
| `recipe_opt`, `can_make` | the owner's answer. An error from **any** member fails the lookup, and two members with a recipe for the same key is a configuration error naming both (`Key … has a recipe from two recipe providers, 'recipes_yaml' and 'manifest'`) |
| `recipe`, `recipe_plan`, `get_asset_info` | delegated to the owner; with none, `No recipe found for key …` |
| `contains`, `has_recipes` | any member's |
| `assets_with_recipes` | the de-duplicated union, in list order |

So a recipe provider answers "no recipe" for a key it does not own and fails only when something
is substantially wrong (decided at the Phase 2 gate). The clash between a manifest chunk and a
sibling `recipes.yaml` entry is this rule; `ManifestRecipeProvider` no longer reads `recipes.yaml`
itself. Members are named by entry type — `type#position` when the type occurs more than once in the
chain (`convert#0`, `convert#2`) — or `#n` by position when supplied in code, so errors can say
which. An appended provider (`with_appended_recipe_provider`, `with_records_recipe_provider`) is
skipped when a member already has its entry type.

**Setters.** The last of the three base setters wins; appended items are kept.

| Method | Effect |
|---|---|
| `EnvironmentBuilder::with_recipes(RecipesConfig)` | The base is the configured list, resolved at `build()` |
| `EnvironmentBuilder::with_recipe_provider_choice(choice)` | `with_recipes(choice.into())`: `Default` is the kind's list, `Trivial` is `[]` |
| `EnvironmentBuilder::with_recipe_provider(provider)` | The base is one provider supplied in code (member `provider`) |
| `EnvironmentBuilder::with_appended_recipe_entry(entry)` | Appends an entry after the base, **skipped** when an entry of that name is already in the chain |
| `EnvironmentBuilder::with_appended_recipe_provider(provider)` | Appends a provider supplied in code (member `#n`) |
| `GenericEnvironment::try_with_recipes(RecipesConfig)` | On a constructed environment: installs the resolved list, or returns the error and changes nothing |
| `GenericEnvironment::with_recipe_provider_choice`, `with_default_recipe_provider`, `with_trivial_recipe_provider` | Infallible. Should the kind's own default list not resolve, the installed provider fails every lookup with that error rather than the setter panicking |
| `GenericEnvironment::with_appended_recipe_provider(provider)` | Wraps the current provider and the new one in a chain |

**`liquers-lib`.** `LibKind`'s default list is `[recipes_yaml, manifest]` with `records`, and an
unconfigured `default_environment_builder()` or `DefaultEnvironment::new()` resolves it. A
configuration naming `default` (or saying nothing) keeps manifests; an explicit list keeps them only
if it names `manifest`. `RecordsRecipeProvider::with_records_recipe_provider()` appends the
`manifest` entry and is idempotent:

```rust,ignore
use liquers_lib::environment::RecordsRecipeProvider;

let builder = liquers_lib::environment::default_environment_builder::<Value, ()>()
    .with_config(config, Box::new(default_store_factory()))
    .with_records_recipe_provider();   // a no-op when the list already has `manifest`
```

**Migration hazard.** Appending a raw `ManifestRecipeProvider` with `with_appended_recipe_provider`
after a list that already holds `manifest` makes every chunk a two-owner error. Use
`with_records_recipe_provider()` (or `with_appended_recipe_entry`) instead.

**Features unify.** `liquers-agent` builds `liquers-lib` without `records`, so on its own its
default is `[recipes_yaml]`; in a workspace build another crate's `records` feature is unified in
and the default gains `manifest`.

## Related

- Guide: [Building and Configuring an Environment](../guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md)
- Reference: [Store Configuration](./STORE_CONFIG_FSD.md)
- Reference: [DOC-04 Environment, Context and Evaluation](./api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md)
- Design: [`design/environment-builder/`](../design/environment-builder/)
- Design: [`design/recipe-provider-chain-config/`](../design/recipe-provider-chain-config/)

## History

| Date | Change | Source |
|---|---|---|
| 2026-11-01 | §The recipe provider chain: one entry type may repeat with different arguments, only an identical entry is refused; repeated types name their members `type#position`; integration entry types (Python) in the entries table, replacing the `{type: python, …}` placeholder. | `design/liquers-py-recipes/` (phase-5) |
| 2026-10-30 | `recipes` is a `RecipesConfig`: an ordered list of entries (bare name or `{type: …}`), `default` the kind's list, `trivial` the empty list. §The recipe provider chain rewritten: entry types and factories, `resolve_recipes`, every member asked (any error fails, two owners is an error naming both), the setters including `with_recipes`, `with_appended_recipe_entry` and `try_with_recipes`, `LibKind`'s list, the migration hazard. Build-time failures gain the entry errors. | phase-5 (`design/recipe-provider-chain-config/`) |
| 2026-10-20 | Links to `reference/api/DOC_08_RECIPES_PLANS.md` follow its promotion to `reference/RECIPES_AND_PLANS.md`. | DOC-08 promotion |
| 2026-10-10 | `assets` gains `recipe_cache_strategy`, `query_cache_strategy` (`none` \| `result` \| `all`) and `cut_predecessors`, with the origin rule (a dependency follows its creator), the public-service example, and the accessors. | phase-5 (`design/plan-policy/`) |
| 2026-10-08 | `dependency_audit` row: the two values described as the trusting and conservative policies; `on_load` resolves upstream recursively; the startup store audit named; links to §Consistency policies and the new guide. | phase-5 (`design/dependency-chain-analysis-cost/`) |
| 2026-10-02 | Reviewed against `design/dependency-audit-and-expiry-provenance/`. §Format gains the `assets` key table: `dependency_audit` (`explicit` \| `on_load`), `verify_versions` (`off` \| `on_read`) and `external_change` (`user_input` \| `corrupted`), with defaults and meaning, checked against `AssetManagerOptions`' serde names. | phase-5 |
| 2026-09-27 | Reviewed against `design/record-streams/` Phase 5. Added §The recipe provider chain: `RecipeProviderChain` and its delegation rules, `with_appended_recipe_provider` on the builder and on `GenericEnvironment`, `RecipeProviderChoice` unchanged and selecting only the base, `liquers-lib`'s `[DefaultRecipeProvider, ManifestRecipeProvider]` default with `records`, and `with_records_recipe_provider()` for a build that sets its own base. `recipes: default` answers "no recipes" for a folder the store refuses as unsupported. | phase-5 |
| 2026-08-31 | Created with `EnvironmentConfig`: fields, constructors, deferred failures, the two deliberate omissions, and the `recipes`-absent asymmetry. | `design/environment-builder/phase-5` |

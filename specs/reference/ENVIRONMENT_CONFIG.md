---
title: Environment Configuration
kind: reference
audience: both
area: [core/context, core/store, core/assets]
reviewed: 2026-10-10
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
recipes: default                # default | trivial
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
| `recipes` | `RecipeProviderChoice` | **`default`** | `default` reads recipes through the store (a folder whose `recipes.yaml` key the store refuses as unsupported simply has none); `trivial` resolves none. Aliases `none` and `no_recipes` are accepted for `trivial`. Selects the **base** provider only — see §The recipe provider chain. |
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
[DOC-08 §Predecessor boundaries](./api/DOC_08_RECIPES_PLANS.md) and
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

`with_config` is equivalent to `with_store_config` + `with_recipe_provider_choice` +
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

| Situation | Recipe provider |
|---|---|
| `EnvironmentBuilder::new()` with no configuration | `Trivial` — resolves no recipes |
| `EnvironmentConfig` with no `recipes:` key, applied | **`Default`** — reads recipes through the store |
| `liquers_lib::default_environment_builder()` | `Default`, followed by `ManifestRecipeProvider` when `liquers-lib`'s `records` feature is on (§The recipe provider chain) |

`RecipeProviderChoice`'s `#[default]` is the *document* default, chosen on the grounds that a
configuration saying nothing about recipes most plausibly wants them to work. `liquers-core`'s
builder has no opinion and resolves nothing. So applying even an empty configuration is an explicit
act that changes how `-R/` queries resolve. Pinned by
`environment_config::tests::an_absent_recipes_key_means_default_not_trivial`.

## The recipe provider chain

An environment has one recipe provider, and it may be a **chain**: `RecipeProviderChain`
(`liquers-core/src/recipes.rs`) consults its providers in order. `recipe_opt` answers from the
first provider that has the key; `contains` is true when any provider's `recipe_opt` answers;
`recipe`, `recipe_plan` and `get_asset_info` delegate whole to that provider; `has_recipes` is any,
and `assets_with_recipes` is the de-duplicated union in provider order.

A chain is built in code, not in the document. `RecipeProviderChoice` is unchanged — still
`default` or `trivial` — because a choice is data and cannot name a provider that lives in another
crate. Two setters append a provider after the one already there:

| Method | Effect |
|---|---|
| `EnvironmentBuilder::with_appended_recipe_provider(provider)` | Queued for `build()`, which composes `RecipeProviderChain::new([base, appended…])`. The base is the configured provider, or the kind's default when none is configured. A later `with_recipe_provider`, `with_recipe_provider_choice` or `with_config` replaces **only the base** and keeps every appended provider. |
| `GenericEnvironment::with_appended_recipe_provider(provider)` | On a constructed environment: replaces the provider with `RecipeProviderChain::new([current, provider])`. Calling it twice nests a chain in a chain, which behaves identically. |

**`liquers-lib`'s default is a chain when `records` is on.** `LibKind::default_recipe_provider` is
`[DefaultRecipeProvider, ManifestRecipeProvider]`, so a manifest's keyed chunks resolve from any
`-R/` query exactly as a `recipes.yaml` entry does, with `recipes.yaml` consulted first. Without
`records` it is `DefaultRecipeProvider` alone, as before.

That default is the *kind's*, so a build that **sets the base** — `with_recipe_provider`,
`with_recipe_provider_choice`, or `with_config` with any document — replaces the whole chain and
loses manifest support without a word. Such a build calls
`liquers_lib::environment::RecordsRecipeProvider::with_records_recipe_provider()`, which appends
`ManifestRecipeProvider` after whatever base is configured:

```rust,ignore
use liquers_lib::environment::RecordsRecipeProvider;

let builder = liquers_lib::environment::default_environment_builder::<Value, ()>()
    .with_config(config, Box::new(default_store_factory()))
    .with_records_recipe_provider();
```

Nothing in `liquers-lib` or `liquers-axum` constructs an environment from a configuration document
yet, so no built-in path calls it today; an application that does must.

## Related

- Guide: [Building and Configuring an Environment](../guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md)
- Reference: [Store Configuration](./STORE_CONFIG_FSD.md)
- Reference: [DOC-04 Environment, Context and Evaluation](./api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md)
- Design: [`design/environment-builder/`](../design/environment-builder/)

## History

| Date | Change | Source |
|---|---|---|
| 2026-10-10 | `assets` gains `recipe_cache_strategy`, `query_cache_strategy` (`none` \| `result` \| `all`) and `cut_predecessors`, with the origin rule (a dependency follows its creator), the public-service example, and the accessors. | phase-5 (`design/plan-policy/`) |
| 2026-10-08 | `dependency_audit` row: the two values described as the trusting and conservative policies; `on_load` resolves upstream recursively; the startup store audit named; links to §Consistency policies and the new guide. | phase-5 (`design/dependency-chain-analysis-cost/`) |
| 2026-10-02 | Reviewed against `design/dependency-audit-and-expiry-provenance/`. §Format gains the `assets` key table: `dependency_audit` (`explicit` \| `on_load`), `verify_versions` (`off` \| `on_read`) and `external_change` (`user_input` \| `corrupted`), with defaults and meaning, checked against `AssetManagerOptions`' serde names. | phase-5 |
| 2026-09-27 | Reviewed against `design/record-streams/` Phase 5. Added §The recipe provider chain: `RecipeProviderChain` and its delegation rules, `with_appended_recipe_provider` on the builder and on `GenericEnvironment`, `RecipeProviderChoice` unchanged and selecting only the base, `liquers-lib`'s `[DefaultRecipeProvider, ManifestRecipeProvider]` default with `records`, and `with_records_recipe_provider()` for a build that sets its own base. `recipes: default` answers "no recipes" for a folder the store refuses as unsupported. | phase-5 |
| 2026-08-31 | Created with `EnvironmentConfig`: fields, constructors, deferred failures, the two deliberate omissions, and the `recipes`-absent asymmetry. | `design/environment-builder/phase-5` |

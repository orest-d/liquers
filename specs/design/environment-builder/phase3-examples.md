# Phase 3: Examples & Use-cases - Environment Builder

Phase 1 asked for one thing: an observable boundary after which an environment is ready to evaluate.
Phase 2 delivers it by making `EnvironmentBuilder::build` the only construction path. These examples
exist to answer the practical question that follows — **does the new way read better than the old
one at real call sites?** Every "before" block below is current in-tree code, quoted with its file,
not a strawman.

The progression: Scenario 1 is the ordinary case (build, register, evaluate). Scenario 2 adds the
mechanisms — kind selection, payloads, a custom type registry — using the hardest real call site in
the tree, `liquers-web`. Scenario 3 collects what will bite people.

**Examples are conceptual.** Making them runnable requires the implementation, which is Phase 4.
Each is written so it can be lifted into a test or a guide once the code exists.

> **Amended 2026-08-31**, in two passes. First, after the four prerequisite designs merged: every
> "before" block was re-read against `HEAD`; the only one that moved is Scenario 2a (`liquers-web`),
> noted inline, and T9 changed what it proves because the panic it guarded against was fixed
> directly.
>
> Then two maintainer decisions at the gate. **`to_ref` stays** — so Scenario 3b is rewritten (it
> was a deprecation warning; it is now a supported path), open question 5 is resolved and no longer
> blocks Phase 4, and a new Scenario 3e covers the case the decision exists for: a user-implemented
> environment. **`EnvironmentConfig` is in scope** — so Scenario 4 stops being a sketch, and gains
> tests. See [`DESIGN.md`](./DESIGN.md) §Prerequisite review and §Gate decisions.

## Overview Table

| # | Example | Demonstrates | Phase 2 element exercised |
|---|---|---|---|
| 1a | Hello world, native | The ordinary build-register-evaluate flow | `EnvironmentBuilder::new`, `command_registry`, `build` |
| 1b | Axum server with a file store | Store configuration through the builder | `with_async_store` |
| 2a | `liquers-web`: custom type registry + recipe provider | The hardest real call site; foreign type registration | `with_type_registry`, `with_recipe_provider` |
| 2b | Inline / wasm environment | Kind selection; runtime-free construction | `Inline`, `DefaultKind`, `AssetManagerKind` |
| 2c | Payload environment | The `P` parameter survives consolidation | `GenericEnvironment<V, P, K>` aliases |
| 2d | `liquers-lib` polars test | Extension trait replacing an inherent method | `PolarsCommandRegistration` |
| 3a | The readiness guarantee | The defect this project exists to fix | `build()` ordering, `is_started` |
| 3b | `to_ref` still compiles, and is supported | 348 call sites keep working, no warning | `Environment::to_ref` / `try_to_ref` |
| 3c | `Queued` needs a runtime | Sync ≠ runtime-free | `Queued::build` |
| 3d | Late command registration | The re-runnable barrier | `refresh_command_versions` |
| 3e | A user-implemented `Environment` | The path the builder deliberately does not own | `init_with_envref`, `try_to_ref` |
| 4 | One document configures environment + store | The committed configuration goal | `EnvironmentConfig`, `with_config` |

| # | Test | Checks | Kind |
|---|---|---|---|
| T1 | `build_returns_a_started_manager` | `is_started()` is true the instant `build()` returns | unit |
| T2 | `plan_dependencies_registered_on_first_evaluation` | The original bug: edges are registered, not dropped | integration |
| T3 | `command_version_present_immediately_after_build` | The Phase 1 reproduction, inverted | integration |
| T4 | `concurrent_first_evaluations_share_one_startup` | Issue verification item 3 | integration |
| T5 | `startup_failure_propagates_from_build` | Issue verification item 4 | unit |
| T6 | `readiness_equivalent_across_kinds` | Issue verification item 5 — `Queued` and `Inline` agree | integration, parametric |
| T7 | `refresh_command_versions_expires_dependents` | A changed version cascades | integration |
| T8 | `refresh_is_idempotent_when_nothing_changed` | Re-running expires nothing | unit |
| T9 | `recipe_provider_defaults_across_all_aliases` | The per-crate defaults are preserved, and `CORE-PAYLOAD-ENV-RECIPE-PROVIDER-PANIC` cannot recur | unit |
| T10 | `to_ref_produces_a_ready_envref` | The `to_ref` door is not a hole either | integration |
| T11 | `aliases_are_the_generic_type` | Consolidation did not change any public type identity | compile-time |
| T12 | `inline_builds_without_a_tokio_runtime` | Wasm path; `Inline` spawns nothing | unit |
| T13 | `build_refreshes_command_metadata_versions` | A command mutated after registration is registered under its **refreshed** version, not the stale one — the `to_ref` invariant from `refresh-command-metadata-versions`, inherited by `build()` because it delegates to `try_to_ref` | unit |
| T14 | `custom_environment_gets_the_readiness_guarantee` | A test-local `Environment` implementing only `init_with_envref` reaches a started manager through `to_ref` — the ad-hoc path the gate decision keeps open | integration |
| T15 | `config_roundtrips_and_applies` | `EnvironmentConfig` YAML/JSON round-trip; `with_config` yields the named store router, recipe provider and manager options | unit |
| T16 | `config_errors_surface_at_build` | An unset `${VAR}` and an unclaimed store `type` both fail at `build()`, not at the setter, and the store-type error lists what the chain supports | unit |

## Example

### Scenario 1a — Hello world, native

**Before** (`liquers-core/tests/async_hellow_world.rs`, verbatim shape):

```rust
type CommandEnvironment = SimpleEnvironment<Value>;
let mut env = SimpleEnvironment::<Value>::new();

let cr = &mut env.command_registry;
register_command!(cr, fn world(state) -> result)?;
register_command!(cr, async fn greet(state, greet: String = "Hello") -> result)?;

let envref = env.to_ref();
let state = evaluate(envref.clone(), "world/greet", None).await?;
assert_eq!(state.try_into_string()?, "Hello, world!");
```

**After:**

```rust
type CommandEnvironment = SimpleEnvironment<Value>;
let mut builder = EnvironmentBuilder::<Value, (), Queued>::new();

let cr = &mut builder.command_registry;
register_command!(cr, fn world(state) -> result)?;
register_command!(cr, async fn greet(state, greet: String = "Hello") -> result)?;

let envref = builder.build()?;
let state = evaluate(envref.clone(), "world/greet", None).await?;
assert_eq!(state.try_into_string()?, "Hello, world!");
```

Two lines change, and `CommandEnvironment` — the alias `register_command!` needs — is untouched
because `SimpleEnvironment<Value>` still names a type. The `?` on `build()` is the visible
difference in kind: construction can now report a problem instead of leaving one latent.

**There is exactly one builder type.** An earlier draft proposed `SimpleEnvironmentBuilder<V>` /
`ImmediateEnvironmentBuilder<V>` aliases for brevity; those are withdrawn, because a name per
environment reads like a builder *family* and so re-suggests the very duplication this project
removes. Brevity comes from **default type parameters** instead:

```rust
pub struct EnvironmentBuilder<V: ValueInterface,
                              P: PayloadType = (),
                              K: AssetManagerKind = DefaultKind> { /* … */ }

// DefaultKind is target-selected, in liquers-core:
#[cfg(not(target_arch = "wasm32"))] pub type DefaultKind = Queued;
#[cfg(target_arch = "wasm32")]      pub type DefaultKind = Inline;
```

So the ordinary call is `EnvironmentBuilder::<Value>::new()` — queued natively, inline on wasm — and
a parameter is spelled out only when it is being chosen: `EnvironmentBuilder::<Value, (), Inline>`
to force inline on native, `EnvironmentBuilder::<Value, UiPayload>` for a payload. One type, three
knobs, no alias family.

`GenericEnvironment<V, P = (), K = DefaultKind>` takes the same defaults. The *environment* aliases
(`SimpleEnvironment`, `DefaultEnvironment`, `WebEnvironment`) stay, but those are pre-existing names
kept for compatibility — not new ones being coined.

### Scenario 1b — Axum server with a file store

**Before** (`liquers-axum/examples/basic_server.rs:61-69`):

```rust
let async_store = AsyncFileStore::new(&store_path, &Key::new());

let mut env = SimpleEnvironment::<Value>::new();
env.with_async_store(Box::new(async_store));

let env = register_commands(env).expect("Failed to register commands");
let env_ref = env.to_ref();
```

**After:**

```rust
let async_store = AsyncFileStore::new(&store_path, &Key::new());

let mut builder = EnvironmentBuilder::<Value>::new()
    .with_async_store(Arc::new(async_store));

register_commands(&mut builder.command_registry)?;
let env_ref = builder.build()?;
```

Three things improve at once. The `let mut env; env.with_x(…);` dance collapses into a chain,
because setters take `self`. `register_commands` takes `&mut CommandRegistry` instead of consuming
and returning the environment — a signature the helper wanted anyway. And `Arc` replaces `Box`,
matching what the field actually stores; today's `Box` is converted with `Arc::from` immediately.

The `.expect(…)` also disappears: `build()` returns `Result`, so an example can use `?` throughout
rather than mixing `expect` with error handling.

### Scenario 2a — `liquers-web`: the hardest real call site

This one matters most, because if the builder does not fit here it does not fit anywhere. It needs a
**caller-supplied type registry** (to register the `js.Value` foreign handle), a **recipe provider**,
and it must keep the environment un-shared while JavaScript commands are registered.

**Before** (`liquers-web/src/environment.rs:80-107`, condensed):

```rust
pub fn new_environment() -> Result<WebEnvironment, Error> {
    let mut types = TypeRegistry::from_value_type::<Value>();
    types.register(crate::value::js_value_type_info())?;

    let mut env = WebEnvironment::new_with_type_registry(types);
    crate::builtins::register_builtin_commands(&mut env)?;
    env.with_default_recipe_provider();
    Ok(env)
}

pub fn build_environment() -> Result<EnvRef<WebEnvironment>, Error> {
    Ok(new_environment()?.to_ref())
}
```

**After:**

```rust
pub fn new_environment() -> Result<EnvironmentBuilder<Value>, Error> {
    let mut types = TypeRegistry::from_value_type::<Value>();
    types.register(crate::value::js_value_type_info())?;

    let mut builder = EnvironmentBuilder::<Value>::new()
        .with_type_registry(types)
        .with_recipe_provider(Arc::new(DefaultRecipeProvider));

    crate::builtins::register_builtin_commands(&mut builder.command_registry)?;
    Ok(builder)
}

pub fn build_environment() -> Result<EnvRef<WebEnvironment>, Error> {
    new_environment()?.build()
}
```

The structure is **already** builder-shaped — `new_environment` exists precisely to keep the
environment un-shared and mutable, and its doc comment says so ("Kept separate from `to_ref` so that
JavaScript commands can be registered into it before it is shared"). The builder is what that
function was reaching for. Renaming its return type is most of the migration.

The rebuild machinery (`REGISTERED_SPECS` replay for `POST-INIT-COMMAND-REGISTRATION`) is unaffected:
it calls `new_environment()` and replays declarations, which now means replaying into a builder.
One code path, as before.

> **Note for Phase 4:** the comment above `with_default_recipe_provider` in that file claims
> `DefaultEnvironment::get_recipe_provider` *panics* when none is set, citing
> `liquers-lib/src/environment.rs:152`. That is stale — `LIB-RECIPE-PROVIDER-PANIC` is `closed` and
> the field is now a non-optional `Arc`. The provider call is still wanted (the default reads
> recipes from the store), but the stated reason is wrong and should be corrected while the
> surrounding lines are being edited. **Still true at `HEAD` on 2026-08-31**; the comment survived
> three intervening PRs, which is itself the argument for fixing it in passing.

> **Re-checked 2026-08-31 — the file has moved, the shape has not.** Since PR 46 and PR 50,
> `liquers-web/src/environment.rs` also holds `STORE_CONFIG` (a
> `liquers_core::store_config::StoreRouterConfig` — the crate no longer depends on `liquers-store`
> at all), `STORE_OBJECTS`, and a `REGISTERED_SPECS` replay whose parser now builds on
> `liquers_core::command_declaration::CommandDeclaration`. None of that changes the migration
> above: `new_environment()` still returns an un-shared environment and `build_environment()` still
> calls `to_ref` on it, so renaming the return type is still most of the work. What it adds is a
> **rebuild obligation** for Phase 4 — a rebuild must replay the store configuration and store
> objects as well as the command declarations, and the builder must therefore be constructible
> repeatedly from retained state. It is, since `new_environment` returns the builder by value; the
> point is that the migration must not collapse the two functions into one.

### Scenario 2b — Inline / wasm, and why the kind is a type parameter

```rust
// Runs in a browser: no Tokio runtime, nothing spawned.
let envref = EnvironmentBuilder::<Value, (), Inline>::new()
    .with_async_store(Arc::new(browser_store))
    .build()?;
```

`Inline` and `Queued` differ only in the kind parameter, and the environment type is otherwise
identical — which is what lets `liquers-lib` express its target selection as data rather than as an
import-shadowing hack:

```rust
// Before — liquers-lib/src/environment.rs:20-22
#[cfg(not(target_arch = "wasm32"))]
use liquers_core::assets::DefaultAssetManager as SelectedAssetManager;
#[cfg(target_arch = "wasm32")]
use liquers_core::assets::ImmediateAssetManager as SelectedAssetManager;

// After — liquers-lib needs no target selection of its own at all
pub type DefaultEnvironment<V, P = ()> = GenericEnvironment<V, P>;   // K = DefaultKind
```

The cfg pair disappears from `liquers-lib` entirely: `DefaultKind` in `liquers-core` already selects
`Queued` natively and `Inline` on wasm, which is exactly what the shadowed import was emulating.

### Scenario 2c — Payload environment

```rust
#[derive(Clone)] struct UiPayload { /* … */ }
impl PayloadType for UiPayload {}

let envref = EnvironmentBuilder::<Value, UiPayload, Queued>::new()
    .with_async_store(store)
    .build()?;
// `SimpleEnvironmentWithPayload<Value, UiPayload>` still names this exact type.
```

The payload dimension survives consolidation as a plain parameter rather than as two extra structs.
`liquers-core/tests/payload_inheritance.rs` and `plan_cwd_freeze.rs` keep their type names and change
only their construction lines.

### Scenario 2d — `liquers-lib` polars test, and the coherence constraint

**Before** (`liquers-lib/tests/polars_commands.rs:17-22`):

```rust
fn create_test_env() -> DefaultEnvironment<Value> {
    let mut env = DefaultEnvironment::<Value>::new();
    env.with_default_recipe_provider();
    env.register_polars_commands().expect("Failed to register polars commands");
    env
}
```

**After:**

```rust
use liquers_lib::environment::PolarsCommandRegistration;  // extension trait, now required

fn create_test_env() -> Result<EnvRef<DefaultEnvironment<Value>>, Error> {
    // `default_environment_builder` carries liquers-lib's DefaultRecipeProvider default,
    // which the bare core builder does not — see Phase 2 §The recipe-provider default is
    // per-crate. `env.with_default_recipe_provider()` above was redundant with the old
    // constructor's default and stays redundant here.
    let mut builder = liquers_lib::default_environment_builder::<Value, ()>();
    builder.register_polars_commands()?;
    builder.build()
}
```

`register_polars_commands` moves from an inherent method to an extension trait, because
`DefaultEnvironment` becomes an alias of a type defined in `liquers-core` and Rust permits an
inherent `impl` only in the defining crate (Phase 2, finding B1). The call site is unchanged apart
from the `use`.

Note the helper now returns the `EnvRef` rather than the environment — it has to, since the
environment can no longer be handed around un-shared. That is the *point*, but it means helpers of
this shape (there are several across the test suites) change their return type.

### Scenario 3a — The readiness guarantee, stated as a test

This is the defect the project exists to fix, and it is now expressible as an assertion rather than
a sleep:

```rust
#[tokio::test]
async fn build_returns_a_started_manager() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = EnvironmentBuilder::<Value>::new();
    register_command!(&mut builder.command_registry, fn world(state) -> result)?;

    let envref = builder.build()?;

    // Before this project, both of these were false immediately after `to_ref()`.
    assert!(envref.get_asset_manager().is_started());
    let key = DependencyKey::for_command_metadata(&CommandKey::new_name("world"));
    assert!(envref.get_asset_manager().dependency_manager()
        .get_version(&key).await.is_some());
    Ok(())
}
```

Compare with what the Phase 1 reproduction had to do — sleep and hope — and with
`liquers-core/tests/dependency_manager_integration.rs:87-89`, which today reads:

```rust
// Give the spawned task time to complete
tokio::task::yield_now().await;
tokio::time::sleep(std::time::Duration::from_millis(50)).await;
```

Both lines delete. A timing-dependent test becomes a deterministic one, which is the clearest
before/after in the suite.

### Scenario 4 — `EnvironmentConfig`: one document for the environment and its store

**In scope as of 2026-08-31.** This scenario was written as an illustrative sketch, testing only
that the design did not *preclude* a single configuration point. The maintainer decision at the gate
makes it the goal: the store router configuration is a section of the environment configuration, and
one file or JSON structure configures both. Everything below is now specified rather than sketched —
`liquers-core` holds every type it names, and Phase 4 sequences it as the final, separable step
after the readiness fix is green.

**What configuration can and cannot cover.** Commands are Rust functions registered by a macro; no
YAML can name one. So a configuration file configures *services*, and code registers *commands*.
The builder already splits exactly along that line — `with_*` setters are the config-drivable half,
the public `command_registry` field is the code-only half — which is the main thing this sketch
confirms.

```yaml
# environment.yaml
store:                          # verbatim StoreRouterConfig, reused unchanged
  stores:
    - type: fs
      prefix: data
      config: { root: "${LIQUERS_DATA}" }
    - type: memory
      prefix: tmp
recipes: default                # default | trivial
assets:
  job_capacity: 8               # queued only; see the finding below
```

> **Amended 2026-08-31.** When this scenario was written, `StoreRouterConfig` lived in
> `liquers-store` and `recipes: default` had no type behind it, so the sketch placed
> `EnvironmentConfig` in `liquers-store` and treated `RecipeProviderChoice` as hypothetical. Both
> premises are gone: PR 46 moved the store configuration types, the `StoreFactory` trait, factory
> chaining and `StoreRouterBuilder` into `liquers-core`, and PR 48 added the real
> `RecipeProviderChoice` to `liquers-core/src/recipes.rs`. Every field below is a core type, so the
> whole document lives beside the builder it configures.

```rust
// in liquers-core/src/environment_config.rs: since PR 46 the crate holds StoreRouterConfig,
// StoreFactory and StoreRouterBuilder, so it sees every field of this struct and the
// EnvironmentBuilder they configure.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EnvironmentConfig {
    #[serde(default)] pub store: StoreRouterConfig,
    #[serde(default)] pub recipes: RecipeProviderChoice,
    #[serde(default)] pub assets: AssetManagerOptions,
}

impl EnvironmentConfig {
    pub fn from_yaml(yaml: &str) -> Result<Self, Error>;
    pub fn from_json(json: &str) -> Result<Self, Error>;
    pub fn from_toml(toml: &str) -> Result<Self, Error>;
    pub fn to_yaml(&self) -> Result<String, Error>;
    pub fn to_json(&self) -> Result<String, Error>;
    pub fn expand_env_vars(&mut self) -> Result<(), Error>;
}

// applied through the builder, in the same direction as every other setter:
impl<V, P, K> EnvironmentBuilder<V, P, K> {
    pub fn with_config(self, config: EnvironmentConfig, factory: Box<dyn StoreFactory>) -> Self;
}
```

Use:

```rust
let config = EnvironmentConfig::from_yaml(&std::fs::read_to_string("environment.yaml")?)?;

let mut builder = EnvironmentBuilder::<Value>::new()
    .with_config(config, Box::new(default_store_factory()));
register_my_commands(&mut builder.command_registry)?;   // code, not config
let envref = builder.build()?;
```

**Revised from the sketch.** `apply(builder, &factories) -> Result<builder>` became
`builder.with_config(config, factory) -> Self`, for three reasons: it reads in the same direction as
every other setter, so configuration and hand-written overrides compose in either order without one
of them inverting; it stays infallible, because store construction is deferred to `build()` where
the other fallible work already lives; and the factory is a `Box<dyn StoreFactory>`, matching
`StoreRouterBuilder::new`'s existing parameter rather than introducing a slice-of-boxes convention
beside it. `${VAR}` expansion also moves into `build()` — the caller no longer has to remember
`expand_env_vars()`, and the wasm path that must *not* expand says so explicitly with
`with_store_config_unexpanded`, mirroring `StoreRouterBuilder::build_without_env_expansion`.

Two behaviors worth stating, because both are silent otherwise:

- **`recipes:` absent means `default`, not `trivial`.** `RecipeProviderChoice`'s `#[default]` variant
  is `Default`, chosen deliberately by `recipe-provider-selection` on the grounds that a document
  saying nothing about recipes most plausibly wants them to work. That is *not* the core builder's
  unconfigured default, which is `Trivial`. So applying a configuration is an explicit act that sets
  the provider — a bare `EnvironmentBuilder::<Value>::new()` resolves recipes trivially, and the
  same builder `.with_config(EnvironmentConfig::default(), …)` resolves them through the store.
  Intended, and the reference must say so.
- **The expander errors on an unset variable** and has no default-value syntax, so a missing
  `LIQUERS_DATA` fails the build rather than producing an empty root. Also intended; T16 pins it.

**~~Layering.~~ Superseded 2026-08-31.** This paragraph read: `EnvironmentConfig` cannot live in
`liquers-core` because `StoreRouterConfig` lives in `liquers-store`, which depends on core, so it
belongs in `liquers-store`; and that is *why* `with_async_store` takes an already-constructed
`Arc<dyn AsyncStore>` rather than a store config.

Both halves are void. `STORE-CONFIG-IN-CORE` (PR 46) put `StoreRouterConfig`, `StoreConfig`,
`expand_env_vars`, `StoreFactory`, factory chaining and `StoreRouterBuilder` in `liquers-core`, and
`liquers-web` dropped `liquers-store` entirely as the design intended. So `EnvironmentConfig` can
live in core, and `with_async_store`'s signature no longer has a layering justification — it has
only an ordinary scope one: this project is a readiness fix, and adding a configuration entry point
widens it. That is a decision rather than a constraint now, and it is **Phase 2 open question 4**,
where the recommendation is to keep `with_async_store` as the sole store entry point for this
project and add `with_store_config` later as a purely additive setter.

`RecipeProviderChoice` in the struct above is likewise no longer hypothetical: PR 48 added exactly
that enum, `#[serde(rename_all = "lowercase")]` with `Default` / `Trivial` variants, so
`recipes: default` in the YAML deserializes with no new type at all.

**The manager kind stays a type parameter, and should.** A YAML string cannot select a type: two
branches of a `match` on `"queued"` / `"inline"` produce two different concrete environment types,
and `Environment` is not object-safe (associated types, `Sized`), so they cannot be erased behind a
`dyn`. This is not a limitation worth fighting — the choice is a *build* fact, not a deployment
one. Wasm has no choice at all; natively `Inline` exists for deterministic testing, not for
production tuning. `DefaultKind` already gets it right on both targets.

Where runtime selection is genuinely wanted, the application monomorphizes its own tail:

```rust
match config.manager.as_str() {
    "queued" => serve(config.apply(EnvironmentBuilder::<Value, (), Queued>::new(), &f)?.build()?).await,
    "inline" => serve(config.apply(EnvironmentBuilder::<Value, (), Inline>::new(), &f)?.build()?).await,
    other    => return Err(Error::general_error(format!("unknown manager kind: {other}"))),
}
```

`serve` is generic over the environment, so the two branches converge immediately. Explicit match,
no default arm — consistent with the project's enum convention.

> **Finding 3-A — a Phase 2 amendment this sketch produced.** `AssetManagerKind::build(envref)`
> takes no options, so `assets.job_capacity` has no way to reach
> `DefaultAssetManager::with_capacity`, whose capacity is currently hardcoded to 4. Any per-manager
> setting — capacity now, expiration-monitor tuning later — is unreachable, and adding a parameter
> afterwards is a breaking change to a public trait. **Amend the signature now**, while it costs
> nothing:
>
> ```rust
> fn build<E: Environment>(envref: EnvRef<E>, options: &AssetManagerOptions)
>     -> Result<Arc<Self::Manager<E>>, Error>;
> ```
>
> with `AssetManagerOptions` a plain serde-able struct of optional fields in `liquers-core`, and
> `EnvironmentBuilder::with_asset_manager_options(self, …)`. A kind ignores fields that do not apply
> to it — `Inline` has no queue — which is a real wart: a `job_capacity` set against an inline
> environment would be **silently ignored**. Phase 4 should make `build` return `Err` on a setting
> the kind cannot honor, rather than dropping it quietly. Note `build` also becomes fallible here,
> which it should have been anyway for symmetry with `start`.

## Corner Cases

### 3b — `to_ref` still compiles, and is now a supported path

```rust
let envref = SimpleEnvironment::<Value>::new().to_ref();   // no warning: supported
assert!(envref.get_asset_manager().is_started());           // and ready
```

**Rewritten 2026-08-31.** This corner case previously read "all 336 sites keep working, emitting a
deprecation warning", and carried open question 5 — the contradiction between deprecating-and-hiding
`to_ref` and claiming its call sites were free. The maintainer decision removes the contradiction
rather than resolving it: constructors stay `pub`, `to_ref` stays on the `Environment` trait with its
signature and no `#[deprecated]`, and the 348 sites are genuinely untouched.

What changed for them is invisible and is the entire point: `to_ref` now returns an `EnvRef` whose
manager is constructed, installed and **started**, because its body delegates the sequence to
`try_to_ref` and `init_with_envref`. Before this project the same line returned an `EnvRef` whose
manager startup was a detached task that might not have run.

```rust
// When the error matters, the fallible half:
let envref = SimpleEnvironment::<Value>::new().try_to_ref()?;
```

`to_ref` panics on a startup error, which neither built-in manager can produce (startup writes an
in-memory map). `try_to_ref` is the same body with the `Result` exposed, and is what
`EnvironmentBuilder::build` calls.

> **Open question 5 — resolved, no longer blocking Phase 4.** Keep constructors `pub`. The
> alternative (`pub(crate)` now, mandatory migration in the same PR) is incompatible with supporting
> ad-hoc environments at all: a caller who cannot construct an environment cannot call `to_ref` on
> one.

### 3e — A user-implemented `Environment`

The case the gate decision exists for. A user with their own global services implements
`Environment` directly, and the readiness guarantee has to reach them too — the builder owns
concrete environment types (Phase 1, question 1) and deliberately does not serve this path.

```rust
struct MyEnvironment {
    command_registry: CommandRegistry<Self>,
    // The deferred-slot pattern: the manager cannot exist before the EnvRef does.
    asset_store: OnceLock<Arc<ImmediateAssetManager<Self>>>,
    // … the caller's own global services …
}

impl Environment for MyEnvironment {
    // … associated types and accessors …

    /// The one method that carries the readiness obligation: on return the manager must be
    /// constructed with this EnvRef, installed, and started.
    fn init_with_envref(&self, envref: EnvRef<Self>) -> Result<(), Error> {
        let manager = Arc::new(ImmediateAssetManager::new(envref));
        let _ = self.asset_store.set(manager.clone());
        manager.start()
    }

    // to_ref and try_to_ref are provided; nothing else to implement.
}

let envref = MyEnvironment::new().try_to_ref()?;
assert!(envref.get_asset_manager().is_started());
```

Three lines of obligation, checked by the compiler at two of them — the signature is fallible, and
`start()` returns a `Result` that must be used. Compare with today's contract, which is "call
`set_envref`, then arrange for `start` somehow", where arranging it wrongly is exactly
`QUEUED-MANAGER-STARTUP-READINESS`.

This is why `init_with_envref` is kept rather than deleted: it is the *seam* that lets one generic
`try_to_ref` body serve both the built-in environments and a user-defined one. Deleting it, as the
draft proposed, would have required every such user to reimplement the sequence — including the
metadata-version refresh they have no reason to know about.

### 3c — Sync does not mean runtime-free

```rust
fn main() {
    let envref = EnvironmentBuilder::<Value>::new().build();  // PANICS
    //  `Queued::build` -> DefaultAssetManager::with_capacity -> tokio::spawn
    //  -> "there is no reactor running"
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let envref = EnvironmentBuilder::<Value>::new().build()?;  // fine
    Ok(())
}
```

`build()` being synchronous means "no `.await` at the call site", not "no runtime". `Queued` spawns
a job queue and an expiration monitor from its constructor. `Inline` spawns nothing and genuinely
works with no runtime — which is what wasm needs. The guide must state this per kind; stating it
once globally would be wrong in both directions.

### 3d — Registering a command after `build()`

```rust
let envref = builder.build()?;
// … later, a plugin registers a command through whatever mechanism
// POST-INIT-COMMAND-REGISTRATION eventually provides …
envref.get_asset_manager().refresh_command_versions()?;
```

`refresh_command_versions` is deliberately *not* `start`. `start` establishes readiness once;
`refresh` re-reads the metadata registry and re-registers versions, and a **changed** version makes
`register_version` return the dependents to expire — the cascade that invalidates assets built
against the old command. This is the hook dynamic registration needs, and the reason the barrier is
not a one-shot cell.

Its signature depends on Phase 2 open question 1: a changed version produces `ExpiredDependents`,
and applying them is async.

### Other corner cases

| Case | Expected behavior |
|---|---|
| `build()` called twice on one builder | Impossible — `build(self)` consumes it. |
| Two builders, two environments | Independent; each gets its own manager and dependency graph. |
| Empty command registry | `start()` registers nothing and succeeds; `is_started()` is true. |
| No store configured | `NoAsyncStore` default, as today. A `-R/` query fails with `KeyNotFound`. |
| No recipe provider configured | `RecipeProviderChoice::Trivial` from the core builder; `RecipeProviderChoice::Default` from `liquers_lib::default_environment_builder`. Preserving both is deliberate — one global default would silently break `-R/` queries for `DefaultEnvironment` users. |
| Command metadata mutated by `register_command!` after the registry computed its version | `build()` step 0 refreshes the metadata versions before startup snapshots them, matching what `to_ref` has done since `refresh-command-metadata-versions`. Covered by T13. |
| Manager slot already installed | Unreachable: `build()` is the sole writer and holds the only `EnvRef`. `debug_assert!`, not a runtime branch. |
| Dropping the `EnvRef` | Still leaks (`ENVIRONMENT-MANAGER-REFERENCE-CYCLE`, deferred by decision). Unchanged, not worsened. |
| `Queued` on wasm | Does not exist — `#[cfg(not(target_arch = "wasm32"))]`. A wasm build naming it fails to compile, which is the intent. |

## Test Plan

Conventions per `liquers-unittest`: `#[tokio::test]` for async, `-> Result<(), Box<dyn std::error::Error>>`
where `?` is used, no `unwrap`/`expect` outside tests, typed error constructors, explicit match arms.

### Unit tests — `liquers-core/src/environment_builder.rs`

| Test | Assertion |
|---|---|
| `build_returns_a_started_manager` (T1) | `is_started()` true on return; command version present |
| `startup_failure_propagates_from_build` (T5) | With a test kind whose `start` returns `Err`, `build()` returns that `Error`; no `EnvRef` is produced |
| `refresh_is_idempotent_when_nothing_changed` (T8) | Second `refresh_command_versions()` expires nothing |
| `recipe_provider_defaults_across_all_aliases` (T9) | Core builder yields `RecipeProviderChoice::Trivial` for all four aliases and **none panics**; `liquers_lib::default_environment_builder` yields `RecipeProviderChoice::Default`. Both asserted, so a later collapse of the two defaults fails the test. **Amended 2026-08-31:** the "none panics" half now guards a fix that already landed (PR 51) rather than delivering one, so this is a regression test, not the closing evidence for `CORE-PAYLOAD-ENV-RECIPE-PROVIDER-PANIC` — that issue is already `closed` |
| `build_refreshes_command_metadata_versions` (T13) | Register a command, mutate its metadata (the shape `register_command!` produces), `build()`, then read the version the dependency manager holds: it is the **refreshed** version. Mirrors `immediate_environment_to_ref_refreshes_metadata_versions` in `context.rs`, one layer further on — that test proves the registry was refreshed, this one proves the refreshed value reached the dependency graph |
| `inline_builds_without_a_tokio_runtime` (T12) | Plain `#[test]`, no `#[tokio::test]`: `Inline` builds. `manager_parametric.rs` already carries a "no-tokio-runtime proof" for `ImmediateAssetManager`; extend it to construction rather than duplicating it |
| `builder_defaults_match_previous_environment_defaults` | `NoAsyncStore`, empty registry, type registry from `V` |
| `config_roundtrips_and_applies` (T15) | `EnvironmentConfig` survives a YAML and a JSON round-trip; `with_config` produces the named store router, the named recipe provider and the given manager options. Also asserts the documented asymmetry: `EnvironmentBuilder::new()` alone resolves recipes **trivially**, while the same builder with `EnvironmentConfig::default()` applied resolves them through the store, because `RecipeProviderChoice`'s `#[default]` is the *document* default |
| `config_errors_surface_at_build` (T16) | An unset `${VAR}` in the store section, and a store `type` no factory in the chain claims, both fail at `build()` rather than at the setter; the second error names the store types the chain does support |

### Integration tests — `liquers-core/tests/`

| Test | File | Assertion |
|---|---|---|
| `command_version_present_immediately_after_build` (T3) | `environment_builder.rs` *(new)* | The Phase 1 reproduction inverted: `get_version` is `Some` with **no sleep** |
| `plan_dependencies_registered_on_first_evaluation` (T2) | `environment_builder.rs` | Evaluate a keyed asset immediately after `build()`; `expire_dependents(command_impl_key)` returns that asset. **This is the original bug** — it returned 0 before |
| `concurrent_first_evaluations_share_one_startup` (T4) | `environment_builder.rs` | N concurrent first evaluations; startup ran once (counter on a test kind), all observe complete state |
| `readiness_equivalent_across_kinds` (T6) | `manager_parametric.rs` *(extend)* | Same assertions for `Queued` and `Inline` — issue verification item 5. The file is already parametric over managers |
| `refresh_command_versions_expires_dependents` (T7) | `environment_builder.rs` | Change a metadata version, refresh, assert the dependent asset expired |
| `to_ref_produces_a_ready_envref` (T10) | `environment_builder.rs` | Same readiness assertions through the `to_ref` door. **Renamed 2026-08-31**: no `#[allow(deprecated)]`, because `to_ref` is a supported path and carries no attribute. Also asserts `try_to_ref` agrees |
| `custom_environment_gets_the_readiness_guarantee` (T14) | `environment_builder.rs` | A test-local `Environment` implementing `init_with_envref` and nothing else reaches a started manager through `to_ref` — Scenario 3e. This is the regression test for the gate decision: if a later refactor moves the readiness sequence into the builder, this fails |
| `aliases_are_the_generic_type` (T11) | `environment_builder.rs` | Compile-time: a fn taking `GenericEnvironment<Value, (), Queued>` accepts a `SimpleEnvironment<Value>` |

### Regression and migration coverage

- `dependency_manager_integration.rs:87-89` — **delete the `yield_now()` + `sleep(50ms)`**; the
  assertion becomes deterministic.
- `payload_inheritance.rs`, `plan_cwd_freeze.rs`, `injection.rs`, `expiration_integration.rs`,
  `volatility_integration.rs`, `type_consistency.rs`, `recipe_cwd_resolution.rs`,
  `asset_failure_contract.rs`, `manager_parametric.rs` — construction lines migrate; assertions
  unchanged. This is the bulk of the 94 test-side sites and is the real regression net: if
  consolidation broke a type identity, these stop compiling.
- `liquers-lib/tests/polars_commands.rs`, `registry_export.rs` — extension trait in scope; registry
  export unchanged (no command signatures move).
- `liquers-web` — `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`
  must pass; the rebuild-on-late-registration path is the sensitive one, and since PR 46 it also
  replays a `StoreRouterConfig` and its store objects, not only command declarations.
- **`EnvironmentConfig` is the last step and separably testable.** T15/T16 exercise it through the
  builder with no environment-lifecycle assertions, so if the configuration layer slips, the
  readiness fix and its tests (T1-T14) still stand alone. Phase 4 should keep that separation.

### Commands to run

```bash
cargo test -p liquers-core --lib --tests
cargo test -p liquers-lib --lib --tests
bash scripts/check-build-matrix.sh
cargo clean && cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles
```

## Documentation and Learning Log

Material for Phase 5, collected while writing these examples.

**Guide-worthy (for `ENVIRONMENT_CONSTRUCTION_GUIDE.md`):**

- Scenario 3e as the "implementing your own environment" section: the `init_with_envref` contract is
  the one obligation a custom environment carries, and getting it wrong reproduces the very issue
  this project closes.
- Scenario 4 as the configuration walkthrough, with the `recipes:`-absent asymmetry called out
  explicitly — it is the kind of default that is obvious in the reference and surprising in
  practice.

- Scenario 1a as the opening walkthrough — smallest complete build-register-evaluate.
- Scenario 1b for stores; 2b for kind selection and the wasm story.
- Scenario 3c (`Queued` needs a runtime) as an explicit pitfall box — this will be a recurring
  support question, and the failure mode is a panic with an unhelpful message.
- Scenario 2a as the "integrating a host language" worked example, linked from
  `LANGUAGE-INTEGRATION_GUIDE.md`.
- The `to_ref` → `build()` migration table.

**Executable evidence to link:** T2 (`plan_dependencies_registered_on_first_evaluation`) is the test
that demonstrates why the guide insists on `build()`; T6 shows the guarantee is kind-independent.

**Learning points:**

1. `liquers-web`'s `new_environment` / `build_environment` split is already the builder pattern,
   invented locally because the framework lacked it. Evidence the abstraction belongs in core.
2. The stale panic comment in `liquers-web/src/environment.rs` (Scenario 2a note) shows how a fixed
   defect leaves misleading commentary behind.
3. Test-helper functions that return an un-shared environment (`create_test_env`) have to change
   return type. Not hard, but it is the most repeated migration shape and belongs in the guide.
4. Writing the examples surfaced two design questions the architecture had not settled — the
   builder aliases (open question 4) and the `pub(crate)` / `to_ref` tension (open question 5,
   material). That tension was invisible until a call site outside `liquers-core` was written out.
5. *(2026-08-31)* Reversing finding A1 made the design **smaller**. The finding removed `to_ref`
   from the trait because a defaulted body cannot construct a builder for an arbitrary implementor —
   true, and irrelevant: the body needs the *sequence*, not a builder, and the one step that varies
   is already behind `init_with_envref`. Restoring it means `build()` delegates to `try_to_ref`
   instead of reimplementing it, so one readiness guarantee has one implementation. Worth recording
   as guidance: when a trait method "cannot be generic", check whether the varying part is already
   abstracted by a neighbouring hook before moving the method to a concrete type.
6. *(2026-08-31)* A design that waits between phases accumulates stale premises rather than stale
   conclusions. Four prerequisite PRs merged between Phase 3 and this review; none of them touched
   the architecture, and all four touched facts the documents cited to justify it — where a type
   lives, whether a function panics, what `to_ref` does first. The one that mattered was invisible
   from the outside: `to_ref` gained a `refresh_metadata_versions()` call, and `build()`, which
   deliberately does *not* delegate to `to_ref`, had no equivalent. Worth a Phase 5 note — a
   construction path that bypasses another must be re-checked against it whenever the bypassed one
   changes, and nothing in the workflow does that automatically.

## Review Record

Three review passes run sequentially before the approval gate (skill host-compatibility fallback,
no agents spawned): Phase 1 conformity, Phase 2 conformity, and codebase + query validation.

**Reviewer 1 — Phase 1 conformity.** Examples exercise every Phase 1 decision: sync fallible
`build()` (1a, 1b), kind selection replacing the cfg-import hack (2b), the readiness guarantee as an
assertion (3a), the re-runnable barrier (3d), `to_ref` still correct (3b), the reference
cycle explicitly unchanged (corner-case table). No scope drift; no Phase 1 element unexercised.

**Reviewer 2 — Phase 2 conformity.** Signatures in the examples match §Function Signatures:
`with_async_store(Arc<dyn AsyncStore>)` not `Box` (a real change from today's `Box`, shown in 1b);
`command_registry` as a public field, not an accessor (1a, 1b, 2a); `build(self) -> Result<EnvRef<…>, Error>`;
`PolarsCommandRegistration` as an extension trait per finding B1 (2d). **Finding 2-A:** writing
Scenario 3b exposed a contradiction Phase 2 did not catch — `pub(crate)` constructors versus "336
`to_ref` sites keep working" cannot both hold for callers outside `liquers-core`, and its own
`tests/` are outside. Recorded as open question 5 and marked blocking for Phase 4 rather than
papered over.

**Reviewer 3 — codebase and query validation.** Every "before" block was re-read against its source
file. Corrections applied: the sleep in `dependency_manager_integration.rs` is **50 ms preceded by a
`yield_now()`**, not 100 ms as first written; `manager_parametric.rs` already contains a
no-tokio-runtime proof, so T12 extends it rather than duplicating. Verified: `manager_parametric.rs`
is genuinely parametric over both managers (its module doc states the contract), so T6 fits there;
`liquers-web/src/environment.rs` really does split `new_environment` from `build_environment` with a
doc comment explaining why, so Scenario 2a's claim is accurate; `liquers-axum/examples/basic_server.rs:61-69`
matches the quoted shape.

Query validation: the only query strings used are `world/greet` (Scenarios 1a) — checked with
`liquers-validate --command world --command greet -- 'world/greet'`, status **Ok**, `encoded`
round-trips to `world/greet`. The commands are example-local rather than registry commands, hence
the `--command` overrides. No `-R/` resource queries appear, so no store-presence check applies.
No spaces, newlines or special characters in any query.

**Post-merge review (2026-08-31).** A fourth pass re-read every "before" block against `HEAD` after
the four prerequisite designs merged. `liquers-core`'s construction and lifecycle code is untouched,
so Scenarios 1a, 2b, 2c, 3a, 3b, 3c and the `dependency_manager_integration.rs` sleep all still
quote current source. `liquers-axum/examples/basic_server.rs` is unchanged. `liquers-lib`'s
`SelectedAssetManager` cfg pair and inherent `register_polars_commands` are unchanged, so 2d stands.
Scenario 2a's file grew (store-configuration replay, `CommandDeclaration`-based parsing) without
changing shape — noted inline, with a rebuild obligation added for Phase 4. Scenario 4's layering
argument is superseded and struck through. One test added (T13) for the `build()` step-0 refresh
invariant; T9 re-scoped from closing evidence to regression guard.

**Gate decisions (2026-08-31).** Two maintainer decisions, applied above. **`to_ref` stays** —
corner case 3b rewritten from "deprecated but working" to "supported", new Scenario 3e for the
user-implemented environment the decision exists for, T10 renamed and T14 added, open question 5
resolved and Phase 4 unblocked. **One configuration document** — Scenario 4 promoted from sketch to
committed scope, its `apply` reshaped into `with_config` to read in the same direction as the other
setters, and T15/T16 added. The examples were re-read after both: no scenario contradicts the
revised architecture, and 1a, 1b, 2a-2d are unaffected because none of them ever called `to_ref` or
a configuration API.

## Open Questions

**Nothing here blocks Phase 4 any more.** The two that did — open question 5 below, and Phase 2's
question 4 — were resolved by maintainer decision on 2026-08-31.

1. *(from Phase 2)* `refresh_command_versions` cascade application — return `ExpiredDependents` or
   apply synchronously? **Open**, decides one signature; the readiness guarantee does not depend on
   it.
2. *(from Phase 2)* Confirm no wasm path needs `Queued` present-but-unusable. **Open**, low risk.
3. **~~Is `to_ref` deprecated indefinitely, or removed once tests migrate?~~ Resolved: neither.**
   `to_ref` is supported and carries no deprecation. The builder is recommended and documented;
   in-tree call sites are phased out only where cheap. `EnvRef::new` keeps its deprecation.
4. **~~Builder convenience aliases?~~ Resolved: none.** One `EnvironmentBuilder<V, P, K>` with
   default type parameters (`P = ()`, `K = DefaultKind`, target-selected). A builder name per
   environment would re-suggest the duplication this project removes. Side benefit: `liquers-lib`
   loses its own target-selection cfg pair, since `DefaultKind` already does it.
5. **~~`pub(crate)` constructors versus 336 working `to_ref` sites.~~ Resolved: constructors stay
   `pub`.** The tension was created by the proposal to hide them, and the maintainer decision
   withdraws it — supporting ad-hoc environments requires that constructing one stays possible. No
   mandatory migration, no deprecation warnings, Phase 4 unblocked. See corner case 3b.
6. **~~Does the builder accept a store configuration?~~ Resolved: yes, as a section of
   `EnvironmentConfig`.** One document configures the environment and its store. Scenario 4 is in
   scope; Phase 4 sequences it last and separably.
7. **~~The `eprintln!` removal.~~ Resolved:** Phase 5 note, no replacement diagnostic.

### Newly open, from the 2026-08-31 decisions

8. **Where does the default store factory chain come from for an application?** `with_config` takes
   the factory explicitly, which is right for `liquers-web` and `liquers-store` but makes the common
   `liquers-lib` case write out a chain to get the obvious answer. Recommendation: a `liquers-lib`
   convenience beside `default_environment_builder`, matching how the recipe-provider default is
   already handled per crate. *(Phase 2 open question 6.)*
9. **Does `liquers-web` migrate its hand-rolled `apply_store` onto `EnvironmentConfig` in this
   project, or later?** Recommendation: **later.** The rebuild path is the crate's most delicate
   code, it works, and this project already carries a readiness fix plus a configuration layer. The
   migration is what makes `EnvironmentConfig` pay off for the JavaScript target, so it should be
   filed rather than forgotten.

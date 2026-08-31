# Phase 2: Solution & Architecture - Environment Builder

## Overview

One generic environment replaces the four near-duplicates, parameterized by value type, payload type
and an **asset-manager kind** marker; the existing names survive as type aliases, so no call site
moves. An `EnvironmentBuilder` owns the construction cycle inside a single synchronous `build()`:
it constructs the environment with an empty manager slot, wraps it in an `EnvRef`, constructs the
manager with that `EnvRef` in hand, installs it, and runs startup — so no partially initialized
`EnvRef` is ever observable. `Environment::to_ref` stays public and is reimplemented over the same
path, closing the readiness hole through that door too.

> **Amended 2026-08-31**, in two passes: a factual review after the four prerequisite designs
> merged, then two maintainer decisions taken at the gate. Both are summarised in
> [`DESIGN.md`](./DESIGN.md) §Prerequisite review and §Gate decisions.
>
> The **factual** amendments: `build()` needs a metadata-version refresh step, the recipe-provider
> default is expressed with `RecipeProviderChoice`, the payload environment's panic is already fixed
> so the builder preserves rather than delivers that fix, and the layering constraint against a
> core-side `EnvironmentConfig` is void.
>
> The **decisions** changed the architecture in two places, both recorded below:
> **`to_ref` stays a trait method** (this reverses Phase 2's original Finding A1, and turns out to
> simplify the design — `build()` now delegates to it, so there is one readiness sequence rather
> than two); and **`EnvironmentConfig` is in scope**, embedding `StoreRouterConfig`, so one document
> configures the environment and its store together.

## Known-Issue Preflight

Searched: `specs/index.csv` for locally open records (`draft` / `accepted` / `in_progress`) in areas
`core/assets`, `core/context`, `core/commands`, `core/value`, `web`, `lib/*`; `specs/issues/` for
records touching environment construction, asset-manager lifecycle, the dependency manager, command
registration, and the recipe provider.

| Issue | Status | Priority | Relevance and solution impact | First? | Blocking? | Required action | Priority action |
|---|---|---|---|---|---|---|---|
| `QUEUED-MANAGER-STARTUP-READINESS` | accepted | P1 | The project itself. | n/a | no | Resolve here. | Keep P1; complexity M→L applied |
| `CORE-PAYLOAD-ENV-RECIPE-PROVIDER-PANIC` | **closed** (PR 51) | P1 | Filed during this preflight, then fixed directly: `SimpleEnvironmentWithPayload::get_recipe_provider` now falls back to `TrivialRecipeProvider` and logs to stderr. Consolidation still deletes the divergent copy — the builder **preserves** the fix and removes the per-call `eprintln!`, since the field becomes a non-optional `Arc` with no unconfigured state to report. | n/a | no | No longer an obligation; §The recipe-provider default is per-crate updated. | — |
| `STORE-CONFIG-IN-CORE` | **closed** (PR 46) | P0 | Prerequisite for document-driven setup. `store_config.rs` / `store_factory.rs` are core modules; `liquers-web` dropped `liquers-store`. Removes the layering constraint Phase 1 recorded. | n/a | no | Opens open question 4 (store config in the builder). | — |
| `RECIPE-PROVIDER-BY-NAME` | **closed** (PR 48) | P0 | `RecipeProviderChoice` in `liquers-core/src/recipes.rs` names `default` / `trivial` and yields the provider. The builder should express its default with it rather than a bare `Arc::new(TrivialRecipeProvider)`. | n/a | no | Applied in §`EnvironmentBuilder` inherent API. | — |
| `COMMAND-DECLARATION-FORMAT` | **closed** (PR 50) | P0 | `CommandDeclaration` in `liquers-core`. The builder does not touch declaration parsing; only `liquers-web`'s replay path is affected, and only internally. | n/a | no | None. | — |
| `MACRO-LEAVES-STALE-METADATA-VERSION` | **closed** (`refresh-command-metadata-versions`) | — | `Environment::to_ref` now calls `CommandMetadataRegistry::refresh_metadata_versions()` before `EnvRef::new`. `build()` bypasses `to_ref`, so it must run the same operation or reintroduce the stale-version defect. | n/a | **yes, as an invariant** | `build()` step 0; test coverage in Phase 3. | — |
| `QUEUED-MANAGER-EVICTION-RACE` | accepted (design `in_review`) | P2 | `DefaultAssetManager` cache eviction, not construction or startup. Edits the same file. | no | no | Monitor for merge conflicts only. | Keep P2 |
| `CORE-EVALUATE-PATH-CONSOLIDATION` | accepted | P1 | Duplicated evaluation paths. This design removes the five lazy `ensure_started()` calls from the inline entry points, which touches those paths. No conflict of intent — fewer per-call barriers is what consolidation wants — but the two will overlap textually. | no | no | Monitor; note the `ensure_started` removal in that issue if it starts first. | Keep P1 |
| `ENVIRONMENT-MANAGER-REFERENCE-CYCLE` | draft | P2 | Two `Arc` cycles keep every environment alive. Deferred by decision in Phase 1. The architecture keeps the back-reference strong, so it neither fixes nor worsens it. | no | no | Monitor; do not regress. `build()` is the natural future home for the fix. | Keep P2 |
| `POST-INIT-COMMAND-REGISTRATION` | accepted | P3 | Registration needs `&mut CommandRegistry`; `Arc::get_mut` never sees count 1. Unchanged by this work — the deferred slot moving to the environment does not alter the strong count. Its long-term goal (dynamic registration and metadata modification) **does** constrain us: startup must be re-runnable. | no | no | Design `refresh_command_versions` as a separate re-runnable path; do not use a one-shot cell. | Keep P3 |
| `ASSETS-FIX1` | accepted | P2 | TODO/FIXME markers in the asset lifecycle. Touches `assets.rs`, which this project edits, but no overlap with construction or startup. | no | no | Monitor for merge conflicts only. | Keep P2 |
| `INLINE-PATH-LACKS-EXECUTE-ONCE` | accepted | — | Concerns `ImmediateAssetManager`'s execution model, not its construction or startup. | no | no | Independent. | Keep |
| `EGUI-ASSET-MANAGER-INTEGRATION` | accepted | P2 | Wants a stable widget/manager adapter. Benefits from a single configuration point but does not constrain it. | no | no | Independent. | Keep P2 |
| `VALUE-TYPE-DEFINITION-MACRO` | draft | P2 | Touches the type registry, which the builder passes through unchanged. | no | no | Independent. | Keep P2 |

### Blocking and Priority Decision

**No blockers.** As of 2026-08-31 the four prerequisites recorded in `DESIGN.md` have all merged,
so nothing this design depends on is outstanding. `CORE-PAYLOAD-ENV-RECIPE-PROVIDER-PANIC` was never
a prerequisite — consolidation removes the divergent implementation rather than depending on it —
and was fixed directly in the meantime.

One **invariant** rather than a blocker: `refresh-command-metadata-versions` put a
`refresh_metadata_versions()` call at the head of `to_ref`. Because `build()` does not delegate
through `to_ref`, it must run that operation itself; otherwise this design silently reopens
`MACRO-LEAVES-STALE-METADATA-VERSION`. Recorded as step 0 of the `build()` sequence.
`ENVIRONMENT-MANAGER-REFERENCE-CYCLE` is a deliberate non-goal (Phase 1, question 3) and the
architecture is required only not to make it worse.

## Data Structures

### `AssetManagerKind` — the marker that makes consolidation nameable

The obstacle to a single generic environment is that the manager is parameterized by the
environment: `DefaultAssetManager<E>` where `E` owns an `Arc<DefaultAssetManager<E>>`. Writing the
manager as a direct type parameter produces an infinitely recursive type name
(`GenericEnvironment<V, (), DefaultAssetManager<GenericEnvironment<V, (), …>>>`). A **kind marker**
that is not itself parameterized by `E` breaks it:

```rust
/// Selects an asset-manager implementation without naming the environment it will serve.
pub trait AssetManagerKind: 'static {
    /// The manager this kind produces for environment `E`.
    type Manager<E: Environment>: AssetManager<E>;

    /// Construct the manager for an environment that already exists.
    ///
    /// Called from `GenericEnvironment::init_with_envref`, after the `EnvRef` is created and
    /// before it is observable — so on both the builder path and the `to_ref` path.
    /// Sync: see §Sync vs Async.
    ///
    /// `options` carries per-manager settings (queue capacity today, expiration tuning later).
    /// A kind that cannot honor a set field returns `Err` rather than ignoring it silently —
    /// `job_capacity` against `Inline` is a configuration mistake, not a no-op. Fallible for the
    /// same reason `start` is: so a manager whose construction can fail stays expressible.
    fn build<E: Environment>(envref: EnvRef<E>, options: &AssetManagerOptions)
        -> Result<Arc<Self::Manager<E>>, Error>;
}

/// Native queued execution: `DefaultAssetManager`, job queue plus expiration monitor.
#[cfg(not(target_arch = "wasm32"))]
pub struct Queued;

/// Spawn-free inline execution: `ImmediateAssetManager`. The only kind available on wasm.
pub struct Inline;

/// Per-manager construction settings. Every field optional; a kind rejects what it cannot honor.
///
/// Serde-able so a future `EnvironmentConfig` (in `liquers-store` — see §Integration Points) can
/// carry it. `liquers-core` owns the struct but never reads a configuration file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssetManagerOptions {
    /// Job-queue capacity for a queued kind. `DefaultAssetManager` currently hardcodes 4.
    #[serde(default)] pub job_capacity: Option<usize>,
}

/// The kind used when none is named: queued natively, inline on wasm.
///
/// Default for both `EnvironmentBuilder` and `GenericEnvironment`, so
/// `EnvironmentBuilder::<Value>::new()` is correct on every target. It also replaces
/// `liquers-lib`'s `SelectedAssetManager` cfg-import pair, which emulated exactly this.
#[cfg(not(target_arch = "wasm32"))] pub type DefaultKind = Queued;
#[cfg(target_arch = "wasm32")]      pub type DefaultKind = Inline;
```

`type Manager<E>` is a generic associated type — stable since Rust 1.65; the workspace is edition
2021 on 1.94, so this is available. `AssetManagerKind` is deliberately **not** object-safe (it has a
generic method and a GAT); it is a compile-time selector, never a `dyn`.

**Ownership rationale:** `build` returns `Arc<_>` because the environment stores the manager as
`Arc` and hands out clones through `get_asset_manager`. `EnvRef<E>` is taken by value because the
manager stores it (strong, per Phase 1 question 3).

**Serialization:** none. These are zero-sized markers; a future `EnvironmentConfiguration` maps a
string like `"queued"` onto a kind at the call site, not through `serde` on this trait.

### `GenericEnvironment`

```rust
pub struct GenericEnvironment<V: ValueInterface,
                              P: PayloadType = (),
                              K: AssetManagerKind = DefaultKind> {
    type_registry: TypeRegistry,
    async_store: Arc<dyn AsyncStore>,
    pub command_registry: CommandRegistry<Self>,
    /// Written exactly once by `EnvironmentBuilder::build`, before any `EnvRef` is observable.
    asset_store: OnceLock<Arc<K::Manager<Self>>>,
    recipe_provider: Arc<dyn AsyncRecipeProvider<Self>>,
    /// Read by `init_with_envref` when it constructs the manager. Lives on the environment rather
    /// than staying in the builder because the hook is what constructs the manager, and the hook
    /// only has `&self`. Added 2026-08-31 with the `to_ref` decision.
    manager_options: AssetManagerOptions,
    _payload: PhantomData<P>,
}
```

Differences from the five structs it replaces, each deliberate:

- **The legacy `store: Arc<dyn Store>` field is dropped.** Its own doc says it "is not exposed
  through `Environment` and is not used by the asset manager". Two of the five carry it; it is dead
  weight. `with_store` and the always-panicking `with_cache` go with it.
- **`recipe_provider` is `Arc<…>`, not `Option<Arc<…>>`.** The builder resolves the default once, at
  build time. This is what removes `CORE-PAYLOAD-ENV-RECIPE-PROVIDER-PANIC` by construction:
  there is no unconfigured state left to panic on. **The default is per-crate, not global** — see
  below; a single global default would silently regress `liquers-lib`.
- **`asset_store` is a `OnceLock`** — the deferred slot moved from the manager to the environment
  (Phase 1, question 2). The manager gains a plain strong `EnvRef` field and loses its
  `OnceLock<EnvRef<E>>` and its `"Environment not set"` panic entirely.
- **`command_registry` stays a public field**, so `register_command!` and `CommandRegistryAccess`
  keep working during the builder phase.

**Ownership rationale:** `async_store` and `recipe_provider` are `Arc<dyn …>` because they are
shared, cheaply cloned, and returned by value from `Environment` accessors. `type_registry` is owned
and never written after construction, which is what lets `get_type_registry` return `&` with no
lock. `PhantomData<P>` carries the payload type, which appears only in the associated type.

### Compatibility aliases

Every existing name survives, so no call site changes:

```rust
#[cfg(not(target_arch = "wasm32"))]
pub type SimpleEnvironment<V>             = GenericEnvironment<V, (), Queued>;
#[cfg(not(target_arch = "wasm32"))]
pub type SimpleEnvironmentWithPayload<V, P> = GenericEnvironment<V, P, Queued>;
pub type ImmediateEnvironment<V>          = GenericEnvironment<V, (), Inline>;
pub type ImmediateEnvironmentWithPayload<V, P> = GenericEnvironment<V, P, Inline>;
```

In `liquers-lib`, the `SelectedAssetManager` cfg-import pair disappears entirely — `DefaultKind`
already performs that selection in `liquers-core`:

```rust
pub type DefaultEnvironment<V, P = ()> = GenericEnvironment<V, P>;   // K = DefaultKind
```

**There is one builder type, not one per environment.** Brevity comes from the default type
parameters above, so the ordinary call is `EnvironmentBuilder::<Value>::new()` and a parameter is
written only when it is being chosen. Convenience aliases such as `SimpleEnvironmentBuilder<V>` are
deliberately *not* provided: a builder name per environment would read as a builder family and
re-suggest the duplication this design removes.

`DefaultEnvironment`'s extra surface needs care, and this is where the alias approach has a real
constraint. A type alias creates no new type, so `GenericEnvironment<…>` stays a **foreign** type in
`liquers-lib`, and Rust permits an inherent `impl` only in the defining crate. Therefore:

- `CommandRegistryAccess` — **fine as is.** A local trait may be implemented for a foreign type.
- `register_polars_commands` — **currently an inherent method** on `DefaultEnvironment<Value>`
  (`liquers-lib/src/environment.rs:110`, called as `env.register_polars_commands()` in
  `liquers-lib/tests/polars_commands.rs:19`). It **cannot** remain inherent. It becomes a method on
  a small local extension trait:

  ```rust
  #[cfg(feature = "polars")]
  pub trait PolarsCommandRegistration { fn register_polars_commands(&mut self) -> Result<(), Error>; }
  ```

  The call site is unchanged once the trait is in scope, and the underlying
  `register_polars_commands!` macro — which the exporter and `registry_export` test already use
  directly — is untouched.

`liquers-lib` defines no kind of its own; it uses `DefaultKind` from `liquers-core`. No impl there
requires locality of the kind.

### `EnvironmentConfig` — one document for the environment and its store

**In scope by maintainer decision, 2026-08-31.** Phase 1 placed a single configuration point in
*Future Direction*, out of scope, because `StoreRouterConfig` could not be embedded from
`liquers-core`. `STORE-CONFIG-IN-CORE` removed that constraint and `RECIPE-PROVIDER-BY-NAME` supplied
the last field that could not be expressed as data, so the goal is now committed: **one file or JSON
structure configures both the environment and its store.**

```rust
/// Everything about an environment that can be written down rather than compiled in.
///
/// Commands are Rust functions registered by a macro, so no document can name one: a configuration
/// configures *services*, and code registers *commands*. The builder splits along exactly that
/// line — the `with_*` setters are the config-drivable half, the public `command_registry` field is
/// the code-only half.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnvironmentConfig {
    /// Store router, verbatim `StoreRouterConfig`. Reused, not re-specified.
    #[serde(default)]
    pub store: StoreRouterConfig,

    /// Which built-in recipe provider. Absent means [`RecipeProviderChoice::Default`] — the
    /// *document* default that `recipe-provider-selection` deliberately chose, on the grounds that
    /// a configuration saying nothing about recipes most plausibly wants them to work. Note this
    /// is **not** the core builder's unconfigured default, which is `Trivial`; applying a config is
    /// an explicit act, and it says so.
    #[serde(default)]
    pub recipes: RecipeProviderChoice,

    /// Per-manager settings. The manager *kind* is not here — see below.
    #[serde(default)]
    pub assets: AssetManagerOptions,
}

impl EnvironmentConfig {
    pub fn from_yaml(yaml: &str) -> Result<Self, Error>;
    pub fn from_json(json: &str) -> Result<Self, Error>;
    pub fn from_toml(toml: &str) -> Result<Self, Error>;
    pub fn to_yaml(&self) -> Result<String, Error>;
    pub fn to_json(&self) -> Result<String, Error>;

    /// Expand `${VAR}` references in the store section.
    pub fn expand_env_vars(&mut self) -> Result<(), Error>;
}
```

Mirrors `StoreRouterConfig`'s own surface (`from_yaml` / `from_json` / `from_toml` / `to_*` /
`expand_env_vars`) rather than inventing a second idiom for the same job, and delegates the store
half to it outright.

**The manager kind stays a type parameter and is deliberately absent from the document.** A YAML
string cannot select a type: `"queued"` and `"inline"` produce two different concrete environment
types, and `Environment` is not object-safe (associated types, `Sized`), so they cannot be erased
behind a `dyn`. The choice is a build fact, not a deployment one — wasm has no choice at all, and
natively `Inline` exists for deterministic testing rather than production tuning. An application
that genuinely wants runtime selection monomorphizes its own tail with an explicit two-arm match;
Phase 3 §Scenario 4 shows it.

**Store factories are an argument, not a field.** `StoreRouterBuilder` requires a factory because it
has no store types of its own, and which backends exist is a build fact for the same reason the kind
is: `liquers-core` supplies `default_store_factory()` (memory, plus filesystem off wasm),
`liquers-store` chains OpenDAL onto it, `liquers-web` chains its own `WebStoreFactory`. So the
factory reaches the builder through a setter, and the config document names store *types* the
factory chain is expected to resolve — with `unknown_store_type_error` already producing an accurate
"supported types" message for the build in hand.

### `EnvironmentBuilder`

```rust
pub struct EnvironmentBuilder<V: ValueInterface,
                              P: PayloadType = (),
                              K: AssetManagerKind = DefaultKind> {
    type_registry: TypeRegistry,
    async_store: Arc<dyn AsyncStore>,
    /// Public field, mirroring the environments' existing `pub command_registry`. This is what
    /// keeps the 120 `&mut env.command_registry` sites to a one-word rename of the receiver.
    pub command_registry: CommandRegistry<GenericEnvironment<V, P, K>>,
    recipe_provider: Option<Arc<dyn AsyncRecipeProvider<GenericEnvironment<V, P, K>>>>,
    /// A store configuration not yet built, plus the factory chain that will resolve it.
    /// Alternative to `async_store`; `build()` resolves whichever is set. Added 2026-08-31.
    store_config: Option<(StoreRouterConfig, Box<dyn StoreFactory>)>,
    manager_options: AssetManagerOptions,
    _payload: PhantomData<P>,
    _kind: PhantomData<K>,
}
```

`recipe_provider` is `Option` **here** and resolved to a concrete default in `build()`; the
environment never sees `None`. That is the whole of the change that retires the panic.

## Trait Implementations

### The recipe-provider default is per-crate

Consolidation cannot use one default provider, because the two crates disagree today and both are
correct for their audience:

| Constructor | Default provider today (post-PR 51) |
|---|---|
| `SimpleEnvironment`, `ImmediateEnvironment`, `ImmediateEnvironmentWithPayload` | `TrivialRecipeProvider` |
| `SimpleEnvironmentWithPayload` | `TrivialRecipeProvider`, plus an `eprintln!` on **every** call |
| `liquers_lib::DefaultEnvironment` (`environment.rs:77`) | **`DefaultRecipeProvider`** |

**Amended 2026-08-31.** This table originally recorded `SimpleEnvironmentWithPayload` as *panicking*;
`payload-env-recipe-provider-fallback` (PR 51) fixed that before this design started implementing,
so the four core environments now agree. Two consequences: the builder **preserves** a fix rather
than delivering one, and the warning `eprintln!` disappears with it — a non-optional `Arc` field has
no unconfigured state to warn about, so the diagnostic has nothing left to say.

`DefaultRecipeProvider` reads recipes through the store; `TrivialRecipeProvider` resolves none. So
if `DefaultEnvironment` became an alias whose builder defaulted to `Trivial`, every `-R/` query in
an application that relied on the library default would start failing `KeyNotFound` — a silent
behavior regression, invisible at compile time.

**Resolution.** `EnvironmentBuilder::new` in `liquers-core` defaults to
`RecipeProviderChoice::Trivial`, unchanged for all four core environments. `liquers-lib` supplies
its own pre-configured constructor rather than relying on the core default:

```rust
// liquers-lib
pub fn default_environment_builder<V: ValueInterface, P: PayloadType>()
    -> EnvironmentBuilder<V, P, DefaultKind>
{
    EnvironmentBuilder::new().with_recipe_provider_choice(RecipeProviderChoice::Default)
}
```

Both defaults are now written as a `RecipeProviderChoice` — the serde enum
`recipe-provider-selection` (PR 48) added to `liquers-core/src/recipes.rs` — rather than as a bare
`Arc::new(TrivialRecipeProvider)`. That is a small change with two payoffs: the default becomes a
value that can be printed, compared and asserted on in T9, and it is the same value a future
`EnvironmentConfig` would deserialize, so no second spelling of "which provider" appears.

Every existing behavior is preserved, the divergence becomes explicit and testable rather than an
accident of which constructor was called, and `liquers-lib` gains the natural home for any future
library-level default. Test **T9** is extended to assert *both* defaults, so a later change that
collapses them fails.

### `EnvironmentBuilder` inherent API

```rust
impl<V: ValueInterface, P: PayloadType, K: AssetManagerKind> EnvironmentBuilder<V, P, K> {
    /// Type registry from `V::type_descriptions()`, no store, no recipe provider.
    pub fn new() -> Self;

    /// For an integration adding a type `V` cannot describe statically (a `js.Value` handle).
    /// Extend `TypeRegistry::from_value_type`; starting from `TypeRegistry::new()` loses the
    /// `error` pseudo-type that even a failed asset needs.
    pub fn with_type_registry(self, registry: TypeRegistry) -> Self;

    pub fn with_async_store(self, store: Arc<dyn AsyncStore>) -> Self;
    pub fn with_recipe_provider(
        self,
        provider: Arc<dyn AsyncRecipeProvider<GenericEnvironment<V, P, K>>>,
    ) -> Self;

    /// Build the store from a configuration document plus a factory chain, instead of passing an
    /// already-constructed store. Construction is deferred to `build()` so this setter stays
    /// infallible and chainable; a bad configuration or an unresolved store type surfaces there.
    ///
    /// `${VAR}` expansion runs at `build()` time. Use `with_store_config_unexpanded` where there
    /// are no environment variables to expand — a browser page — mirroring
    /// `StoreRouterBuilder::build_without_env_expansion`.
    pub fn with_store_config(
        self,
        config: StoreRouterConfig,
        factory: Box<dyn StoreFactory>,
    ) -> Self;
    pub fn with_store_config_unexpanded(
        self,
        config: StoreRouterConfig,
        factory: Box<dyn StoreFactory>,
    ) -> Self;

    /// Apply a whole configuration document: store, recipes and manager options at once.
    ///
    /// Equivalent to the three matching setters, so it composes with hand-written configuration in
    /// either order — document first then overridden in code, or the reverse. Commands are the
    /// caller's job either way.
    pub fn with_config(self, config: EnvironmentConfig, factory: Box<dyn StoreFactory>) -> Self;

    /// Select a built-in provider by name — the data-expressible half, added 2026-08-31.
    /// `RecipeProviderChoice` (`liquers-core/src/recipes.rs`, from `RECIPE-PROVIDER-BY-NAME`)
    /// already yields the provider, so this is `with_recipe_provider(choice.provider())`.
    /// It exists so a configuration document and hand-written code spell the choice identically.
    pub fn with_recipe_provider_choice(self, choice: RecipeProviderChoice) -> Self;

    pub fn with_asset_manager_options(self, options: AssetManagerOptions) -> Self;

    /// Construct, install, and start. Returns an `EnvRef` that is ready to evaluate.
    pub fn build(self) -> Result<EnvRef<GenericEnvironment<V, P, K>>, Error>;
}

impl<V: ValueInterface, P: PayloadType, K: AssetManagerKind> Default
    for EnvironmentBuilder<V, P, K> { fn default() -> Self { Self::new() } }
```

Setters take `self` and return `Self` — the by-value builder idiom — rather than today's
`&mut self -> &mut Self`, which forces the awkward `let mut env = …; env.with_x(…); env.to_ref()`
dance at every call site. `command_registry` stays a **public field** rather than becoming an
accessor: `register_command!` needs a `&mut CommandRegistry` and cannot be threaded through a
by-value chain, and keeping the field mirrors the environments' current shape so the 120 existing
`&mut env.command_registry` sites migrate by renaming the receiver, not by restructuring.

`build()` returns `Result` even though today's startup cannot fail. The fallible signature is the
cheap half of the issue's "startup failures should be returned to the caller"; making it infallible
now would be the breaking change later.

**`build()` sequence** (signatures only; bodies are Phase 4). **Revised 2026-08-31**: the sequence
is no longer written out here, because `build()` no longer implements it — it configures an
environment and hands it to `Environment::try_to_ref`, which owns the readiness sequence for every
door (see §`Environment` below).

1. Resolve the recipe provider: configured, else `RecipeProviderChoice::Trivial`.
2. Resolve the store: a configured `Arc<dyn AsyncStore>`, or one built from a configured
   `StoreRouterConfig` plus its factory chain, else `NoAsyncStore`. Sync — `StoreRouterBuilder::build`
   is a synchronous function, verified at `HEAD`.
3. Construct `GenericEnvironment` with `asset_store: OnceLock::new()`.
4. `env.try_to_ref()` — which refreshes command metadata versions, creates the `EnvRef`, and calls
   `init_with_envref`; `GenericEnvironment`'s implementation constructs the manager with that
   `EnvRef` via `K::build`, installs it into the `OnceLock`, and starts it.

Steps 3–4 are the only window in which the environment's manager slot is empty, and no `EnvRef`
escapes during it. That is the entire readiness guarantee, and it is now stated in exactly one place.

The step-0 metadata refresh recorded earlier in this document is **not** a separate builder
operation: it lives in `try_to_ref`'s default body, where `refresh-command-metadata-versions` put
it, so `build()` inherits it rather than duplicating it. Its ordering constraint — refresh must
precede the startup that snapshots those versions — is satisfied by the sequence above.

### `AssetManager` — startup becomes sync and re-runnable

```rust
pub trait AssetManager<E: Environment>: MaybeSend + MaybeSync {
    // REMOVED: fn set_envref(&self, envref: EnvRef<E>);
    //   The manager receives its EnvRef at construction; there is no unset state to fill.

    // WAS: async fn start(&self);
    /// Idempotent startup. Registers command metadata and implementation versions.
    ///
    /// Synchronous: the work is uncontended in-memory map writes (§Sync vs Async).
    /// Fallible so a manager whose startup can fail is expressible without a breaking change.
    fn start(&self) -> Result<(), Error>;

    /// Refresh the command metadata registry's versions, then re-register them.
    ///
    /// Two operations, in order: `CommandMetadataRegistry::refresh_metadata_versions` (the same
    /// call `build()` makes at step 0, so a metadata edit is reflected in `metadata_version`),
    /// then re-registration into the `DependencyManager`. Refreshing without re-registering
    /// changes nothing observable; re-registering without refreshing re-registers stale versions.
    ///
    /// Separate from `start` because it is **not** a readiness operation: it exists so a later
    /// command registration or metadata edit can be reflected. Re-registering a changed version
    /// makes `DependencyManager::register_version` return the dependents to expire, so this is the
    /// hook `POST-INIT-COMMAND-REGISTRATION` needs. Callable any number of times.
    fn refresh_command_versions(&self) -> Result<(), Error>;

    /// Whether `start` has completed at least once. The observable readiness boundary the issue
    /// asks for.
    fn is_started(&self) -> bool;

    // unchanged: get_asset, apply, get, dependency_manager, eval_mode, …
}
```

`ImmediateAssetManager::ensure_started`'s `tokio::sync::OnceCell` becomes a `std::sync::atomic::AtomicBool`
(or `OnceLock<()>`): a one-shot async cell would foreclose `refresh_command_versions`, and it no
longer needs to be async. The lazy `ensure_started()` calls at the five inline entry points are
removed — `build()` has already started the manager, so lazily re-checking on every call is dead
weight once the readiness boundary exists.

**No default match arm** is introduced anywhere by this change; `EvalMode` and `Status` matches are
untouched.

### `DependencyManager` — a sync registration path

```rust
impl<E: Environment> DependencyManager<E> {
    /// Synchronous counterpart of `register_version`, for the uncontended startup path.
    ///
    /// Uses `scc::HashMap::entry_sync`. Returns the dependents to expire, exactly as the async
    /// form does, so `refresh_command_versions` gets correct cascade behavior on a changed version.
    pub fn register_version_sync(&self, key: &DependencyKey, version: Version)
        -> ExpiredDependents<E>;
}
```

`load_command_versions` gains a sync sibling built on it. The async `register_version` stays for
every other caller.

**Cascade caveat.** At first `start()` every key is `Vacant`, so nothing expires and the sync path
is trivially correct. `refresh_command_versions` can produce a non-empty `ExpiredDependents`, and
`expire_dependencies_result` is async. Phase 4 must therefore either keep
`refresh_command_versions` returning the `ExpiredDependents` for an async caller to apply, or give
it a sync application path. **Recorded as open question 1** — it is the one place where "sync
startup" leaks.

### `Environment` — `to_ref` stays on the trait, `init_with_envref` is strengthened

**Reversed by maintainer decision, 2026-08-31.** The draft removed both methods from the trait and
made `to_ref` an inherent method on `GenericEnvironment`, deprecated (Finding A1). The decision is
that the builder is the *recommended* construction path, not the only one: an ad-hoc,
user-implemented `Environment` still needs `to_ref` or an equivalent, so it stays on the trait and
is not deprecated. Phase out in-tree call sites where it is cheap and sensible; do not force a
migration.

That reversal makes the design **smaller**, not larger, because it removes the second readiness
sequence:

```rust
pub trait Environment: Sized + MaybeSync + MaybeSend + 'static {
    // … associated types unchanged …

    // CHANGED: sync (was: arrange an async start), fallible (was: infallible),
    //          and the contract is strengthened.
    /// Construct, install and **start** this environment's asset manager.
    ///
    /// Called once by [`Self::try_to_ref`], with an `EnvRef` that is not yet observable by anyone
    /// else. On return the manager must be fully usable: constructed with this `EnvRef`, installed
    /// in the environment, and started. That obligation is the whole readiness guarantee, and it
    /// is why this hook is the one thing a custom environment must get right.
    ///
    /// The expected shape is the deferred-slot pattern `GenericEnvironment` uses: hold the manager
    /// in a `OnceLock`, construct it here with the `EnvRef` in hand, install, start. A manager that
    /// needs no back-reference may ignore the argument.
    fn init_with_envref(&self, envref: EnvRef<Self>) -> Result<(), Error>;

    /// Consumes, shares and initializes this environment. Fallible form.
    ///
    /// Refreshes command metadata versions, creates the `EnvRef`, then hands it to
    /// `init_with_envref` before returning it. No `EnvRef` escapes this function before the hook
    /// has run, so the value it returns is ready to evaluate.
    fn try_to_ref(mut self) -> Result<EnvRef<Self>, Error> {
        self.get_mut_command_metadata_registry().refresh_metadata_versions();
        let envref = EnvRef::new(self);
        envref.0.init_with_envref(envref.clone())?;
        Ok(envref)
    }

    /// Consumes, shares and initializes this environment.
    ///
    /// Signature-compatible with today's `to_ref`, so all 348 call sites are unchanged. Panics if
    /// startup fails — which cannot happen with either built-in manager, since startup writes an
    /// in-memory map. Prefer [`Self::try_to_ref`] or `EnvironmentBuilder::build` where the error
    /// matters.
    fn to_ref(self) -> EnvRef<Self> { /* try_to_ref, panicking on Err */ }
}
```

**Why this is better than the inherent-method version it replaces.** Finding A1's objection was that
a defaulted trait method cannot construct a builder for an arbitrary implementor. True, and beside
the point: the default body does not need a builder. It needs the *sequence* — refresh, wrap,
initialize — and the one step that varies per environment is exactly what `init_with_envref`
already abstracts. Keeping the hook keeps the sequence generic.

The consequence is that **`EnvironmentBuilder::build` delegates to `try_to_ref`** rather than
reimplementing the sequence beside it:

```rust
pub fn build(self) -> Result<EnvRef<GenericEnvironment<V, P, K>>, Error> {
    // 1. Resolve services (recipe provider, store, manager options) into the environment.
    // 2. GenericEnvironment { asset_store: OnceLock::new(), … }
    // 3. env.try_to_ref()      <- the whole readiness sequence, shared with every other door
}
```

Three things follow, and they are the argument for the decision:

- **There is one readiness sequence, not two.** The original design had `build()` doing
  refresh-construct-install-start and `to_ref` doing it again through a different path; two
  implementations of one guarantee is how the next `to_ref`-shaped hole gets introduced.
- **The step-0 refresh invariant is satisfied structurally.** `refresh_metadata_versions()` lives in
  `try_to_ref`'s default body, where `refresh-command-metadata-versions` already put it. `build()`
  cannot forget it, because `build()` does not implement it. This is what `DESIGN.md` anticipated:
  *"if the eventual builder delegates through the refreshed `to_ref` path, no separate builder
  operation is needed."*
- **A custom environment gets the same guarantee**, provided it implements `init_with_envref`
  correctly — which is now a documented, single-obligation contract rather than the current
  "call `set_envref`, then spawn `start` and hope".

`GenericEnvironment`'s own implementation is where the kind is used:

```rust
impl<V, P, K> Environment for GenericEnvironment<V, P, K> {
    fn init_with_envref(&self, envref: EnvRef<Self>) -> Result<(), Error> {
        let manager = K::build(envref, &self.manager_options)?;
        // OnceLock::set takes &self; build() holds the only EnvRef, so this cannot be already-set.
        let _ = self.asset_store.set(manager.clone());
        manager.start()
    }
}
```

### `EnvRef::new` — still deprecated

```rust
impl<E: Environment> EnvRef<E> {
    #[deprecated(note = "produces an EnvRef with no asset manager installed; use \
                         Environment::to_ref or EnvironmentBuilder::build")]
    pub fn new(env: E) -> Self;
}
```

This one keeps its deprecation. `to_ref` is a *correct* door and stays open; `EnvRef::new` is the
actual hole — it hands out an `EnvRef` whose manager was never installed or started, which is the
`DOC_04` P0 gap row. It has exactly one in-tree caller (`try_to_ref`'s default body), so deprecating
it costs nothing and it can become `pub(crate)` after a release. A custom environment does not need
it: the default `try_to_ref` body calls it on the implementor's behalf.

### Hiding the remaining doors — **withdrawn**

Phase 1's refinement (question 4) proposed making the built-in environments' constructors
`pub(crate)` and dropping their public `Default` impls, so the builder would be the only way to
obtain an owned environment. **Withdrawn by the same decision.** If `to_ref` is a supported path for
ad-hoc environments, then constructing an environment to call it on must stay possible; and the
`pub(crate)` proposal was in direct tension with "336 `to_ref` sites keep working" for every caller
outside `liquers-core`, its own `tests/` included (Phase 3, open question 5 — now resolved by this).

Constructors stay `pub`. The `Default` impls may still go, since nothing in tree calls them, but
that is cosmetic rather than a door being closed.

So there are two supported paths to an `EnvRef`, both running the same sequence:

| Path | For | Fallible |
|---|---|---|
| `EnvironmentBuilder::build()` | applications and integrations — the recommended, documented path | yes |
| `Environment::to_ref()` / `try_to_ref()` | an ad-hoc or user-implemented environment | `to_ref` panics; `try_to_ref` returns |

**Phasing out, where cheap.** The guide and `DOC_04` document `build()` as the recommended path;
in-tree call sites migrate opportunistically when the surrounding code is being touched anyway, not
as a mass rename. No deprecation attribute on `to_ref`, so no warning storm across 348 sites and no
churn in the test suites.

## Sync vs Async Decisions

| Operation | Decision | Rationale |
|---|---|---|
| `EnvironmentBuilder::build` | **sync**, fallible | Phase 1 option A. Callable from `main`, from a plain `#[test]`, from a wasm entry point. |
| `AssetManager::start` | **sync**, fallible | Its only `await` was `scc::entry_async`. At startup the `versions` map is empty and uncontended, every key inserts `Vacant`, so `version_changed` is always false and no cascade can fire. No store is touched: `load_from_records` is reached from asset recovery and `track_asset`, never from `start`. |
| `refresh_command_versions` | **sync**, fallible, returns work | Same map writes, but a changed version *can* cascade — see the caveat above and open question 1. |
| `Environment::try_to_ref` / `to_ref` | **sync**, fallible / panicking | Owns the readiness sequence for every door; `build()` delegates to it. |
| `Environment::init_with_envref` | **sync**, fallible | Was infallible and spawned a detached async `start`. That detachment is the defect. |
| Store construction from a `StoreRouterConfig` | **sync** | `StoreRouterBuilder::build` and `build_without_env_expansion` are synchronous functions at `HEAD`, and every `StoreFactory::create` returns a constructed store rather than a future. So accepting a store *configuration* does not make `build()` async — verified rather than assumed, since it would otherwise reopen Phase 1's option A. |
| `AsyncRecipeProvider` | **unchanged** | See §Recipe Provider. |
| Everything else | unchanged | No evaluation path changes async-ness. |

**Sync is not runtime-free for `Queued`.** `DefaultAssetManager::with_capacity` calls `tokio::spawn`
twice, so `Queued::build` still requires an active Tokio runtime. `Inline::build` spawns nothing and
is genuinely runtime-free — which is what wasm needs. The builder documents this per kind rather
than promising it globally.

## Recipe Provider

`AsyncRecipeProvider<E>` takes `envref: EnvRef<E>` on every method — the third of three ways this
codebase solves "component needs the environment" (Phase 1). **Decision: leave the signatures
unchanged in this project.** Rationale:

- It is the only one of the three that is actually correct: no back-reference, so no cycle and no
  readiness hole.
- Changing ~8 trait methods plus every implementor and call site is a large diff whose benefit
  (a provider that can hold the environment) has no current consumer.
- The builder makes it *possible* later without another redesign: a `with_recipe_provider_factory(
  impl FnOnce(EnvRef<Env>) -> Arc<dyn AsyncRecipeProvider<Env>>)` slots into step 1 of `build()`
  exactly as `K::build` slots into step 4.

What this project does contribute is that the inconsistency is now written down (Phase 1
§Recipe Provider) with the builder positioned as the place to resolve it.

**Amended 2026-08-31.** `RECIPE-PROVIDER-BY-NAME` closed while this design waited, adding
`RecipeProviderChoice` to `liquers-core/src/recipes.rs`: a `Default` / `Trivial` serde enum with
`provider()`, `boxed_provider()`, `FromStr` and `Display`. It changes none of the reasoning above —
the *trait* signatures still take `envref` per call, and that decision stands — but it does supply
the vocabulary the builder should use for selecting a built-in provider, which is why
`with_recipe_provider_choice` joins the API. A provider that needs the environment at construction
time is still future work, still reachable through the `with_recipe_provider_factory` hook named
above, and still has no consumer.

## Function Signatures

Consolidated list of every signature this project adds, changes or removes. Bodies are Phase 4.

### Added

```rust
// liquers-core/src/environment_builder.rs
pub trait AssetManagerKind: 'static {
    type Manager<E: Environment>: AssetManager<E>;
    fn build<E: Environment>(envref: EnvRef<E>, options: &AssetManagerOptions)
        -> Result<Arc<Self::Manager<E>>, Error>;
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssetManagerOptions { pub job_capacity: Option<usize> }

// Defaults P = (), K = DefaultKind, so `EnvironmentBuilder::<Value>::new()` is the ordinary call.
impl<V: ValueInterface, P: PayloadType, K: AssetManagerKind> EnvironmentBuilder<V, P, K> {
    pub fn new() -> Self;
    pub fn with_type_registry(self, registry: TypeRegistry) -> Self;
    pub fn with_async_store(self, store: Arc<dyn AsyncStore>) -> Self;
    pub fn with_recipe_provider(
        self,
        provider: Arc<dyn AsyncRecipeProvider<GenericEnvironment<V, P, K>>>,
    ) -> Self;
    pub fn with_recipe_provider_choice(self, choice: RecipeProviderChoice) -> Self;
    pub fn with_asset_manager_options(self, options: AssetManagerOptions) -> Self;
    pub fn with_store_config(self, config: StoreRouterConfig, factory: Box<dyn StoreFactory>) -> Self;
    pub fn with_store_config_unexpanded(self, config: StoreRouterConfig, factory: Box<dyn StoreFactory>) -> Self;
    pub fn with_config(self, config: EnvironmentConfig, factory: Box<dyn StoreFactory>) -> Self;
    pub fn build(self) -> Result<EnvRef<GenericEnvironment<V, P, K>>, Error>;
}

// liquers-core/src/environment_config.rs — one document for the environment and its store
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnvironmentConfig {
    pub store: StoreRouterConfig,
    pub recipes: RecipeProviderChoice,
    pub assets: AssetManagerOptions,
}
impl EnvironmentConfig {
    pub fn from_yaml(yaml: &str) -> Result<Self, Error>;
    pub fn from_json(json: &str) -> Result<Self, Error>;
    pub fn from_toml(toml: &str) -> Result<Self, Error>;
    pub fn to_yaml(&self) -> Result<String, Error>;
    pub fn to_json(&self) -> Result<String, Error>;
    pub fn expand_env_vars(&mut self) -> Result<(), Error>;
}

// liquers-core/src/context.rs — Environment
fn try_to_ref(self) -> Result<EnvRef<Self>, Error>;   // provided; owns the readiness sequence
}

// liquers-core/src/assets.rs — AssetManager<E>
fn refresh_command_versions(&self) -> Result<(), Error>;
fn is_started(&self) -> bool;

// liquers-core/src/dependencies.rs — DependencyManager<E>
pub fn register_version_sync(&self, key: &DependencyKey, version: Version)
    -> ExpiredDependents<E>;
pub(crate) fn load_command_versions_sync<E: Environment>(
    dm: &DependencyManager<E>,
    cmr: &CommandMetadataRegistry,
);

// liquers-core/src/assets.rs — manager constructors take the EnvRef
impl<E: Environment> DefaultAssetManager<E> {
    pub fn new(envref: EnvRef<E>) -> Self;
    pub fn with_capacity(envref: EnvRef<E>, capacity: usize) -> Self;
}
impl<E: Environment> ImmediateAssetManager<E> {
    pub fn new(envref: EnvRef<E>) -> Self;
}
```

### Changed

```rust
// AssetManager<E>
- async fn start(&self);
+ fn start(&self) -> Result<(), Error>;

// AssetManager<E> — get_envref no longer has an unset state to guard
  fn get_envref(&self) -> EnvRef<E>;   // signature unchanged; the panic path disappears

// Environment — the hook stays, its contract is strengthened (sync, fallible, must start)
- fn init_with_envref(&self, envref: EnvRef<Self>);
+ fn init_with_envref(&self, envref: EnvRef<Self>) -> Result<(), Error>;

// Environment — to_ref keeps its signature; its provided body now delegates to try_to_ref
  fn to_ref(self) -> EnvRef<Self>;     // unchanged, not deprecated, 348 call sites untouched
```

### Removed

```rust
// AssetManager<E>
- fn set_envref(&self, envref: EnvRef<E>);   // the manager gets its EnvRef at construction

// GenericEnvironment (were on each of the four structs)
- pub fn with_store(&mut self, store: Box<dyn Store>) -> &mut Self;
- pub fn with_cache(&mut self, _cache: Box<dyn Cache<V>>) -> &mut Self;   // always panicked
- impl Default for …                                                     // no in-tree caller
```

**Revised 2026-08-31.** This list previously removed `Environment::init_with_envref` and
`Environment::to_ref`. Both stay, per the maintainer decision recorded in §`Environment`;
constructors stay `pub`, so the `Default` impls go only because nothing calls them, not to close a
door.

### Deprecated, kept

```rust
impl<E: Environment> EnvRef<E> {
    #[deprecated(note = "produces an EnvRef with no asset manager installed; \
                         use Environment::to_ref or EnvironmentBuilder::build")]
    pub fn new(env: E) -> Self;
}
```

`EnvRef::new` is the only deprecation. `to_ref` is a correct, supported path and carries no
attribute — a `#[deprecated]` there would warn at 348 call sites for a method the project intends to
keep.

**Bounds justification.** `V: ValueInterface`, `P: PayloadType` and `K: AssetManagerKind` are each
required by a field or an associated type — none is speculative. `AssetManagerKind: 'static` mirrors
`Environment: 'static`, which the manager already requires. `AssetManagerKind::build` is generic over
`E` rather than over `Self`, keeping the kind a zero-sized selector; the trait is intentionally not
object-safe and is never used as `dyn`.

## Error Handling

All errors are `liquers_core::error::Error`, constructed with typed constructors — no new error
type, no `Error::new`.

| Failure | Constructor | Where |
|---|---|---|
| Asset-manager startup failed | `Error::general_error(…)` | `AssetManager::start`, propagated by `build()` with `?` |
| Version refresh failed | `Error::general_error(…)` | `refresh_command_versions` |
| Manager slot already installed | not an error — unreachable | `build()` holds the only `EnvRef`; a `debug_assert!` documents the invariant rather than a runtime branch |
| Option the kind cannot honor (`job_capacity` on `Inline`) | `Error::general_error(…)` | `AssetManagerKind::build`, propagated by `build()`. Explicitly *not* silently ignored |

Neither `start` nor `refresh_command_versions` can fail today: both write an in-memory map. The
`Result` exists so that a manager whose startup *can* fail — one restoring a persisted dependency
graph from a store — is expressible without a breaking change. Phase 4 must not "simplify" these to
infallible; the fallible signature is the point.

**No `unwrap()` / `expect()` in any of this code.** Two existing `expect`s are deleted rather than
relocated: `DefaultAssetManager::get_envref`'s `"Environment not set in AssetStore"` and
`ImmediateAssetManager::envref`'s `"Environment not set in ImmediateAssetManager"`. Both exist only
because the back-reference could be unset; once it is a constructor parameter, the state they guard
cannot occur. `GenericEnvironment::get_asset_manager` reads the environment-side `OnceLock`, whose
unset state is likewise unreachable — `build()` writes it before any `EnvRef` is observable — so it
uses `debug_assert!` plus the installed value, not `expect`.

`to_ref` is the single exception: its infallible signature forces it to panic on a startup error.
**Revised 2026-08-31** — that is no longer a reason to deprecate it, because `try_to_ref` now sits
beside it with the same body and a `Result`. The pair is the ordinary Rust shape (`foo` /
`try_foo`), the panicking half preserves 348 call sites verbatim, and the fallible half is what the
guide and `build()` use. Neither can fail with either built-in manager, since startup writes an
in-memory map.

Two failures reach `build()` from the configuration path added by decision 2, and both surface
there rather than at the setter: a `${VAR}` reference to an unset environment variable
(`expand_env_vars` already errors on it, with no default-value syntax), and a store `type` no
factory in the chain claims (`unknown_store_type_error`, which lists the types the chain does
support). Deferring them to `build()` is what keeps `with_store_config` infallible and chainable.

## Integration Points

| Crate / file | Change |
|---|---|
| `liquers-core/src/context.rs` | `GenericEnvironment` + four aliases replace four structs (~900 lines to ~250); `Environment` keeps `to_ref` and gains `try_to_ref`; `init_with_envref` becomes sync and fallible; `EnvRef::new` deprecated. |
| `liquers-core/src/environment_builder.rs` *(new)* | `EnvironmentBuilder`, `AssetManagerKind`, `Queued`, `Inline`. |
| `liquers-core/src/environment_config.rs` *(new)* | `EnvironmentConfig`. Separable: it depends on the builder, nothing depends on it. |
| `liquers-core/src/assets.rs` | `AssetManager`: drop `set_envref`, `start` becomes sync/fallible, add `refresh_command_versions` and `is_started`. Both managers take `EnvRef` at construction; `OnceLock<EnvRef<E>>` and its panic removed. `ensure_started` calls dropped from the five inline entry points. Remove the stray `eprintln!("Spawned job queue")`. |
| `liquers-core/src/dependencies.rs` | `register_version_sync`; sync `load_command_versions`. |
| `liquers-lib/src/environment.rs` | `SelectedAssetManager` cfg-import pair deleted (`DefaultKind` replaces it); `DefaultEnvironment` becomes an alias; `CommandRegistryAccess` and `register_polars_commands` move to impls on the aliased generic. |
| `liquers-web/src/environment.rs` | Construction sites migrate to the builder. `new_environment()` already separates registration from sharing, which maps onto the builder directly. Its hand-rolled `apply_store` — retained `STORE_CONFIG` + `STORE_OBJECTS`, rebuilt through `WebStoreFactory` on every environment construction — is `EnvironmentConfig::apply` written out by hand, and is the strongest evidence that this belongs in core. Migrating it is optional for this project and should be, since the rebuild path is the crate's most delicate. |
| `liquers-axum` | Construction sites migrate. |
| `liquers-py/src/context.rs` | `init_with_envref` is `todo!()`; removing the hook deletes it. Out of scope otherwise. |

**Dependency flow** is respected: everything new is in `liquers-core`, and `liquers-lib` /
`liquers-web` / `liquers-axum` only consume it. No backward `use`.

**Crate placement of a future `EnvironmentConfig` — amended 2026-08-31.** Sketched in Phase 3
§Scenario 4. This paragraph originally read: it cannot live in `liquers-core`, because
`StoreRouterConfig` lives in `liquers-store`, which depends on core, so it belongs in
`liquers-store` or above. **That constraint no longer exists.** `STORE-CONFIG-IN-CORE` (PR 46) moved
`StoreRouterConfig`, `StoreConfig`, `expand_env_vars`, the `StoreFactory` trait, factory chaining and
`StoreRouterBuilder` into `liquers-core`; `RECIPE-PROVIDER-BY-NAME` (PR 48) added
`RecipeProviderChoice` there too. Every field of the sketched `EnvironmentConfig` — store, recipes,
assets — is now a `liquers-core` type, so the whole configuration document can live beside the
builder it configures.

**Decided 2026-08-31: `EnvironmentConfig` lives in `liquers-core`, embeds `StoreRouterConfig`, and
the builder accepts it.** The goal is one file or JSON structure configuring the environment and its
store together, so splitting the document across two crates — or making the store section opaque —
would defeat it. `with_async_store(Arc<dyn AsyncStore>)` stays as the direct entry point for a caller
who has already built a store; `with_store_config` and `with_config` are added beside it.

This grows the project's scope beyond the readiness fix, deliberately. It is the smallest version of
the growth: `EnvironmentConfig` is a three-field serde struct plus `apply`-shaped setters over a
builder that has to exist anyway, and every type it names already exists in `liquers-core`. Phase 4
should sequence it as the **final, separable step**, after the readiness fix is green, so the
project can still land its P1 if the configuration layer turns up a surprise.

## Relevant Commands

**No new commands, and no command signature changes.** This project is construction and lifecycle
only; nothing reaches the query language, the planner, or `specs/command_registry.yaml` (so
`cargo test -p liquers-lib --test registry_export` is unaffected).

Existing namespaces are relevant only as *consumers* that must keep compiling: `lui` and `egui`
(`liquers-lib/src/ui/`), `pl` (`liquers-lib/src/polars/`) — the last reachable through
`register_polars_commands`, which moves as described above.

> **Question for the user:** is that read correct — that no command namespace needs to change, and
> the only command-side obligation is that `register_command!` keeps working against
> `builder.command_registry()`?

## Documentation Architecture

| Path | Kind | Audience | Change | Links |
|---|---|---|---|---|
| `specs/guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md` | guide (**new**) | application authors, integrators | Build and configure an environment: kind selection, `Value`/payload types, commands, store, recipe provider; the readiness guarantee; **configuring an environment and its store from one document**; and **implementing `init_with_envref` for a custom environment**, which is the contract that keeps `to_ref` correct. Replaces the earlier "migrating from `to_ref`" section with "when to use the builder and when `to_ref` is right". | from `specs/README.md`, DOC_04 |
| `specs/reference/ENVIRONMENT_CONFIG.md` | reference (**new**) | application authors, integrators | The configuration document: every field, the store section's delegation to `StoreRouterConfig`, `${VAR}` expansion, why the manager kind and the store factories are not fields. Sibling to `STORE_CONFIG_FSD.md`, linking to it rather than restating it. | from `specs/README.md`, the new guide, `STORE_CONFIG_FSD.md` |
| `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` | reference | maintainers | Replace the initialization sequence with `try_to_ref`'s, and document the builder as the recommended path; document `GenericEnvironment` and the aliases, and `init_with_envref`'s strengthened contract; retire gap rows **P0** (`EnvRef::new` unsafe — the deprecation and the manager-always-installed invariant close it) and **P1** (startup not observable). `## History` row + `reviewed:` bump. | to the new guide |
| `specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` | reference | maintainers | Manager lifecycle primitives: `set_envref` gone, `start` sync/fallible, `refresh_command_versions`, `is_started`. `## History` + `reviewed:`. | to DOC_04 |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | guide | integrators | What an integration does now that `init_with_envref` is gone; `with_type_registry` for a foreign handle type. | to the new guide |
| `specs/reference/PAYLOAD_GUIDE.md` | reference | maintainers | Names `SimpleEnvironmentWithPayload` / `ImmediateEnvironmentWithPayload`; they are aliases now. | — |
| `CLAUDE.md` | guide | agents | §Adding a Value Type points at `new_with_type_registry`; retarget to the builder. | — |
| `specs/README.md`, `specs/index.csv` | index | all | Design folder; issue statuses at Phase 5. | — |

**Proposed `affects_docs`:** `DOC_04_ENVIRONMENT_CONTEXT_EVALUATION`,
`DOC_03_ASSETS_EXECUTION_LIFECYCLE`, `ENVIRONMENT_CONSTRUCTION_GUIDE`, `ENVIRONMENT_CONFIG`,
`LANGUAGE-INTEGRATION_GUIDE`, `PAYLOAD_GUIDE`, `ASSET_LIFECYCLE`, `STORE_CONFIG_FSD`.

*(Revised 2026-08-31: `ENVIRONMENT_CONFIG` and `STORE_CONFIG_FSD` added — the configuration document
embeds `StoreRouterConfig`, so the store-configuration reference gains a sibling it must point at.)*

## Migration

Deliberately near-zero, which is what makes an L-complexity change safe:

- **348 `.to_ref()` sites** — compile unchanged, with **no deprecation warning**: `to_ref` keeps its
  trait method and its signature. *(Revised 2026-08-31: the draft had them warning, and the count
  was 336 when first measured — recounted at `HEAD`.)*
- **Every `EnvRef<SimpleEnvironment<Value>>`, `Context<E>`, `CommandEnvironment` alias** — unchanged
  (type aliases).
- **`&mut <env>.command_registry` — 120 sites** (108 `liquers-core`, 4 `liquers-web`, 4
  `liquers-axum`, 3 `liquers-lib`, 1 `liquers-py`). These are the real migration. Because the builder
  keeps `command_registry` as a public field, each is a one-word change of receiver
  (`env` → `builder`), not a restructuring. Sizing it honestly: mechanical, but 120 lines, not
  "small". *(Recounted at `HEAD` 2026-08-31; Reviewer B's original figure of 173 used a looser
  pattern that also matched non-mutable and non-receiver uses. 194 lines mention
  `.command_registry` at all.)*

  With the gate decision that constructors stay `pub`, **this migration is no longer mandatory**:
  an existing `let mut env = SimpleEnvironment::new(); … &mut env.command_registry … env.to_ref()`
  still compiles and is still correct. It is the recommended shape, applied where code is being
  touched anyway — which changes this row from the project's bulk to its long tail.
- **`with_async_store` / `with_recipe_provider` call sites** — move to the builder. Few: construction
  sites only. *(Revised: constructors stay `pub`, so this is a recommendation rather than a forced
  migration; `Box` → `Arc` on `with_async_store` is the one mechanical change.)*
- **`Default::default()` on an environment** — grep finds **no** in-tree caller, so dropping the
  `Default` impls costs nothing.
- **`register_polars_commands`** — one call site; needs the extension trait in scope.
- **`with_store` / `with_cache`** — removed. `with_cache` always panicked; `with_store` set a field
  nothing read.
- **`init_with_envref` implementors** — `liquers-lib`, `liquers-py` (a `todo!()`), and the four core
  environments. The four core ones collapse into `GenericEnvironment`'s single implementation and
  `liquers-lib`'s goes with its struct; `liquers-py`'s `todo!()` must become a real implementation or
  an `Err`, since the method is now fallible and its contract is what guarantees readiness.
  *(Revised: the hook is kept, not deleted.)*
- **Custom `Environment` implementors outside the tree** — the one genuinely breaking change in this
  design, and it is deliberate: `init_with_envref` gains a `Result` and the obligation to start the
  manager. The compiler catches it (changed signature), and the migration is to move manager
  construction into the hook. The `ENVIRONMENT_CONSTRUCTION_GUIDE` must show this, since it is the
  path the maintainer decision keeps open.

## Review Record

Two independent review passes were run against this document before the approval gate (Reviewer A:
Phase 1 conformity; Reviewer B: codebase alignment). Run sequentially rather than as parallel
agents, per the skill's host-compatibility fallback. Findings and their disposition:

### Reviewer A — Phase 1 conformity

| Phase 1 decision | Phase 2 | |
|---|---|---|
| Sync, fallible `build()` (option A) | honored | ok |
| Factory not `Arc::new_cyclic`; deferred slot moves to the environment | honored | ok |
| Reference cycle deferred, back-reference stays strong | honored | ok |
| Startup barrier re-runnable, not one-shot | `refresh_command_versions` + `is_started` | ok |
| `pub(crate)` constructors, types stay nameable | honored | ok |
| `EnvRef::new` deprecated | honored | ok |
| Recipe-provider inconsistency recorded, builder positioned to resolve later | decided: signatures unchanged, with rationale and the future hook named | ok |
| Complexity M→L | applied to issue and index | ok |
| **`to_ref` stays public, body reimplemented over the builder path** | ~~corrected~~ **honored** | see A1, reversed |

**Finding A1 (deviation, surfaced) — ~~upheld~~ REVERSED 2026-08-31 by maintainer decision.**

The finding read: Phase 1 recorded that `to_ref` would keep its public trait signature with a
builder-backed body, and it cannot, because a defaulted method on `Environment` has no way to
construct a builder for an arbitrary implementor; so Phase 2 moved it to a deprecated inherent
method on `GenericEnvironment`, accepting that an externally-defined `Environment` loses `to_ref`.

The maintainer decision is that an ad-hoc, user-implemented environment **does** need `to_ref` or an
equivalent, so that loss is not acceptable. Re-examined under that constraint, the finding's premise
is wrong in an instructive way: the default body never needed to construct a *builder*, only to run
the *sequence*, and the one step that varies per implementor is already abstracted behind
`init_with_envref`. Keeping the hook keeps the body generic — so Phase 1's original decision was
achievable all along, and the correction was the error.

Recorded rather than quietly rewritten because it changed the shape of the design twice, and the
second shape is simpler than either: `build()` now delegates to `try_to_ref`, so there is one
readiness sequence in the codebase instead of two implementations of one guarantee. See
§`Environment`.

**Finding A2 (scope).** Phase 1 left consolidation as a research task. Phase 2 commits to it. That
is the research answered, not scope creep, but it is a material commitment and is called out at the
approval gate rather than buried.

### Reviewer B — codebase alignment

**B1 — blocking, fixed.** The draft claimed `liquers-lib` could keep `register_polars_commands` as
an inherent method on the aliased type "because `SelectedKind` is a local type". Wrong twice: a type
alias creates no new type, and an inherent `impl` is permitted only in the defining crate regardless
of the parameters. Corrected in §Compatibility aliases — it becomes a local extension trait.
`CommandRegistryAccess` is unaffected (local trait, foreign type is allowed).

**B2 — sizing, fixed.** The draft called the migration "small". Grep found a large number of
`&mut env.command_registry` sites — recorded as 173 at the time, recounted as **120** at `HEAD` on
2026-08-31 with a tighter pattern. Corrected: the builder keeps `command_registry` as a public
field so each is a one-word receiver rename, and §Migration states the current number. The
2026-08-31 gate decision softens the finding further — with constructors staying `pub`, none of
these sites *has* to move.

**B3 — verified, no change.** `Default::default()` on an environment has no in-tree caller, so
dropping the `Default` impls is free.

**B4 — verified, no change.** `OnceLock::set` takes `&self`, so step 5 of `build()` works through
the `Arc` inside `EnvRef` with no `&mut` and no unsafe.

**B5 — verified, no change.** GATs are stable since Rust 1.65; the workspace is edition 2021 and the
toolchain is 1.94.

### rust-best-practices pass

No blocking findings. Checked: no `unwrap`/`expect` introduced (two existing `expect`s deleted, §Error
Handling); all errors are `liquers_core::error::Error` via typed constructors, no `Error::new`; no
default match arm added; crate dependency flow respected (everything new in `liquers-core`); bounds
are each justified by a field or associated type; `Arc` for shared cheaply-cloned handles, owned
`TypeRegistry` for write-once data. Advisory: `AssetManagerKind` is deliberately not object-safe —
noted in §Function Signatures so nobody later tries to make it `dyn`.

### Post-merge review (2026-08-31)

A fifth pass, run after the four prerequisite designs merged, re-read this document against `HEAD`
rather than against the tree it was written on. Findings:

**C1 — invariant, fixed.** `Environment::to_ref` now calls `refresh_metadata_versions()` before
`EnvRef::new` (`liquers-core/src/context.rs:223-227`), added by
`refresh-command-metadata-versions`. The `build()` sequence in this document predates it and had no
equivalent step, so implementing it as written would have reopened
`MACRO-LEAVES-STALE-METADATA-VERSION` for every environment built through the builder — silently,
because the symptom is a stale `metadata_version` in the dependency graph rather than a compile or
test failure. Added as step 0, with the ordering constraint stated.

**C2 — stale fact, fixed.** The payload environment no longer panics without a recipe provider
(PR 51). Three places said or implied it did.

**C3 — constraint lifted, recorded.** The layering argument against a core-side `EnvironmentConfig`
is void since PR 46. Recorded rather than acted on, as open question 4.

**C4 — vocabulary available, applied.** `RecipeProviderChoice` exists; the builder's defaults are
now expressed with it.

**C5 — verified, no change.** The consolidation targets are untouched at `HEAD`: four environment
structs with their own `init_with_envref` (`context.rs:1116`, `1251`, `1392`, `2106`),
`AssetManager::set_envref` (`assets.rs:3558`) and `async fn start` (`3629`), both
`OnceLock<EnvRef<E>>` slots (`3853`, `5613`), the `eprintln!("Spawned job queue")` (`3902`),
`ImmediateAssetManager::ensure_started` (`5671`), `liquers-lib`'s `SelectedAssetManager` cfg pair
(`environment.rs:20-22`), and the `yield_now()` + `sleep(50ms)` in
`dependency_manager_integration.rs:89-90`. Every "before" quotation in Phase 3 still matches its
source except where Phase 3 now marks otherwise.

### Gate decisions (2026-08-31)

Two maintainer decisions taken at the Phase 3 gate, and the rust-best-practices re-check they
prompted.

**D1 — `to_ref` stays.** The builder is the ergonomic, recommended construction path, but an ad-hoc
user-created environment may still need `to_ref` or an equivalent; phase it out only where that is
cheap and sensible. Applied in §`Environment`, §Hiding the remaining doors (withdrawn), §Function
Signatures, §Migration, and Reviewer A's finding A1 (reversed). Resolves Phase 3 open question 5,
which was blocking Phase 4.

**D2 — one configuration document.** The store router configuration is a section of the environment
configuration; the goal is a single file or JSON structure configuring both. Applied in §Data
Structures (`EnvironmentConfig`), §`EnvironmentBuilder` inherent API, §Integration Points,
§Documentation Architecture. Resolves open question 4, and moves Phase 1's *Future Direction* into
scope.

**rust-best-practices re-check on the changed surface.** `init_with_envref` returning
`Result<(), Error>` uses the typed constructors, no `Error::new`; the provided `try_to_ref` body
introduces no `unwrap`/`expect`, and `to_ref`'s panic is the documented cost of an infallible
signature that predates this design. `EnvironmentConfig` derives `Serialize`/`Deserialize` with
`#[serde(default)]` on every field, matching `StoreRouterConfig`, and holds owned data with no
lifetimes. `Box<dyn StoreFactory>` matches `StoreRouterBuilder::new`'s existing parameter rather
than introducing a second convention. `with_store_config` stores the pair and defers construction,
so no setter becomes fallible. No new `match` is introduced, so the no-default-arm rule is
unaffected. One advisory: `EnvironmentConfig` should get `#[non_exhaustive]` if a `commands:`
section is genuinely expected (open question 7) — but `#[serde(default)]` already makes field
addition non-breaking for deserialization, and `#[non_exhaustive]` would block struct-literal
construction in tests, so the recommendation is **not** to add it.

## Open Questions

1. **Cascade application in `refresh_command_versions`.** A changed version yields
   `ExpiredDependents`, and `expire_dependencies_result` is async. Return the set for an async
   caller to apply, or provide a sync application path? First `start()` is unaffected (nothing can
   cascade), so this does not weaken the readiness guarantee — but it decides
   `refresh_command_versions`'s signature. **Still open.**
2. **`Queued` on wasm.** `Queued` is `#[cfg(not(target_arch = "wasm32"))]`, so
   `GenericEnvironment<V, P, Queued>` simply does not exist there. Confirm no wasm code path needs
   a compile-time-present-but-unusable kind. **Still open**, though low-risk: `liquers-lib`'s
   existing `SelectedAssetManager` cfg pair already makes `DefaultAssetManager` absent on wasm, so
   the situation is unchanged rather than new.
3. **~~Deprecation horizon for `to_ref`.~~ Resolved 2026-08-31 (maintainer).** `to_ref` is not
   deprecated and is not scheduled for removal. The builder is the recommended, documented path;
   `to_ref` remains supported for an ad-hoc or user-implemented environment. In-tree call sites are
   phased out opportunistically, where it is cheap and the surrounding code is being touched anyway
   — never as a mass migration. `EnvRef::new` keeps its deprecation, since it is the path that
   actually produces an unready `EnvRef`.
4. **~~Does the builder accept a store configuration?~~ Resolved 2026-08-31 (maintainer): yes, as
   part of a whole `EnvironmentConfig`.** The store router configuration is a *section of* the
   environment configuration, not a separate document — the goal is that one file or JSON structure
   configures both. `EnvironmentConfig` lives in `liquers-core` (§Data Structures), the builder
   gains `with_store_config` / `with_config`, and `with_async_store` stays for a caller who has
   already built a store. Scope grows deliberately; Phase 4 sequences it last and separably.
5. **~~The `eprintln!` removal.~~ Resolved, recommendation adopted.** Deleting
   `SimpleEnvironmentWithPayload`'s per-call `"No recipe provider configured"` line loses nothing
   observable — the consolidated field is a non-optional `Arc`, so the state it warned about cannot
   occur. A Phase 5 note, no replacement diagnostic.

### Newly open, from the 2026-08-31 decisions

6. **Where does the factory chain's default come from?** `with_store_config` takes the factory
   explicitly, which is right for `liquers-web` (its own chain) and `liquers-store` (core + OpenDAL).
   But the common case — an application on `liquers-lib` — then writes out a factory chain to get
   the obvious answer. Options: a `with_config`-shaped overload defaulting to
   `default_store_factory()`; a `liquers-lib` convenience mirroring `default_environment_builder`;
   or leaving it explicit. **Recommendation: the `liquers-lib` convenience**, matching how the
   recipe-provider default is already handled per crate — core stays neutral, the library crate
   supplies the batteries.
7. **Does `EnvironmentConfig` need a `commands:` section eventually?** Out of scope here, and the
   answer is probably "a *declaration* section, not a command section": `CommandDeclaration`
   (PR 50) is serde-able and names an implementation to resolve, which is precisely the
   document-#2 shape `DESIGN.md` describes. Recorded so the struct's forward compatibility is a
   considered omission rather than an oversight — adding a field to a `#[serde(default)]` struct is
   non-breaking.

# Phase 2: Solution & Architecture - Environment Builder

## Overview

One generic environment replaces the four near-duplicates, parameterized by value type, payload type
and an **asset-manager kind** marker; the existing names survive as type aliases, so no call site
moves. An `EnvironmentBuilder` owns the construction cycle inside a single synchronous `build()`:
it constructs the environment with an empty manager slot, wraps it in an `EnvRef`, constructs the
manager with that `EnvRef` in hand, installs it, and runs startup — so no partially initialized
`EnvRef` is ever observable. `Environment::to_ref` stays public and is reimplemented over the same
path, closing the readiness hole through that door too.

> **Amended 2026-08-31, after the four prerequisite designs merged.** The architecture is unchanged;
> five factual amendments are marked inline and summarised in
> [`DESIGN.md`](./DESIGN.md) §Prerequisite review. The material ones: `build()` gains a
> metadata-version refresh step (§`EnvironmentBuilder` inherent API), the recipe-provider default is
> now expressed with `RecipeProviderChoice` (§Recipe Provider), the payload environment's panic is
> already fixed so the builder preserves rather than delivers that fix (§preflight), and a core-side
> `EnvironmentConfig` is now possible, which opens **open question 4**.

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
    /// Called by `EnvironmentBuilder::build` after the `EnvRef` is created and before it is
    /// observable. Sync: see §Sync vs Async.
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

### `EnvironmentBuilder`

```rust
pub struct EnvironmentBuilder<V: ValueInterface,
                              P: PayloadType = (),
                              K: AssetManagerKind = DefaultKind> {
    type_registry: TypeRegistry,
    async_store: Arc<dyn AsyncStore>,
    /// Public field, mirroring the environments' existing `pub command_registry`. This is what
    /// keeps the 173 `&mut env.command_registry` sites to a one-word rename of the receiver.
    pub command_registry: CommandRegistry<GenericEnvironment<V, P, K>>,
    recipe_provider: Option<Arc<dyn AsyncRecipeProvider<GenericEnvironment<V, P, K>>>>,
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
by-value chain, and keeping the field mirrors the environments' current shape so the 173 existing
`&mut env.command_registry` sites migrate by renaming the receiver, not by restructuring.

`build()` returns `Result` even though today's startup cannot fail. The fallible signature is the
cheap half of the issue's "startup failures should be returned to the caller"; making it infallible
now would be the breaking change later.

**`build()` sequence** (signatures only; bodies are Phase 4):

0. **`self.command_registry.get_mut_command_metadata_registry().refresh_metadata_versions();`**
   *(added 2026-08-31)* — `register_command!` mutates command metadata after the registry first
   computes `metadata_version`, so the versions are stale until refreshed.
   `refresh-command-metadata-versions` put this call at the head of `Environment::to_ref`; `build()`
   does not delegate through `to_ref`, so it must run the same operation or reopen
   `MACRO-LEAVES-STALE-METADATA-VERSION`. **Order is load-bearing**: refresh must precede step 6,
   because `start()` snapshots those versions into the `DependencyManager`, and a version snapshotted
   stale is a cache-validation defect rather than a cosmetic one. Step 0 is also the operation
   `refresh_command_versions` must re-run for late registration — refresh the metadata registry, then
   re-register into the dependency manager.
1. Resolve the recipe provider: configured, else `RecipeProviderChoice::Trivial`.
2. Construct `GenericEnvironment` with `asset_store: OnceLock::new()`.
3. `let envref = EnvRef::new(env);`
4. `let manager = K::build(envref.clone(), &self.manager_options)?;` — the manager receives a live
   `EnvRef` and stores it as a plain field. It is fully formed at birth. An option the kind cannot
   honor fails here rather than being dropped.
5. Install into the environment's `OnceLock`. Unreachable-if-already-set: the builder is the only
   writer and it holds the sole `EnvRef`.
6. `manager.start()?` — synchronous, see below.
7. Return `envref`.

Steps 3–5 are the only window in which the environment's manager slot is empty, and no `EnvRef`
escapes the function during it. That is the entire readiness guarantee.

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

### `Environment` — `to_ref` reimplemented, `init_with_envref` removed

```rust
pub trait Environment: Sized + MaybeSync + MaybeSend + 'static {
    // … associated types unchanged …

    // REMOVED: fn init_with_envref(&self, envref: EnvRef<Self>);
    //   Its two jobs — install the back-reference, arrange startup — now happen inside build(),
    //   before an EnvRef exists. Nothing is left for the hook to do.

    // REMOVED: fn to_ref(self) -> EnvRef<Self>;
    //   Not a trait method any more: it cannot be written generically once construction is the
    //   builder's job. See below.
}
```

`to_ref` is **kept as an inherent method on `GenericEnvironment`**, not on the trait:

```rust
impl<V, P, K> GenericEnvironment<V, P, K> {
    #[deprecated(note = "use EnvironmentBuilder::build; this panics on a startup error")]
    pub fn to_ref(self) -> EnvRef<Self>;
}
```

This is the one place Phase 1's decision needs a correction. Phase 1 recorded "`to_ref` stays
public, body reimplemented over the builder path". As a *trait* method it cannot be: its default
body would have to construct a builder, and the trait has no way to know how. As an *inherent*
method on the concrete generic type it can — and because every existing environment name is an
alias of that type, all 336 call sites still compile. The cost is that a hypothetical
externally-defined `Environment` loses `to_ref`, which Phase 1 (question 1) already accepted: such a
user replicates the construction.

Its signature is infallible, so it must panic on a startup error. That is why it is deprecated
rather than merely kept, and it is the reason `build()` is the documented path.

```rust
impl<E: Environment> EnvRef<E> {
    #[deprecated(note = "produces an EnvRef with no asset manager installed; use \
                         EnvironmentBuilder::build")]
    pub fn new(env: E) -> Self;  // becomes pub(crate) once the deprecation cycle ends
}
```

### Hiding the remaining doors

Per Phase 1's refinement: `GenericEnvironment::new*` constructors become `pub(crate)` and the public
`Default` impls are dropped. The **type** stays public and nameable — `register_command!` needs a
`CommandEnvironment` alias, and users write `EnvRef<SimpleEnvironment<Value>>` in their own
signatures — but no user can obtain an owned environment except from the builder, so `to_ref` is
unreachable in new code without being removed from existing code.

After this, exactly two paths reach an `EnvRef`: `EnvironmentBuilder::build` and the deprecated
`to_ref` that delegates to it. Both start the manager.

## Sync vs Async Decisions

| Operation | Decision | Rationale |
|---|---|---|
| `EnvironmentBuilder::build` | **sync**, fallible | Phase 1 option A. Callable from `main`, from a plain `#[test]`, from a wasm entry point. |
| `AssetManager::start` | **sync**, fallible | Its only `await` was `scc::entry_async`. At startup the `versions` map is empty and uncontended, every key inserts `Vacant`, so `version_changed` is always false and no cascade can fire. No store is touched: `load_from_records` is reached from asset recovery and `track_asset`, never from `start`. |
| `refresh_command_versions` | **sync**, fallible, returns work | Same map writes, but a changed version *can* cascade — see the caveat above and open question 1. |
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
    pub fn build(self) -> Result<EnvRef<GenericEnvironment<V, P, K>>, Error>;
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
```

### Removed

```rust
// Environment
- fn init_with_envref(&self, envref: EnvRef<Self>);
- fn to_ref(self) -> EnvRef<Self>;          // survives as an inherent method, deprecated

// AssetManager<E>
- fn set_envref(&self, envref: EnvRef<E>);

// GenericEnvironment (were on each of the four structs)
- pub fn with_store(&mut self, store: Box<dyn Store>) -> &mut Self;
- pub fn with_cache(&mut self, _cache: Box<dyn Cache<V>>) -> &mut Self;   // always panicked
- impl Default for …                                                     // constructors pub(crate)
```

### Deprecated, kept

```rust
impl<V, P, K> GenericEnvironment<V, P, K> {
    #[deprecated(note = "use EnvironmentBuilder::build; this panics on a startup error")]
    pub fn to_ref(self) -> EnvRef<Self>;
}
impl<E: Environment> EnvRef<E> {
    #[deprecated(note = "produces an EnvRef with no asset manager installed; \
                         use EnvironmentBuilder::build")]
    pub fn new(env: E) -> Self;
}
```

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
That is exactly why it is deprecated.

## Integration Points

| Crate / file | Change |
|---|---|
| `liquers-core/src/context.rs` | `GenericEnvironment` + four aliases replace four structs (~900 lines to ~250); `Environment` loses `to_ref` and `init_with_envref`; `EnvRef::new` deprecated. |
| `liquers-core/src/environment_builder.rs` *(new)* | `EnvironmentBuilder`, `AssetManagerKind`, `Queued`, `Inline`. |
| `liquers-core/src/assets.rs` | `AssetManager`: drop `set_envref`, `start` becomes sync/fallible, add `refresh_command_versions` and `is_started`. Both managers take `EnvRef` at construction; `OnceLock<EnvRef<E>>` and its panic removed. `ensure_started` calls dropped from the five inline entry points. Remove the stray `eprintln!("Spawned job queue")`. |
| `liquers-core/src/dependencies.rs` | `register_version_sync`; sync `load_command_versions`. |
| `liquers-lib/src/environment.rs` | `SelectedAssetManager` cfg-import pair deleted (`DefaultKind` replaces it); `DefaultEnvironment` becomes an alias; `CommandRegistryAccess` and `register_polars_commands` move to impls on the aliased generic. |
| `liquers-web/src/environment.rs` | Construction sites migrate to the builder. `new_environment()` already separates registration from sharing, which maps onto the builder directly. |
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

Keeping `with_async_store(Arc<dyn AsyncStore>)` as the builder's only store entry point is therefore
now a *choice* rather than a constraint, and one this design has not made. **Open question 4.**

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
| `specs/guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md` | guide (**new**) | application authors, integrators | Build and configure an environment: kind selection, `Value`/payload types, commands, store, recipe provider; the readiness guarantee; migrating from `to_ref`. Structured so a config-driven section can be added later without restructuring. | from `specs/README.md`, DOC_04 |
| `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` | reference | maintainers | Replace the initialization sequence with the builder; document `GenericEnvironment` and the aliases; retire gap rows **P0** (`EnvRef::new` unsafe) and **P1** (startup not observable). `## History` row + `reviewed:` bump. | to the new guide |
| `specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` | reference | maintainers | Manager lifecycle primitives: `set_envref` gone, `start` sync/fallible, `refresh_command_versions`, `is_started`. `## History` + `reviewed:`. | to DOC_04 |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | guide | integrators | What an integration does now that `init_with_envref` is gone; `with_type_registry` for a foreign handle type. | to the new guide |
| `specs/reference/PAYLOAD_GUIDE.md` | reference | maintainers | Names `SimpleEnvironmentWithPayload` / `ImmediateEnvironmentWithPayload`; they are aliases now. | — |
| `CLAUDE.md` | guide | agents | §Adding a Value Type points at `new_with_type_registry`; retarget to the builder. | — |
| `specs/README.md`, `specs/index.csv` | index | all | Design folder; issue statuses at Phase 5. | — |

**Proposed `affects_docs`:** `DOC_04_ENVIRONMENT_CONTEXT_EVALUATION`,
`DOC_03_ASSETS_EXECUTION_LIFECYCLE`, `ENVIRONMENT_CONSTRUCTION_GUIDE`,
`LANGUAGE-INTEGRATION_GUIDE`, `PAYLOAD_GUIDE`, `ASSET_LIFECYCLE`.

## Migration

Deliberately near-zero, which is what makes an L-complexity change safe:

- **336 `.to_ref()` sites** — compile unchanged (inherent method on the aliased type), emitting a
  deprecation warning.
- **Every `EnvRef<SimpleEnvironment<Value>>`, `Context<E>`, `CommandEnvironment` alias** — unchanged
  (type aliases).
- **`&mut env.command_registry` — 173 sites** (94 `liquers-core/tests`, 63 `liquers-core/src`, 6
  `liquers-web/src`, the rest examples). These are the real migration. Because the builder keeps
  `command_registry` as a public field, each is a one-word change of receiver
  (`env` → `builder`), not a restructuring. Sizing it honestly: mechanical, but 173 lines, not
  "small".
- **`with_async_store` / `with_recipe_provider` call sites** — move to the builder, since the
  environment's constructors become `pub(crate)`. Few: construction sites only.
- **`Default::default()` on an environment** — grep finds **no** in-tree caller, so dropping the
  `Default` impls costs nothing.
- **`register_polars_commands`** — one call site; needs the extension trait in scope.
- **`with_store` / `with_cache`** — removed. `with_cache` always panicked; `with_store` set a field
  nothing read.
- **`init_with_envref` implementors** — `liquers-lib`, `liquers-py` (a `todo!()`), and the four core
  environments. All deleted.

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
| **`to_ref` stays public, body reimplemented over the builder path** | **corrected** | see below |

**Finding A1 (deviation, surfaced).** Phase 1 recorded that `to_ref` would keep its public trait
signature with a builder-backed body. It cannot: as a defaulted method on `Environment` its body has
no way to know how to construct a builder for an arbitrary implementor. Phase 2 moves it to an
inherent method on `GenericEnvironment`, deprecated. All 336 call sites still compile because every
environment name is an alias of that type; the loss is that an externally-defined `Environment` has
no `to_ref` — which Phase 1 question 1 already accepted.

**Finding A2 (scope).** Phase 1 left consolidation as a research task. Phase 2 commits to it. That
is the research answered, not scope creep, but it is a material commitment and is called out at the
approval gate rather than buried.

### Reviewer B — codebase alignment

**B1 — blocking, fixed.** The draft claimed `liquers-lib` could keep `register_polars_commands` as
an inherent method on the aliased type "because `SelectedKind` is a local type". Wrong twice: a type
alias creates no new type, and an inherent `impl` is permitted only in the defining crate regardless
of the parameters. Corrected in §Compatibility aliases — it becomes a local extension trait.
`CommandRegistryAccess` is unaffected (local trait, foreign type is allowed).

**B2 — sizing, fixed.** The draft called the migration "small". Grep finds **173**
`&mut env.command_registry` sites. Corrected: the builder keeps `command_registry` as a public
field so each is a one-word receiver rename, and §Migration now states the real number.

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

## Open Questions

1. **Cascade application in `refresh_command_versions`.** A changed version yields
   `ExpiredDependents`, and `expire_dependencies_result` is async. Return the set for an async
   caller to apply, or provide a sync application path? First `start()` is unaffected (nothing can
   cascade), so this does not weaken the readiness guarantee — but it decides
   `refresh_command_versions`'s signature.
2. **`Queued` on wasm.** `Queued` is `#[cfg(not(target_arch = "wasm32"))]`, so
   `GenericEnvironment<V, P, Queued>` simply does not exist there. Confirm no wasm code path needs
   a compile-time-present-but-unusable kind.
3. **Deprecation horizon.** Is `to_ref` deprecated-and-kept indefinitely, or removed in a later
   release once the tests migrate? Affects whether the 336 sites are ever touched.
4. **Does the builder accept a store *configuration*, now that it could?** *(new, 2026-08-31.)*
   `StoreRouterConfig`, `StoreFactory` and `StoreRouterBuilder` are `liquers-core` types since
   PR 46, so `EnvironmentBuilder` could offer

   ```rust
   pub fn with_store_config(self, config: StoreRouterConfig, factory: Box<dyn StoreFactory>)
       -> Result<Self, Error>;   // or defer construction to build()
   ```

   alongside `with_async_store`. Three options:

   | Option | Consequence |
   |---|---|
   | **A — keep `with_async_store` only** (status quo) | Smallest surface. A configuration-driven caller builds the router itself and passes the result; `liquers-web` already does exactly this with its retained `STORE_CONFIG`. |
   | **B — add `with_store_config`** | The builder becomes directly configuration-drivable, and the future `EnvironmentConfig` is a thin serde wrapper over the builder rather than a separate assembly step. Costs a fallible setter (or a deferred failure inside `build()`) and a policy on which factory chain is the default. |
   | **C — take a whole `EnvironmentConfig` in core** | Largest step; effectively builds the "single configuration point" now, which Phase 1 explicitly placed out of scope. |

   **Recommendation: A for this project, with B noted as additive.** Both `with_async_store` and
   `with_store_config` can coexist and B can be added later without a breaking change, so declining
   it now costs nothing — whereas taking it on widens an already-L change and drags factory-chain
   policy into a readiness fix. This needs a decision at the gate, because it is the one place where
   a merged prerequisite genuinely offers this design new scope.
5. **Does the `eprintln!` removal need recording as an intentional behavior change?** *(new,
   2026-08-31.)* `SimpleEnvironmentWithPayload::get_recipe_provider` currently logs
   `"No recipe provider configured …"` on every call. Consolidation deletes it, because the
   consolidated field is a non-optional `Arc`. Nothing observable is lost — the fallback provider is
   the same — but a user who relies on that line to notice a misconfiguration loses it. The
   suggested answer is that this is a strict improvement (the builder makes the default explicit at
   construction, so the warning has no subject) and needs only a Phase 5 note, not a replacement
   diagnostic.

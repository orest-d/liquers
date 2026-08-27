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
| 3b | Deprecated `to_ref` still compiles | 336 call sites keep working | inherent `to_ref` on the alias |
| 3c | `Queued` needs a runtime | Sync ≠ runtime-free | `Queued::build` |
| 3d | Late command registration | The re-runnable barrier | `refresh_command_versions` |

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
| T9 | `recipe_provider_defaults_across_all_aliases` | `CORE-PAYLOAD-ENV-RECIPE-PROVIDER-PANIC` cannot recur | unit |
| T10 | `deprecated_to_ref_produces_a_ready_envref` | The old door is not a hole | integration |
| T11 | `aliases_are_the_generic_type` | Consolidation did not change any public type identity | compile-time |
| T12 | `inline_builds_without_a_tokio_runtime` | Wasm path; `Inline` spawns nothing | unit |

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
> `liquers-lib/src/environment.rs:152`. That is stale — `LIB-RECIPE-PROVIDER-PANIC` was fixed and
> the field is now a non-optional `Arc`. The provider call is still wanted (the default reads
> recipes from the store), but the stated reason is wrong and should be corrected while the
> surrounding lines are being edited.

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
    let mut builder = EnvironmentBuilder::<Value>::new()
        .with_recipe_provider(Arc::new(DefaultRecipeProvider));
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

### Scenario 4 — `EnvironmentConfig`, sketched (NOT in scope)

Phase 1 records a single-configuration-point ambition and requires only that this design not
preclude it. Sketching it is how that requirement gets tested, so this scenario is illustrative:
nothing here is being built now.

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

```rust
// in liquers-store: the lowest crate that can see both StoreRouterConfig and EnvironmentBuilder
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EnvironmentConfig {
    #[serde(default)] pub store: StoreRouterConfig,
    #[serde(default)] pub recipes: RecipeProviderChoice,
    #[serde(default)] pub assets: AssetManagerOptions,
}

impl EnvironmentConfig {
    pub fn from_yaml(yaml: &str) -> Result<Self, Error>;
    pub fn expand_env_vars(&mut self) -> Result<(), Error>;

    /// Apply every configured service to a builder. Commands are the caller's job.
    pub fn apply<V: ValueInterface, P: PayloadType, K: AssetManagerKind>(
        &self,
        builder: EnvironmentBuilder<V, P, K>,
        factories: &[Box<dyn StoreFactory>],
    ) -> Result<EnvironmentBuilder<V, P, K>, Error>;
}
```

Use:

```rust
let mut config = EnvironmentConfig::from_yaml(&std::fs::read_to_string("environment.yaml")?)?;
config.expand_env_vars()?;

let mut builder = config.apply(EnvironmentBuilder::<Value>::new(), &factories)?;
register_my_commands(&mut builder.command_registry)?;   // code, not config
let envref = builder.build()?;
```

`apply` takes and returns the builder by value, matching the `with_*` setters, so it composes with
hand-written configuration in either order — config first then override in code, or the reverse.
`StoreRouterConfig` and `expand_env_vars` are reused verbatim; note the existing expander supports
`${VAR}` only and **errors** when the variable is unset, with no default-value syntax.

**Layering.** `EnvironmentConfig` cannot live in `liquers-core`, because `StoreRouterConfig` lives
in `liquers-store`, which depends on core (Phase 1 §Future Direction). It belongs in
`liquers-store`, which sees both. This is precisely why `EnvironmentBuilder::with_async_store` takes
an already-constructed `Arc<dyn AsyncStore>` rather than a store *config*: the core builder stays
free of the layering problem, and a higher crate adds the config layer without touching core.

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

### 3b — `to_ref` still compiles, and is still correct

```rust
let envref = SimpleEnvironment::<Value>::new().to_ref();  // warning: deprecated
assert!(envref.get_asset_manager().is_started());          // but ready
```

All 336 sites keep working. The deprecation warning is the migration prompt; the readiness is the
point. Because `to_ref`'s signature is infallible it must panic on a startup error — which cannot
happen today, and is the stated reason the method is deprecated rather than blessed.

**But `SimpleEnvironment::<Value>::new()` is `pub(crate)` after this change**, so the line above
compiles only *inside* `liquers-core`. Outside it, existing `.to_ref()` calls still compile only if
the environment came from somewhere they can still reach.

> **Open question 5** (new, and material): Phase 2 claims both "336 `to_ref` sites keep working" and
> "constructors become `pub(crate)`". Those are in tension for the sites *outside* `liquers-core` —
> 94 in `liquers-core/tests` are integration tests and therefore external crates too. Either the
> constructors stay `pub` through a deprecation period (and `to_ref` is genuinely free), or they go
> `pub(crate)` now (and those sites migrate in the same PR). The second is more honest but makes the
> migration mandatory rather than gradual. **This needs deciding before Phase 4.**

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
| No recipe provider configured | `TrivialRecipeProvider` for **every** alias — the fix for `CORE-PAYLOAD-ENV-RECIPE-PROVIDER-PANIC`. |
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
| `recipe_provider_defaults_across_all_aliases` (T9) | All four aliases return `TrivialRecipeProvider` unconfigured; **none panics** |
| `inline_builds_without_a_tokio_runtime` (T12) | Plain `#[test]`, no `#[tokio::test]`: `Inline` builds. `manager_parametric.rs` already carries a "no-tokio-runtime proof" for `ImmediateAssetManager`; extend it to construction rather than duplicating it |
| `builder_defaults_match_previous_environment_defaults` | `NoAsyncStore`, empty registry, type registry from `V` |

### Integration tests — `liquers-core/tests/`

| Test | File | Assertion |
|---|---|---|
| `command_version_present_immediately_after_build` (T3) | `environment_builder.rs` *(new)* | The Phase 1 reproduction inverted: `get_version` is `Some` with **no sleep** |
| `plan_dependencies_registered_on_first_evaluation` (T2) | `environment_builder.rs` | Evaluate a keyed asset immediately after `build()`; `expire_dependents(command_impl_key)` returns that asset. **This is the original bug** — it returned 0 before |
| `concurrent_first_evaluations_share_one_startup` (T4) | `environment_builder.rs` | N concurrent first evaluations; startup ran once (counter on a test kind), all observe complete state |
| `readiness_equivalent_across_kinds` (T6) | `manager_parametric.rs` *(extend)* | Same assertions for `Queued` and `Inline` — issue verification item 5. The file is already parametric over managers |
| `refresh_command_versions_expires_dependents` (T7) | `environment_builder.rs` | Change a metadata version, refresh, assert the dependent asset expired |
| `deprecated_to_ref_produces_a_ready_envref` (T10) | `environment_builder.rs` | `#[allow(deprecated)]`; same readiness assertions |
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
  must pass; the rebuild-on-late-registration path is the sensitive one.

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

## Review Record

Three review passes run sequentially before the approval gate (skill host-compatibility fallback,
no agents spawned): Phase 1 conformity, Phase 2 conformity, and codebase + query validation.

**Reviewer 1 — Phase 1 conformity.** Examples exercise every Phase 1 decision: sync fallible
`build()` (1a, 1b), kind selection replacing the cfg-import hack (2b), the readiness guarantee as an
assertion (3a), the re-runnable barrier (3d), `to_ref` deprecated-but-correct (3b), the reference
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

## Open Questions

1. *(from Phase 2)* `refresh_command_versions` cascade application — return `ExpiredDependents` or
   apply synchronously?
2. *(from Phase 2)* Confirm no wasm path needs `Queued` present-but-unusable.
3. *(from Phase 2)* Is `to_ref` deprecated indefinitely, or removed once tests migrate?
4. **~~Builder convenience aliases?~~ Resolved: none.** One `EnvironmentBuilder<V, P, K>` with
   default type parameters (`P = ()`, `K = DefaultKind`, target-selected). A builder name per
   environment would re-suggest the duplication this project removes. Side benefit: `liquers-lib`
   loses its own target-selection cfg pair, since `DefaultKind` already does it.
5. **New, and blocking Phase 4.** `pub(crate)` constructors and "336 `to_ref` sites keep working"
   are in tension outside `liquers-core` — including its own `tests/`, which are external crates.
   Keep constructors `pub` through a deprecation period (gradual migration), or make them
   `pub(crate)` now (migration mandatory in the same PR)?

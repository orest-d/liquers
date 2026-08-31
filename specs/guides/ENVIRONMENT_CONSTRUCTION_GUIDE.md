---
title: Building and Configuring an Environment
kind: guide
audience: both
area: [core/context, core/assets, core/store]
reviewed: 2026-08-31
---
# Building and Configuring an Environment

An `Environment` owns the global services a query evaluation needs: the command registry, the
asset manager, the store, the recipe provider and the type registry. This guide covers building
one, configuring it from a document, and — for an integration with its own global services —
implementing one by hand.

## The short version

```rust,ignore
use liquers_core::environment_builder::EnvironmentBuilder;
use liquers_macro::register_command;

let mut builder = EnvironmentBuilder::<Value>::new()
    .with_async_store(Arc::new(store));

let cr = &mut builder.command_registry;
register_command!(cr, fn greet(state, greeting: String = "Hello") -> result)?;

let envref = builder.build()?;                    // ready to evaluate
let state = evaluate(envref.clone(), "world/greet", None).await?;
```

`build()` returns an `EnvRef` whose asset manager is **already started**. There is no window in
which the returned reference is not ready — see §The readiness guarantee.

## Configure, then build

Everything that needs `&mut` happens on the builder; `build()` consumes it.

| What | How |
|---|---|
| Commands | `&mut builder.command_registry`, a public field |
| Store | `.with_async_store(Arc<dyn AsyncStore>)`, or `.with_store_config(config, factory)` |
| Recipe provider | `.with_recipe_provider_choice(RecipeProviderChoice::Default)` |
| Type registry | `.with_type_registry(registry)` |
| Manager options | `.with_asset_manager_options(...)` |
| Everything at once | `.with_config(EnvironmentConfig, factory)` |

The setters take `self` and return `Self`, so they chain. `command_registry` is a field rather than
a setter because `register_command!` needs a `&mut CommandRegistry`, which cannot be threaded
through a by-value chain.

**The split is not arbitrary.** The setters are the half a configuration document can drive;
commands are Rust functions and no document can name one. That is why `EnvironmentConfig` covers
services only.

## Choosing an execution model

The asset-manager *kind* is a type parameter:

```rust,ignore
EnvironmentBuilder::<Value>::new()                    // DefaultKind: Queued natively, Inline on wasm
EnvironmentBuilder::<Value, (), Inline>::new()        // force inline on native, e.g. for tests
EnvironmentBuilder::<Value, UiPayload>::new()         // a payload type
```

| Kind | Execution | Spawns | Needs a Tokio runtime |
|---|---|---|---|
| `Queued` | job queue plus expiration monitor | two tasks, at construction | **yes** |
| `Inline` | in the caller's task | nothing | no |

> **Pitfall — synchronous does not mean runtime-free.**
> ```rust,ignore
> fn main() {
>     let envref = EnvironmentBuilder::<Value>::new().build();  // panics: no reactor running
> }
> ```
> `build()` being synchronous means "no `.await` at the call site", not "no runtime". `Queued`
> spawns from its constructor. Use `#[tokio::main]`, or `Inline` where there is genuinely no
> runtime — which is what a browser build needs, and why `DefaultKind` selects it on wasm.

The kind is a type rather than a configuration string on purpose: two branches of a match on
`"queued"` / `"inline"` produce two different concrete environment types, and `Environment` is not
object-safe, so they cannot be erased behind a `dyn`. An application that wants runtime selection
monomorphizes its own tail:

```rust,ignore
match kind {
    "queued" => serve(EnvironmentBuilder::<Value, (), Queued>::new().build()?).await,
    "inline" => serve(EnvironmentBuilder::<Value, (), Inline>::new().build()?).await,
    other    => return Err(Error::general_error(format!("unknown manager kind: {other}"))),
}
```

## Configuring from a document

`EnvironmentConfig` describes the store, the recipe provider and the manager options in one
YAML/JSON/TOML document. The store section is a verbatim `StoreRouterConfig`, so the two formats do
not diverge.

```yaml
store:
  stores:
    - type: filesystem
      prefix: data
      config:
        path: ${LIQUERS_DATA}
    - type: memory
      prefix: tmp
recipes: default          # default | trivial
assets:
  job_capacity: 8         # queued only
```

```rust,ignore
let config = EnvironmentConfig::from_yaml(&std::fs::read_to_string("environment.yaml")?)?;

let mut builder = EnvironmentBuilder::<Value>::new()
    .with_config(config, Box::new(default_store_factory()));
register_my_commands(&mut builder.command_registry)?;   // code, not config
let envref = builder.build()?;
```

Two behaviours worth knowing, because both are quiet:

- **An absent `recipes:` key means `default`, not `trivial`.** That is the *document* default — a
  configuration saying nothing about recipes most plausibly wants them to work — and it is
  deliberately not the unconfigured default of `EnvironmentBuilder::new()`, which is `Trivial`. So
  applying even an empty configuration changes how recipes resolve.
- **An unset `${VAR}` fails the build.** The expander has no default-value syntax, so a missing
  variable is an error rather than an empty path.

Store construction and `${VAR}` expansion are deferred to `build()`, which is what keeps the
setters infallible: a malformed configuration, an unset variable or a store type no factory claims
all surface there, with the error naming the store types the chain does support.

Which backends exist is a build fact, so the factory chain is an argument rather than a field:
`liquers-core` supplies `default_store_factory()` (memory, plus filesystem off wasm),
`liquers-store` chains OpenDAL onto it, `liquers-web` chains its own.

## Library defaults

`liquers-lib` configures a different recipe provider from `liquers-core`:

```rust,ignore
let mut builder = liquers_lib::environment::default_environment_builder::<Value, ()>();
```

`liquers-core`'s builder defaults to `RecipeProviderChoice::Trivial` — it has no opinion about
recipes. `liquers-lib`'s configures `Default`, which reads recipes through the store, and that is
what every `liquers-lib` consumer has always got. Building a `DefaultEnvironment` from a bare
`EnvironmentBuilder::new()` would silently stop `-R/` queries resolving, with no compile error.

For the polars command namespace, bring the extension trait into scope:

```rust,ignore
use liquers_lib::environment::PolarsCommandRegistration;
builder.register_polars_commands()?;
```

It is an extension trait because `DefaultEnvironment` is now an alias of a `liquers-core` type, and
Rust permits an inherent `impl` only in the crate that defines the type.

## The readiness guarantee

**When `build()` returns, the asset manager is started.** Command metadata versions have been
refreshed and registered into the dependency manager, so the first evaluation sees a complete
dependency graph.

```rust,ignore
let envref = builder.build()?;
assert!(envref.get_asset_manager().is_started());
```

This is not a formality. Before it existed, `to_ref` spawned startup as a detached task and
returned, and `AssetManager::register_plan_dependencies` skips any dependency whose version the
manager does not yet know:

```rust,ignore
if let Some(ver) = self.dependency_manager().get_version(&plan_dep.key).await { /* register */ }
```

So a plan evaluated in that window registered **no** dependency edges — silently, with no error
anywhere — and nothing ever invalidated the assets built from it. Tests had to `sleep` and hope.
That is `QUEUED-MANAGER-STARTUP-READINESS`, and the fix is structural: the manager is constructed,
installed and started inside the sequence that produces the `EnvRef`, so an unready reference is
not reachable rather than merely unlikely.

`start()` is idempotent. For metadata changed *after* construction, `refresh_command_versions()`
re-registers versions and returns the dependency keys that changed;
`refresh_command_versions_and_expire()` also cascades the expiration those changes imply.

## When `to_ref` is the right call

The builder is recommended, not mandatory. `Environment::to_ref` and `Environment::try_to_ref`
remain supported for an environment assembled by hand:

```rust,ignore
let mut env = SimpleEnvironment::<Value>::new();
register_command!(&mut env.command_registry, fn greet(state) -> result)?;
let envref = env.to_ref();                 // ready, same as build()
let envref = env.try_to_ref()?;            // the same, reporting a startup error
```

Both run the same readiness sequence as `build()`. `to_ref` panics on a startup error because its
signature is infallible; neither built-in manager can produce one, since startup writes an
in-memory map.

Prefer the builder when you are configuring services, because it reports errors and reads in one
direction. Reach for `to_ref` when you already have an environment value in hand.

`EnvRef::new` is **deprecated** and is the one construction path that is genuinely unsafe: it wraps
the environment in an `Arc` and does nothing else, so the manager is never constructed or started.

## Implementing your own `Environment`

The builder owns the concrete built-in environment types. An integration with its own global
services implements `Environment` directly, and gets the same readiness guarantee — provided it
implements one hook correctly.

```rust,ignore
struct MyEnvironment {
    command_registry: CommandRegistry<Self>,
    // The manager cannot exist before the EnvRef does, so it lives in a slot.
    asset_store: OnceLock<Arc<ImmediateAssetManager<Self>>>,
    // … your own global services …
}

impl Environment for MyEnvironment {
    // … associated types and accessors …

    /// The whole obligation: construct with this reference, install, start.
    fn init_with_envref(&self, envref: EnvRef<Self>) -> Result<(), Error> {
        let manager = Arc::new(ImmediateAssetManager::new(envref));
        let _ = self.asset_store.set(manager.clone());
        manager.start()
    }
}

let envref = MyEnvironment::new().try_to_ref()?;
```

`try_to_ref` and `to_ref` are provided methods; `init_with_envref` is the only part that varies, and
it is what makes one generic readiness sequence serve every environment. The metadata-version
refresh happens in the provided body, so a custom environment gets it without knowing it exists.

**The deferred slot is not optional.** The manager needs an `EnvRef` and the environment owns the
manager — a cycle. Something has to be filled in after the `EnvRef` exists, and `init_with_envref`
is the moment when that is safe: it runs before anything else can observe the reference.

## Related

- Reference: [DOC-04 Environment, Context and Evaluation](../reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md)
- Reference: [Environment Configuration](../reference/ENVIRONMENT_CONFIG.md)
- Reference: [Store Configuration](../reference/STORE_CONFIG_FSD.md)
- Guide: [Language Integration](./LANGUAGE-INTEGRATION_GUIDE.md)
- Design: [`design/environment-builder/`](../design/environment-builder/)
- Executable evidence: `liquers-core/tests/environment_builder.rs`,
  `liquers-core/tests/manager_parametric.rs`

## History

| Date | Change | Source |
|---|---|---|
| 2026-08-31 | Created: builder, kind selection, configuration document, the readiness guarantee, when `to_ref` applies, and implementing a custom environment. | `design/environment-builder/phase-5` |

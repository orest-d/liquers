---
id: ENVIRONMENT-MANAGER-REFERENCE-CYCLE
kind: issue
title: Environment and asset manager hold each other with strong Arcs, so every environment leaks
status: draft
priority: P2
complexity: M
area: [core/assets, core/context]
design: environment-builder
created: 2026-08-27
github:
---
## Problem

An `Environment` owns its asset manager with a strong `Arc`, and the manager owns the environment
back with a strong `Arc`. Neither is ever dropped.

- `SimpleEnvironment` (and each of the other three built-in environments) holds
  `asset_store: Arc<DefaultAssetManager<Self>>`.
- `DefaultAssetManager` holds `envref: std::sync::OnceLock<EnvRef<E>>`, and
  `EnvRef<E>(pub Arc<E>)` is a **strong** reference.
- `ImmediateAssetManager` holds the same `OnceLock<EnvRef<E>>`.
- `Environment::init_with_envref` closes the cycle by calling `AssetManager::set_envref(envref)`.

Observed on `SimpleEnvironment<Value>`:

```rust
let envref = SimpleEnvironment::<Value>::new().to_ref();
assert_eq!(std::sync::Arc::strong_count(&envref.0), 2); // caller + manager back-reference
```

Dropping the caller's `EnvRef` leaves the count at 1, held by the manager the environment itself
owns.

**There are two independent cycles, not one.** The `set_envref` back-reference above is only the
first. Every cached asset forms a second:

```
Environment -> Arc<DefaultAssetManager> -> assets / query_assets maps
            -> AssetRef -> Arc<RwLock<AssetData<E>>> -> AssetData.envref: EnvRef<E> -> Arc<Environment>
```

`AssetData<E>` holds `envref: EnvRef<E>`, a strong `Arc<E>` (`liquers-core/src/assets.rs`), and the
manager's keyed and query maps hold those assets. So the second cycle exists once any asset is
cached, whether or not the manager's own back-reference is fixed. **Weakening only the manager's
back-reference does not fix the leak.** Both edges have to be addressed. The environment, its command registry, its type registry, its store handle, the asset
manager, and every asset cached in the manager's `assets` / `query_assets` maps are all retained
for the lifetime of the process.

## Impact

Bounded and benign for a server that builds one environment at startup, which is why it has not
been noticed. Assessed and deliberately deferred during the `environment-builder`
design: a typical system holds one environment — more precisely at most one per realm — alive for
the whole process lifetime, so the leak has no practical cost there. A **soft reboot** that tears
down and rebuilds the environment is the case where it would surface, and is the reason this stays
open rather than being closed as not-planned. It is a real leak wherever environments are created repeatedly: per-test
environments, per-request or per-tenant environments, a Wasm page that rebuilds its environment on
reload, and the `liquers-web` paths that call `to_ref` more than once. Leaked assets also keep
their cached values alive, so the leaked footprint is not a fixed per-environment constant.

## Expected behavior

Dropping the last externally held `EnvRef` should drop the environment and its asset manager.

## Fix direction

Both edges must become non-owning:

1. The manager's back-reference — `Weak<E>` behind the existing accessor, with `get_envref`
   upgrading (failure to upgrade meaning "the environment is gone", reachable only during
   teardown). `Arc::new_cyclic` can establish it at construction rather than by post-construction
   back-fill.
2. `AssetData::envref` — the harder half. An asset held only by the manager must not keep the
   environment alive, but an asset handed to a caller mid-evaluation must. Options to weigh:
   `Weak<E>` in `AssetData` with upgrade at use; or the manager holding its cached assets weakly
   and reconstructing on demand.

There are 78 `get_envref()` call sites (68 in `assets.rs`) plus 16 `ImmediateAssetManager::envref()`
sites, so whether the accessor keeps returning `EnvRef<E>` (panicking at teardown) or starts
returning `Option`/`Result` is the main cost driver.

This is entangled with how the environment/manager construction cycle is resolved in general, so
it is recorded against the `environment-builder` design, which is building an
environment builder that owns that construction. Fixing it there is preferable to a separate
change; fixing it separately is possible if the builder work does not land.

## Verification

1. `Arc::strong_count` on a freshly built `EnvRef` is 1, not 2.
1b. Caching an asset does not raise the environment's strong count.
2. A `Drop`-instrumented environment is dropped when the last `EnvRef` goes out of scope.
3. Building and dropping many environments in a loop does not grow retained memory.
4. `get_envref` still returns a usable `EnvRef` from inside an evaluation.

## Status after `design/environment-builder` (2026-08-31)

Unchanged, by decision, and **not worsened**. That design kept the manager's back-reference strong
(Phase 1, question 3), so both cycles are exactly as described above.

One detail moved: the manager's back-reference is no longer a `OnceLock<EnvRef<E>>` filled in after
construction, but a plain `EnvRef<E>` field supplied to the constructor. The strong count is the
same; what changed is that there is no unset state around it. The environment now holds its manager
in a `OnceLock` instead, written once by `Environment::init_with_envref`.

That relocation is where a future fix would go: `init_with_envref` is now the single place that
constructs the manager with a reference to its environment, so weakening that reference is one
function to change rather than four.

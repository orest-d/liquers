---
id: ENVIRONMENT-MANAGER-REFERENCE-CYCLE
kind: issue
title: Environment and asset manager hold each other with strong Arcs, so every environment leaks
status: draft
priority: P2
complexity: M
area: [core/assets, core/context]
design: queued-manager-startup-readiness
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
owns. The environment, its command registry, its type registry, its store handle, the asset
manager, and every asset cached in the manager's `assets` / `query_assets` maps are all retained
for the lifetime of the process.

## Impact

Bounded and benign for a server that builds one environment at startup, which is why it has not
been noticed. It is a real leak wherever environments are created repeatedly: per-test
environments, per-request or per-tenant environments, a Wasm page that rebuilds its environment on
reload, and the `liquers-web` paths that call `to_ref` more than once. Leaked assets also keep
their cached values alive, so the leaked footprint is not a fixed per-environment constant.

## Expected behavior

Dropping the last externally held `EnvRef` should drop the environment and its asset manager.

## Fix direction

Make the manager's back-reference non-owning — `Weak<E>` behind the existing accessor, with
`get_envref` upgrading (and the failure to upgrade meaning "the environment is gone", which is
only reachable during teardown). `Arc::new_cyclic` can establish it at construction rather than by
post-construction back-fill.

This is entangled with how the environment/manager construction cycle is resolved in general, so
it is recorded against the `queued-manager-startup-readiness` design, which is building an
environment builder that owns that construction. Fixing it there is preferable to a separate
change; fixing it separately is possible if the builder work does not land.

## Verification

1. `Arc::strong_count` on a freshly built `EnvRef` is 1, not 2.
2. A `Drop`-instrumented environment is dropped when the last `EnvRef` goes out of scope.
3. Building and dropping many environments in a loop does not grow retained memory.
4. `get_envref` still returns a usable `EnvRef` from inside an evaluation.

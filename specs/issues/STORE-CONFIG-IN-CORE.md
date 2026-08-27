---
id: STORE-CONFIG-IN-CORE
kind: feature
title: Store configuration types live in liquers-store, so liquers-core cannot own an environment configuration
status: draft
priority: P2
complexity: M
area: [core/store, store, web]
design: queued-manager-startup-readiness
created: 2026-08-27
github:
---
## Problem

`StoreRouterConfig`, `StoreConfig` and `expand_env_vars` live in `liquers-store/src/config.rs`.
`liquers-store` depends on `liquers-core`, so nothing in `liquers-core` can embed them. Any
configuration type that must describe a store — an `EnvironmentConfig` covering store, recipe
provider and asset-manager options in one document — is therefore pushed up to `liquers-store` or
above, away from the `EnvironmentBuilder` it configures.

This blocks the shape the JavaScript and Python bindings need: a host sets an environment up from
**two documents** — one configuring the environment (store included), one declaring commands — and
both should be parseable by `liquers-core` without a store backend in the graph.

## Evidence the split is already wanted

`liquers-store`'s `opendal` feature is optional *specifically* so a wasm consumer can take the
crate for its configuration alone. Its own comment says so:

> Optional so that a wasm32 consumer can depend on this crate for its configuration and builder
> alone: OpenDAL is a large, native-oriented dependency, and `liquers-web` needs
> `StoreConfig`/`StoreRouterBuilder` without it.

So `liquers-web` already depends on `liquers-store` with the backends switched off, purely to reach
data types. That is the dependency this change removes. `liquers-web/src/environment.rs:394` already
calls `StoreRouterConfig::from_json`, so the JSON path exists and is simply in the wrong crate.

## What moves, and what does not

The types are **pure data** — `StoreConfig::config` is a `HashMap<String, serde_json::Value>`, and
nothing in the config types touches a backend.

| Moves to `liquers-core` | Stays in `liquers-store` |
|---|---|
| `StoreRouterConfig`, `StoreConfig` | `StoreRouterBuilder` |
| `from_yaml` / `from_json` / `from_toml` | `StoreFactory` and the registered factories |
| `expand_env_vars` and the per-config expansion | `OPENDAL_STORE_TYPES`, `is_opendal_store_type` |
| | every backend implementation, `opendal` |

**No new dependency for `liquers-core`:** it already declares `serde`, `serde_derive`,
`serde_json` and `serde_yaml`. Only `toml` would be added, and it is already optional behind a
feature in `liquers-store`; carry it across the same way.

`OPENDAL_STORE_TYPES` is arguably data too, but it names backends `liquers-core` cannot build, so
leaving it with the factories keeps core free of backend knowledge. Validation of a store *type*
stays where the types are implemented.

## Expected behavior

`liquers-core` can define a configuration type that embeds `StoreRouterConfig`, and
`liquers-store` turns that configuration into an actual store through its factories. A host binding
parses both documents against `liquers-core` alone.

## Fix direction

Move the two structs and the expansion helpers into a new `liquers-core/src/store_config.rs`,
re-export them from `liquers-store::config` so no existing import breaks, and move the `toml`
feature across. Mechanical; the risk is import churn rather than behavior.

## Verification

1. `liquers-store::config::StoreRouterConfig` still resolves (re-export) — no call site edited.
2. `liquers-core` builds with no new non-optional dependency.
3. `liquers-web` builds without depending on `liquers-store` for configuration.
4. `expand_env_vars` doc test and the config parsing tests pass unmoved.
5. `bash scripts/check-build-matrix.sh` — the `liquers-store` feature split is one of its rows.

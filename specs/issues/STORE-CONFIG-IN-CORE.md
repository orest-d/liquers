---
id: STORE-CONFIG-IN-CORE
kind: feature
title: Store configuration types live in liquers-store, so liquers-core cannot own an environment configuration
status: closed
priority: P0
complexity: L
area: [core/store, store/config, web]
design: store-factories-in-core
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

## Priority rationale

Recorded **P0** by maintainer decision (2026-08-27): this is a prerequisite for the document-driven
JavaScript and Python setup path, and that work cannot start until it lands.

Note the tension with `DOCS_STRUCTURE_GUIDE.md` §4.4, which defines P1 as "something blocking
planned work" and reserves P0 for incorrect results, data loss, a panic on a supported path, or a
documented feature that does not work. This issue is none of those; it is scheduling weight, applied
deliberately. Either §4.4 should gain a clause for hard prerequisites, or this should settle at P1.

## Resolution

**Closed 2026-08-29** by [`design/store-factories-in-core/`](../design/store-factories-in-core/),
with a scope wider than filed and a verification list corrected twice. `liquers-core` now owns
`store_config.rs` and `store_factory.rs`; `liquers-store` keeps the OpenDAL backends;
`liquers-web` depends on it not at all.

### Verification, as originally written — two of five items were wrong

1. ~~`liquers-store::config::StoreRouterConfig` still resolves (re-export) — no call site edited.~~
   **Rejected at a gate.** No backwards compatibility was required, and a `liquers-store` re-export
   of a core type is precisely the shadowing to avoid. `config.rs` and `store_builder.rs` are
   deleted; call sites moved.
2. `liquers-core` builds with no new non-optional dependency. ✅ Only `toml`, optional and out of
   `default`.
3. ~~`liquers-web` builds without depending on `liquers-store` **for configuration**.~~
   **Understated.** Under the data-only boundary it was unachievable, since `liquers-web` also uses
   `StoreRouterBuilder` and implements `StoreFactory`. The scope widened to move those too, and the
   delivered result is stronger: **`liquers-web` has no `liquers-store` dependency at all.** ✅
4. `expand_env_vars` doc test and the config parsing tests pass unmoved. ✅ All 11 passed with
   assertions unchanged — the test of whether the move preserved behaviour.
5. `bash scripts/check-build-matrix.sh`. ✅ 14/14, with three new `liquers-core` rows.

### What actually shipped, beyond the issue

`StoreFactory` and `StoreRouterBuilder` moved as well; `claims` became `resolve` so a factory can
infer the store type; chaining is first-wins with no built-in fallback; an unrecognised type reports
what the build supports; factories describe their arguments, with `ArgumentCoverage` distinguishing
a specification from guidance about an externally-owned surface. Complexity was reclassified M → L.

Four issues were filed and left open rather than absorbed:
[`STORE-OPENDAL-SERVICES-NOT-ENABLED`](STORE-OPENDAL-SERVICES-NOT-ENABLED.md) (P0),
[`CORE-NO-DEFAULT-FEATURES-BROKEN`](CORE-NO-DEFAULT-FEATURES-BROKEN.md),
[`STORE-OPENDAL-LIST-OPTION-MISPARSED`](STORE-OPENDAL-LIST-OPTION-MISPARSED.md) and
[`CORE-CONFIGURATION-ERROR-KIND`](CORE-CONFIGURATION-ERROR-KIND.md). Two steps of the plan —
deriving OpenDAL argument names, and the offline S3 tests — are deferred on the first of those.

### The §4.4 priority tension, still unresolved

This issue was recorded P0 by maintainer decision as a hard prerequisite, while
`DOCS_STRUCTURE_GUIDE.md` §4.4 reserves P0 for incorrect results, data loss, a panic, or a
documented feature that does not work. The tension outlives this issue and is recorded in
`design/environment-builder/DESIGN.md`; closing this does not settle it. Worth noting that
`STORE-OPENDAL-SERVICES-NOT-ENABLED`, filed during this work, is a §4.4 P0 on the letter of the
rule — a documented feature that does not work.

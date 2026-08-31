---
id: WEB-STORE-CONFIG-NOT-APPLIED-THROUGH-ENVIRONMENT-CONFIG
kind: issue
title: liquers-web hand-rolls the environment configuration that EnvironmentConfig will own
status: draft
priority: P3
complexity: M
area: [web, core/store, core/context]
design: 
created: 2026-08-31
github:
---
## Problem

`liquers-web` retains a store configuration and re-applies it to every environment it constructs,
by hand:

- `STORE_CONFIG: RefCell<Option<StoreRouterConfig>>` and
  `STORE_OBJECTS: RefCell<Vec<(String, js_sys::Object)>>` thread-locals
  (`liquers-web/src/environment.rs`).
- `apply_store(env)` in the same file rebuilds a router from that configuration through
  `WebStoreFactory` and calls `env.with_async_store(...)`. Its doc comment states the obligation:
  "Called from **every** path that constructs an environment which the singleton will use, so the
  store cannot be lost by a rebuild."

That is `EnvironmentConfig::with_config` written out by hand, one crate above where the abstraction
belongs. `design/environment-builder` puts `EnvironmentConfig` in `liquers-core` — `store`,
`recipes` and `assets` in one serde document, applied to an `EnvironmentBuilder` — which makes the
`liquers-web` code duplication rather than invention.

## Impact

Low, and entirely maintenance: the hand-rolled path works, and its correctness is enforced by the
crate's own tests. The cost is that the two paths can drift — a field added to `EnvironmentConfig`
is not automatically a field `liquers-web` retains and replays, and a store silently lost on rebuild
is worse than a command silently lost, because the symptom is a `-R/` query that stops resolving.

Not a blocker for anything: the environment-builder design explicitly leaves this migration out, on
the grounds that the rebuild path is the crate's most delicate code and that project already carries
a readiness fix plus a new configuration layer.

## Expected behaviour

Once `EnvironmentConfig` exists in `liquers-core`, `liquers-web` retains an `EnvironmentConfig`
rather than a bare `StoreRouterConfig`, and `apply_store` becomes
`builder.with_config(config, factory)`. The `STORE_OBJECTS` mapping stays where it is — a JavaScript
object cannot be written into a configuration document, so the document carries a name and the crate
holds the objects, which is the design's intent rather than a workaround.

The rebuild path must keep its current guarantee: every construction of an environment the singleton
will use replays the full retained configuration.

## Discovery

Found while reviewing `design/environment-builder` against `HEAD` after `store-factories-in-core`
merged. Recorded there as Phase 3 open question 9, with the recommendation to migrate later rather
than inside that project.

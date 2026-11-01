---
id: STORE-CONFIG-HANDLERS-USAGE-UNDOCUMENTED
kind: issue
title: The API docs do not say who runs store_config handlers or how a store gets them
status: closed
priority: P3
complexity: S
area: [docs, core/store, store/config]
design: store-config-handlers-usage-docs
created: 2026-11-01
github: 230
---
## Problem

Filed by Orest as GitHub #230, "Missing store_config handlers documentation":

> It is not well documented how the handlers and store_config pipelines are used in the store.
> Does each store need to implement it? Is there a wrapper that calls it automatically? (There
> should be.) Are only some stores implementing it? How is it called? The API documentation
> should be improved, it should say where is the functionality used and provide an example (or
> point to an existing example).

The answers exist in the code and partly in `specs/guides/STORE_CONFIG_GUIDE.md`, but not where a
reader of the API starts:

- `liquers_core::store::realm` and `liquers_core::store::handlers` describe how entries compile,
  not who runs the compiled chains or when.
- `MetadataPipelineStore` (`store/pipeline.rs`) is the wrapper that runs them, but its docs do not
  say that `StoreRouterBuilder` applies it automatically from a mount's `metadata` section, nor that
  no store implements handlers itself.
- `StoreConfig::metadata`, `MetadataConfig` and the `AsyncStore` data-only methods do not point at
  the wrapper; `METADATA_DEFAULTS` reads as if the defaults applied to every mount, when they apply
  only to a mount that has a section.
- No runnable example shows a section taking effect.
- A store given to `EnvironmentBuilder::with_async_store` is never wrapped, and nothing says so.

## Impact

A store implementer may think every store has to call handlers, and a configurer may expect the
default chains on a mount without a section. Both are wrong, and only reading `store_factory.rs`
tells them.

## Expected behaviour

The rustdoc of the realm, the handlers, the pipeline, the configuration types and the data-only
`AsyncStore` methods says: stores do not implement handlers; `MetadataPipelineStore` runs them;
`StoreRouterBuilder` (and so `EnvironmentBuilder` with a store configuration) wraps a mount with a
non-empty `metadata` section automatically; a backend needs data-only access for that; which
in-tree stores have it; and points at a runnable example. The guides answer the same questions.

## Discovery

Reported by Orest, 2026-11-01. Investigation traced the call path
`StoreRouterBuilder::build_router` → `store_factory::pipeline` → `MetadataPipelineStore::new`, with
`EnvironmentBuilder::build` calling `build_store`.

## Resolution

Fixed by design `store-config-handlers-usage-docs` (2026-11-01): rustdoc of the realm, handlers,
pipeline, configuration types and `AsyncStore::data_capabilities`, a runnable example in the
`store::pipeline` module docs, and "Who runs the handlers" in `STORE_CONFIG_GUIDE.md` §1. The one
case with no automatic wrapper, a store built in code, is `STORE-METADATA-SECTION-CANNOT-WRAP-A-BUILT-STORE`.

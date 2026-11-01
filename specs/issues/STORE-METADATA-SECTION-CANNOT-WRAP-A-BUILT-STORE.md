---
id: STORE-METADATA-SECTION-CANNOT-WRAP-A-BUILT-STORE
kind: feature
title: A metadata section cannot be applied to a store built in code
status: draft
priority: P3
complexity: S
area: [core/store, store/config]
design: metadata-section-on-built-store
created: 2026-11-01
github:
---
## Problem

The only route from a `MetadataConfig` to a `MetadataPipelineStore` is a `StoreRouterBuilder`
mount: the function that compiles a section and wraps a backend, `pipeline` in
`liquers-core/src/store_factory.rs`, is private. A store constructed in code — given to
`EnvironmentBuilder::with_async_store`, built by a language binding, or assembled in a test — gets
no handlers unless the caller rebuilds that function by hand from `PipelineParts`,
`StoreConfigFunctions::compile`, `compile_component` and `version_verification`, as
`liquers-core/tests/external_change_integration.rs` and `tests/storage_none_computed_values.rs`
do.

## Impact

Handlers are automatic only for stores that a factory can name in a configuration document. Code
that already holds a store has to copy builder internals, and drifts when the builder changes (the
`version_verification` step is easy to miss).

## Expected behaviour

A public call that takes a backend and a `MetadataConfig` (plus the registry and functions) and
returns the wrapped store, used by `StoreRouterBuilder` itself so there is one path.

## Discovery

Found while answering STORE-CONFIG-HANDLERS-USAGE-UNDOCUMENTED (GitHub #230), whose reporter
expected "a wrapper that calls it automatically". The automatic wrapper exists for configured
mounts; this is the case it does not cover.

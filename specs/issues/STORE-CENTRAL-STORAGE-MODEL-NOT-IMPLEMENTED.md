---
id: STORE-CENTRAL-STORAGE-MODEL-NOT-IMPLEMENTED
kind: feature
title: The central(mount) storage model is declared but refuses to build
status: draft
priority: P3
complexity: M
area: [core/store, store/config]
design: central-storage-model
created: 2026-10-15
github:
---
## Problem

The storage model `central` (argument `mount`: records kept in another mount of the router) is
registered in the `store_config` realm and exported, but its builder refuses
(`liquers-core/src/store/storage.rs`, `build_central`; description "Declared; not implemented
yet"). Router-level central storage is exercised only through the test-only `memory_central`
model, registered under `cfg(test)` and the `store-conformance` feature.

## Problem Example

```yaml
metadata: { storage: central-meta, on_read: media_type-fill/file_facts }
stores:
  - type: filesystem
    prefix: docs
    config: { path: ./docs }
    read_only: true
  - type: filesystem
    prefix: meta
    config: { path: ./meta }
```

Today: `StoreRouterBuilder::build_store` resolves the mount `meta` through `ComponentContext`, then
`build_central` (`liquers-core/src/store/storage.rs`) returns `not_supported`: "central storage in
mount 'meta' is declared but not implemented yet". Should be: the store builds; `set_metadata` of
`docs/a.md` with a title writes nothing under `./docs`, writes the record to
`./meta/__metadata__/docs/a.md`, and a rebuilt store reads the title back.

## Impact

A configuration cannot keep the records of several read-only mounts in one writable mount, which
is the main use of a router-level `metadata` section (`StoreRouterBuilder::build_store`). Writing
`storage: central-meta` fails at build.

## Expected behaviour

`central-<mount>` resolves the named member of the router and keeps records there by key,
following the `MemoryCentralStorage` behaviour that conformance suite `a5_memory_central` checks
(members' own storage as backup, reads preferring the central record).

## Discovery

Deferred by `design/store-adapters/` (Phase 2 §3, AC-12 implemented with the test model only).

## Interface in place (2026-10-16)

At the user's request the design now guarantees the model can be built. The root cause of the
refusal was that a component builder was a plain function of its parameters and could not reach
the router's members. `ComponentBuilder` now also receives a `ComponentContext`
(`liquers-core/src/store/realm.rs`), which holds the router's members by mount prefix for a
router-level section and nothing for a member's own section. `build_central` resolves the mount
through it (unknown mount, or use in a member section: a configuration error) before refusing;
test `central_storage_resolves_its_mount_through_the_router`. What remains is the model itself:
`MetadataStorage` with `object_store` returning the mount, a key mapping into it (records under the
mount's prefix), its reserved names and listing, run through conformance suite `A5`.

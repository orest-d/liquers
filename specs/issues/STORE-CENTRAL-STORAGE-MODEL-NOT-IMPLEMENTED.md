---
id: STORE-CENTRAL-STORAGE-MODEL-NOT-IMPLEMENTED
kind: feature
title: The central(mount) storage model is declared but refuses to build
status: draft
priority: P3
complexity: M
area: [core/store, store/config]
design:
created: 2026-10-15
github:
---
## Problem

The storage model `central` (argument `mount`: records kept in another mount of the router) is
registered in the `store_config` realm and exported, but its builder refuses
(`liquers-core/src/store/storage.rs`, `build_central`; description "Declared; not implemented
yet"). Router-level central storage is exercised only through the test-only `memory_central`
model, registered under `cfg(test)` and the `store-conformance` feature.

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

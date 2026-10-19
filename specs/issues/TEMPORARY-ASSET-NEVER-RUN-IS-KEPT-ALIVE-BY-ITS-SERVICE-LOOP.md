---
id: TEMPORARY-ASSET-NEVER-RUN-IS-KEPT-ALIVE-BY-ITS-SERVICE-LOOP
kind: issue
title: A temporary asset that is never run is kept alive forever by its own service-message loop
status: draft
priority: P3
complexity: S
area: [core/assets]
created: 2026-10-18
github:
---

## Problem

`AssetRef::new_temporary` (`liquers-core/src/assets.rs`) spawns
`process_service_messages` on a strong clone of the new `AssetRef`. That loop waits on the asset's
own service channel, whose sender lives in the same `AssetData`, so the channel never closes while
the asset exists. The loop ends only on `JobFinishing`. A temporary asset that is created and never
run therefore keeps itself alive, and its task never ends: the asset leaks, and weak references to
it (for example in the dependency manager's dependent lists) never die.

Every such asset also logs "Starting to process service messages for asset 0" to stderr.

Found while fixing `DEPENDENT-ASSET-WEAK-REFERENCES-ARE-NEVER-PRUNED`: that fix's tests had to
build assets with `AssetData::new_temporary(..).to_ref()` instead, because `AssetRef::new_temporary`
assets never drop.

## Expected

Dropping the last outside reference to a temporary asset that never ran frees it and ends its
service loop. For example, the loop could hold a weak reference, or the loop could start only when
the asset is first run.

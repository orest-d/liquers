---
id: TEMPORARY-ASSET-NEVER-RUN-IS-KEPT-ALIVE-BY-ITS-SERVICE-LOOP
kind: issue
title: A temporary asset that is never run is kept alive forever by its own service-message loop
status: closed
priority: P3
complexity: S
area: [core/assets]
design: temporary-asset-never-run-leak
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

## Problem Example

```rust
let asset = AssetRef::new_temporary(envref.clone());   // what interpreter::evaluate does
let weak = asset.downgrade();
drop(asset);
assert!(weak.upgrade().is_none());                      // today: Some, forever
```

## Triage 2026-10-20

Still present at HEAD (`18732dd`): `AssetRef::new_temporary` spawns `process_service_messages` on a
strong clone. Covered by `environment-manager-reference-cycle` (its AC-7 and step 5, a loop holding
a `WeakAssetRef`); coverage record: `temporary-asset-never-run-leak` (`readiness: covered`).

## Resolution (2026-10-22)

Fixed by design `environment-manager-reference-cycle` (step 5): `AssetRef::new_temporary` spawns a
loop that holds a `WeakAssetRef` and the service receiver, so it ends once the asset and its senders
are dropped. Evidence: `teardown08_temporary_asset_loop_exits` and
`teardown10_never_run_temporary_asset_is_freed` (this issue's example) in
`liquers-core/tests/environment_teardown.rs`.


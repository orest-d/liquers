---
id: TEMPORARY-ASSETS-SHARE-ID-ZERO
kind: issue
title: Every temporary asset has id 0, so code that identifies assets by id can confuse them
status: draft
priority: P3
complexity: S
area: [core/assets]
design: temporary-asset-unique-id
created: 2026-10-18
github:
---

## Problem

`AssetData::new_temporary` (`liquers-core/src/assets.rs`) builds every temporary asset with
`id = 0`. That includes the ones the asset manager's `create_temporary_asset` hands out. Any code
that identifies an asset by `id()` can confuse two temporary assets: expiration tracking
(`untrack_expiration(asset_id)`) is one example, and so are log and error messages that name an
asset by id.

Found while fixing `DEPENDENT-ASSET-WEAK-REFERENCES-ARE-NEVER-PRUNED`. Deduplicating dependents by
`id()` would have dropped live temporary dependents, so that fix compares the shared allocation
(`Weak::as_ptr`) instead.

## Expected

Either temporary assets get unique ids, or no code path keys on the id of an asset that can be
temporary. Audit `id()` uses in `assets.rs`, `dependencies.rs` and the expiration tracking.

## Problem Example

`AssetRef::new_temporary(envref.clone()).id()` and
`envref.get_asset_manager().create_temporary_asset().id()` are both `0`. On the queued manager two
concurrent `interpreter::evaluate` calls (each running under a temporary asset) park their
at-capacity dependencies on the same local queue (`JobQueue::push_local_dependency(parent.id(), …)`),
so one drains the other's. Each temporary asset should have its own id.

## Triage 2026-10-20

Still present at HEAD (`18732dd`): `AssetData::new_temporary` calls `new_ext(0, …)`. Design:
`temporary-asset-unique-id` (draw the id from `AssetManager::next_id_for_asset`).

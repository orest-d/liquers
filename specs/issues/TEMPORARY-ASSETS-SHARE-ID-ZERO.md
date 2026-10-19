---
id: TEMPORARY-ASSETS-SHARE-ID-ZERO
kind: issue
title: Every temporary asset has id 0, so code that identifies assets by id can confuse them
status: draft
priority: P3
complexity: S
area: [core/assets]
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

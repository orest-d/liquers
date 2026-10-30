---
id: DROPPED-RECIPE-RESULT-LEAVES-AN-EMPTY-DATA-FILE
kind: issue
title: Removing a computed value with a stored record leaves an empty data file at its key
status: draft
priority: P3
complexity: S
area: [core/assets, core/store]
created: 2026-10-27
github:
---
# Removing a computed value with a stored record leaves an empty data file at its key

## Problem

`AssetManager::remove` on a computed value whose record is stored (`DropComputed`) rewrites the
entry as `Recipe` with `store.set(key, &[], &metadata)`: the record keeps the version, and the data
object becomes **empty bytes**. In a store whose data objects are files people see, such as a mount
over a git checkout with `storage: per_entry-root` and `infer-Source` (the sparse policy of
`design/storage-none-computed-values/`), that leaves an empty `index.md` in the working tree after
a removal, which shows up as a change in `git status` until the value is recomputed.

Under an inferred record (`storage: none`) the data is deleted instead, so this affects only mounts
that store the computed value's record.

## Impact

- A committed generated file (`index.md`) becomes an empty file in the checkout after `remove`.
- Readers outside Liquers cannot tell an empty `Recipe` placeholder from a genuinely empty file.

## Possible directions

- Write the `Recipe` record with `set_metadata` and delete the data object, when the storage model
  keeps records apart from data (`MetadataStorage::records_apart_from_data`), so a record-only key
  remains enumerable (STORE_SEMANTICS §8) without an empty file.
- Keep the empty bytes only for `native` storage, where the record cannot exist without the data.

Found while implementing `design/storage-none-computed-values/` (2026-10-27).

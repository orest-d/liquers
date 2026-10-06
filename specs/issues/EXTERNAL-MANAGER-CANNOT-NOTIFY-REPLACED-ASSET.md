---
id: EXTERNAL-MANAGER-CANNOT-NOTIFY-REPLACED-ASSET
kind: issue
title: An AssetManager implemented outside liquers-core cannot tell a replaced asset it was removed, or store a non-serializable state
status: draft
priority: P3
complexity: S
area: [core/assets]
design: external-manager-replacement-surface
created: 2026-10-02
github:
---

## Problem

Part F made the lifecycle primitives an external manager needs public, but two things the built-in
managers do in `set_binary` / `set_state` stay crate-private:

- `AssetRef::notify_removed` sends the replaced asset its last message once the key holds its new
  value. An external manager can `cancel()` the old asset and drop it from its map, but cannot send
  that message, so a waiter parked on the replaced asset is released by the cancellation only.
- `set_state` in the built-ins registers a new asset holding the given `State`'s value
  (`AssetData`'s `data`, `metadata` and `status` fields are private). An external manager can only
  store a state that has bytes (`State::as_bytes`) and let the next `get` fast-track it; a value
  that cannot be serialized is refused.

`tests/common/minimal_manager.rs` documents both limits.

## Impact

Only managers implemented outside `liquers-core`; none ships. A remote or cluster manager that
needs in-memory-only states or exact waiter semantics on replacement would hit them.

## Direction

Either make `notify_removed` public with a contract doc, and add an `AssetData` constructor that
takes a ready value and metadata, or decide that the external surface stops here and document it in
the manager guide.

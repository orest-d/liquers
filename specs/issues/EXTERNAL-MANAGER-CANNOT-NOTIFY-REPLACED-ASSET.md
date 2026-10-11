---
id: EXTERNAL-MANAGER-CANNOT-NOTIFY-REPLACED-ASSET
kind: issue
title: An AssetManager implemented outside liquers-core cannot tell a replaced asset it was removed, or store a non-serializable state
status: closed
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

## Resolution (2026-10-11)

Both doors opened, as decided on 2026-10-10. `AssetRef::notify_removed` is `pub`, documented as
"once, after `cancel_for_replacement`, before unmapping", and `AssetRef::new_installed(id, key,
state, status, envref)` builds a finished keyed asset from a `State`, accepting only `Source`,
`Override`, `Expired` and `Error` (any other status is `General`). The minimal external manager
(`liquers-core/tests/common/minimal_manager.rs`) now notifies a replaced asset and installs a
supplied state with `new_installed`, so it serves a value that has no byte form. Tests:
`new_installed_accepts_only_written_statuses` (`assets.rs`), the shared scenario
`scenario_replaced_asset_receives_removed_last` (all three managers), and
`scenario_set_state_of_unserializable_value_is_served` (the external manager; the built-ins
validate the declared format and refuse that fixture). `ASSET_MANAGER_IMPLEMENTATION_GUIDE.md`
§5 and §11 updated. Design: `design/external-manager-replacement-surface/`.

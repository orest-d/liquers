---
id: LATE-SAVE-OF-AN-UNREGISTERED-KEYED-ASSET-SURVIVES-REMOVAL
kind: issue
title: A late value write of an unregistered keyed asset (cached false, volatile) lands after the key is removed or replaced
status: draft
priority: P2
complexity: M
area: [core/assets, core/store]
design: unregistered-keyed-write-gate
created: 2026-10-26
github:
---

## Problem

An unregistered keyed asset — a `cached: false` key (`get_uncached_resource_asset`) or a volatile
one — is never put in the manager's key map, but it still writes its value to the store
(`AssetRef::persist_with_status_tracking` → `save_to_store`; in the background on the queued
manager). `AssetManager::remove`, `set_state` and `set_binary` find the live asset through
`lookup_key_asset`, so they never see this one, cannot close its metadata saver, and cannot wait
for its write. A write in progress when the key is removed or replaced therefore lands afterwards.

`replaced-asset-write-gate` (design for `REMOVED-OR-REPLACED-KEY-REVIVED-BY-A-LATE-BACKGROUND-SAVE`)
closes this for the registered owner only, because it relies on reaching the asset through the map.

## Problem Example

`data/u.txt` has a recipe whose cache strategy keeps no result (`cached: none`). `get(data/u.txt)` finishes `Ready`; its background
save is about to call `store.set`. `remove(data/u.txt)` rewrites the stored entry as `Recipe` and
returns. The save then writes `Ready`, and a fresh process fast-tracks the removed value. With
`set_state(data/u.txt, "user")` instead, the stored user value is overwritten by the computed one.
For a volatile key the revived entry is `Volatile`, which the fast track refuses, so the effect is
a stale stored copy rather than a wrong read.

Inferred from reading `liquers-core/src/assets.rs` at `cd6d5da` (unchanged at `70b9005`: both
constructors build the asset with `Some(key)` and never map it; `remove` consults only
`lookup_key_asset`); not reproduced.

## Impact

Same as the registered case, for `cached: false` keys: a removed value reappears after a restart, a
user-supplied value is replaced in the store. Rarer, because `cached: false` with `stored: true` is
uncommon. Workaround: wait for the evaluation's `persistence_status()` before mutating the key.

## Expected behaviour

A keyed mutation also stops, or waits for, the value writes of unregistered assets of the same key.
Possible directions: the manager keeps a per-key registry of in-flight unregistered writers that the
mutations close (a new map); the mutations bump a per-key generation that a write compares under the
asset's write gate; or a conditional store write (`store-conditional-write`).

## Discovery

2026-10-26, bulk design of `REMOVED-OR-REPLACED-KEY-REVIVED-BY-A-LATE-BACKGROUND-SAVE`
(`replaced-asset-write-gate`), whose fix reaches assets only through the key map.

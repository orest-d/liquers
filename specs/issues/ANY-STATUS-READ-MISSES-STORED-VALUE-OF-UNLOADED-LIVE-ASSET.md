---
id: ANY-STATUS-READ-MISSES-STORED-VALUE-OF-UNLOADED-LIVE-ASSET
kind: issue
title: Recovery reads return None while a concurrent get has mapped the key but not loaded it
status: closed
priority: P3
complexity: S
area: [core/assets]
design: recovery-read-defers-placeholder
created: 2026-10-02
github:
---
## Problem

`AssetManager::get_any_status` and `get_binary_any_status` (`liquers-core/src/assets.rs`) answer
from the live asset whenever `lookup_key_asset` finds one, and from the store only otherwise.
`get` maps a placeholder asset (status `None`/`Recipe`) *before* it fast-tracks it from the store.
A recovery read that lands between the two finds the placeholder, which holds no value, and returns
`Ok(None)` — although the store holds the value the next instant's fast track loads.

## Impact

A caller of the recovery reads (the axum assets endpoint, `key_handlers.rs`, uses
`get_binary_any_status`) can be told that a stored key has no data, under concurrent access to the
same key. Retrying succeeds. Not verified beyond the observation below.

## Expected behaviour

A live asset that has not produced or loaded anything yet should defer to the store, as `remove`
and `expire` already do for such statuses (`live_status_defers_to_store`).

## Discovery

Observed 2026-10-02 in a first draft of
`external_change_integration::concurrent_reads_apply_external_change_once`, which mixed `get` with
`get_binary_any_status` on one key: a recovery read returned `None` for a key whose value was in the
store. The final test uses only the recovery reads.

## Resolution (2026-10-07)

Fixed by design `recovery-read-defers-placeholder`. The manager-level recovery reads
`get_any_status` / `get_binary_any_status` now defer a live asset whose status is `None` or
`Recipe` (a placeholder that has produced nothing yet) to the store, using the existing
`live_status_defers_to_store` rule that `remove` already applies. Other live statuses answer as
before.

Evidence: `recovery_reads_defer_placeholder_to_store_default`,
`recovery_reads_defer_placeholder_to_store_immediate` (`liquers-core/src/assets.rs`).

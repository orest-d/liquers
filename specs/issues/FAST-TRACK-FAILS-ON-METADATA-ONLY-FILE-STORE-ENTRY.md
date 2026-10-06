---
id: FAST-TRACK-FAILS-ON-METADATA-ONLY-FILE-STORE-ENTRY
kind: issue
title: Requesting a key stored metadata-only on a file store fails instead of recomputing
status: closed
priority: P2
complexity: S
area: [core/assets, core/store]
created: 2026-10-06
github:
---
## Problem

When a keyed value has no byte form, `set_state` stores its metadata only. On a file store
(`AsyncFileStore`, and any store that answers a metadata-only key's data with `KeyNotFound`, as
`STORE_SEMANTICS.md` §2 describes), the next request for the key went through
`AssetData::try_fast_track`, whose `store.get(&key).await?` propagated `KeyNotFound` out of the
fast track and out of the manager's `get`. The request failed with "Key not found" although the key
has a recipe that could re-derive the value.

## Impact

Any value that cannot be serialized (UI elements, egui widgets, foreign handles, non-manifest
`RecordSource`s without `stored: false`) became unreadable after a restart on a file-backed
deployment, until its sidecar was deleted by hand.

## Resolution (2026-10-06)

Fixed in `liquers-core/src/assets.rs` (`try_fast_track`): a `KeyNotFound` from `store.get` for a
contained key is a metadata-only entry, so the fast track returns `Ok(false)` without logging
corruption, and the recipe re-derives the value. Without a recipe, the key answers exactly like a
key the store does not hold. Other store errors still propagate.

Evidence: `liquers-core/tests/metadata_only_entry_reload.rs`
(`metadata_only_entry_on_file_store_is_recomputed`, which failed before the change with
`KeyNotFound`, and `metadata_only_entry_without_recipe_answers_like_an_absent_key`). The full
`cargo test -p liquers-core --lib --tests` passes.

Found while designing `design/metadata-only-entry-reload/`. The memory store's different
behaviour (it answers empty bytes, which a text format reads as an empty string) remains
`MEMORY-STORE-METADATA-ONLY-ENTRY-READS-AS-EMPTY-BYTES`.

## Discovery

Read off the code during the 2026-10-06 bulk design run and reproduced by the test above.

---
id: QUEUED-MANAGER-EVICTION-RACE
kind: issue
title: The queued manager's cache evictions can delete a replacement asset
status: draft
priority: P2
complexity: S
area: [core/assets]
design:
created: 2026-08-09
github:
---

## Problem

`DefaultAssetManager` evicts a stale-terminal asset in four places by looking the entry up,
comparing its id, dropping the entry guard, and then removing by key:

```rust
let asset_id = asset_ref.id();
if let Some(entry) = self.assets.get_async(key).await {
    if entry.get().id() == asset_id {
        drop(entry);
        let _ = self.assets.remove_async(key).await;   // removes whatever is there NOW
    }
}
continue;
```

`liquers-core/src/assets.rs:4000`, `:4010`, `:4221`, `:4541` — `remove_expired_from_maps` twice
(query and key map), `get_asset`'s query branch, and `get`.

The entry guard is released before the removal, so the removal is unconditional. A second caller
that evicts the same entry and registers a replacement in that window loses the replacement: the
first caller's `remove_async` deletes it despite the different id. The id comparison is doing no
work.

Consequences are a lost cache entry and a duplicated evaluation — not incorrect values, since the
replacement is simply rebuilt on the next request, but the eviction paths are exactly where two
callers are most likely to arrive together, because they are reached when an asset has just
finished in a terminal state.

## Expected behaviour

One atomic conditional removal per site. `scc::HashMap::remove_if_async(key, |v| v.id() ==
asset_id)` evaluates the predicate and removes under a single bucket lock, which is what the code
means to do.

`AssetManager::remove_key_asset_if` already exists and does exactly this for the key map, so three
of the four sites can call it rather than open-coding the sequence. The query-map site needs the
same primitive for `query_assets`, or `remove_expired_from_maps` can grow the conditional form
internally.

`ImmediateAssetManager` is unaffected: its maps are plain `HashMap`s behind a `std::sync::Mutex`,
and its evictions hold the lock across the compare and the remove.

## Discovery

Found on 2026-08-09 by an automated review on
[PR #25](https://github.com/orest-d/liquers/pull/25), which flagged the identical race in the newly
added `remove_key_asset_if`. That one was fixed in the PR by switching to `remove_if_async`; the
four pre-existing instances it was modelled on were left alone, because widening a
keyed-recursion fix into the queued manager's hot path is a separate change. Filed so they are not
forgotten now that the correct primitive exists.

---
id: ASSET-MANAGER-INSERT-KEY-ASSET-NO-OVERWRITE
kind: issue
title: insert_key_asset silently does nothing on the queued manager when the key is taken
status: closed
priority: P2
complexity: S
area: [core/assets]
design: asset-manager-insert-key-asset-semantics
created: 2026-08-09
github:
---

## Problem

The two `AssetManager` implementations disagree about what `insert_key_asset` means when the key
already has an entry, and the trait does not say which is right.

`DefaultAssetManager` (`liquers-core/src/assets.rs:5043`):

```rust
async fn insert_key_asset(&self, key: &Key, asset: AssetRef<E>) {
    let _ = self.assets.insert_async(key.clone(), asset).await;
}
```

`scc::HashMap::insert_async` is insert-if-absent: on a duplicate it returns
`Err((key, value))` and leaves the map unchanged. The `let _ =` discards that, so the call is a
**silent no-op** — the caller believes it registered an asset and did not.

`ImmediateAssetManager` (`:5746`) uses `std::collections::HashMap::insert`, which replaces. Same
trait method, opposite behaviour.

## Impact

Low today, because the only production caller removes first: `AssetManager::set_state` cancels the
existing asset and calls `remove_key_asset` before inserting (`:3096-3100`), so the slot is empty
by the time the insert runs. The defect is therefore latent rather than active.

It is still worth fixing, for two reasons. The trait doc — *"Insert an asset into this manager's
key→asset map"* — promises something one implementation does not do, so any new caller that does
not happen to remove first gets a silent failure on the queued manager and a working replacement on
the inline one. And a discarded `Result` is exactly the shape of error that stays invisible: there
is no log line, no status, and no way to observe it except by reading the map back.

## Expected behaviour

`insert_key_asset` either replaces unconditionally on both managers, or the trait documents
insert-if-absent and both implementations honour it — including returning whether the insertion
happened, so a caller can tell.

Replacing is the better default: it matches the method's name, matches the inline manager, and
matches what `set_state` wants (it removes first only to cancel the old asset, not to make room).
On `scc` that is `entry_async(key).insert_entry(asset)` or a remove-then-insert pair.

Whichever is chosen, the `let _ =` should go — a deliberately ignored result deserves a comment
saying why, and here there is no why.

## Discovery

Found on 2026-08-09 while implementing `specs/design/keyed-recipe-ownership/`. A unit test for the
new `remove_key_asset_if` registered asset A, registered asset B under the same key, and asserted
that removing "if still A" was refused. It failed: B had never been inserted. The test was changed
to remove between the inserts, with a comment pointing here, because it is a test of
`remove_key_asset_if` and should not depend on unsettled insert semantics.

## Resolution

Closed 2026-08-25 by `design/asset-manager-insert-key-asset-semantics`. The public ambiguous
trait method was removed. The built-in managers now use a crate-private insert-if-absent helper,
with duplicate claims observable to internal callers. Keyed mutations are serialized with durable
store work, so recovery cannot reintroduce stale data after a newer external state.

# Phase 2: Solution and Architecture - Deep Listing Includes Recipe Keys at Every Level

## Chosen Solution

Replace the trait default `AssetManager::listdir_keys_deep` with an iterative walk over the
manager's own shallow listing:

```rust
async fn listdir_keys_deep(&self, key: &Key) -> Result<Vec<Key>, Error> {
    let store = self.get_envref().get_async_store();
    let provider = self.get_recipe_provider();
    let mut keys = BTreeSet::new();
    let mut pending = vec![key.clone()];
    while let Some(dir) = pending.pop() {
        let stored: BTreeSet<String> = store.listdir(&dir).await?.into_iter().collect();
        for name in self.listdir(&dir).await? {
            let child = dir.join(&name);
            let descend = if stored.contains(&name) {
                store.is_dir(&child).await?
            } else {
                provider.has_recipes(&child, self.get_envref()).await?
            };
            if descend {
                pending.push(child.clone());
            }
            keys.insert(child);
        }
    }
    Ok(keys.into_iter().collect())
}
```

Notes:
- `self.listdir(&dir)` already unions provider and store names; the extra `store.listdir` is
  only to know which names are store-backed. An implementation may instead inline the union
  (call `assets_with_recipes` and `store.listdir` once each) to avoid listing the store twice —
  an implementation detail; behaviour is identical.
- A loop with an explicit stack rather than `async` recursion (which needs boxing under
  `async_trait`).
- A child both stored and recipe-declared (a computed value persisted under its recipe key) is
  decided by `store.is_dir` — correct, it is a file.

Delete the `listdir_keys_deep` method from `impl<E: Environment> AssetManager<E> for
DefaultAssetManager<E>` (≈7517). `ImmediateAssetManager` already uses the default.

## Rejected Alternatives

- **Minimal patch: add `key` to the recipe pass.** Fixes criterion 4 and the top-level half,
  but not recipe-only subdirectories, and keeps two algorithms (shallow, deep) that must be kept
  in agreement by hand — the drift that caused the bug.
- **Union `store.listdir_keys_deep` with a per-directory recipe pass over every directory
  found** — still blind to recipe-only directories.

## Files and Symbols

| File | Symbol | Change |
|---|---|---|
| `liquers-core/src/assets.rs` | `trait AssetManager::listdir_keys_deep` (default) | rewritten as above; doc comment states the invariant |
| `liquers-core/src/assets.rs` | `DefaultAssetManager`'s `listdir_keys_deep` | deleted |
| `liquers-core/src/assets.rs` | `mod tests` | new tests (Phase 3) |

Callers unchanged: `keys()` (both managers), `removedir`, `liquers-axum` `key_handlers.rs`.

## Errors, Ownership, Sync/Async

Errors from `store.listdir`, `store.is_dir`, `listdir`, `has_recipes` propagate (as today from
the store calls). On a non-directory `key`, `store.listdir` behaviour decides: memory and file
stores answer `Ok([])` for an absent directory (`store-router-empty-prefix`), so the result is
empty, matching today's `store.listdir_keys_deep`. Async, no locks held across awaits (none are
taken).

## API and Compatibility

No signature change. Result grows by the missing recipe keys. `removedir` change recorded in
Phase 1. No `liquers-py`/`liquers-web` override of this method exists (checked).

## Questions

- **Implementation detail — single vs double store listing per directory:** either; prefer the
  inlined union.
- **Implementation detail — recursion order:** irrelevant; result is a sorted set.

## Risk Table

| Aspect | Assessment |
|---|---|
| Likely files | `liquers-core/src/assets.rs` |
| Affected workflows/crates | core managers; `liquers-axum` deep listing; `removedir` |
| Existing-test impact | removedir tests and axum `listdir?deep=true` tests must stay green; a test asserting the old (incomplete) set would need correcting — none found by inspection |
| New validation | invariant test on both managers; root `keys()`; recipe-only directory; removedir unmaps a top-level recipe live asset |
| Compatibility/data | listings grow; no stored data change |
| Concurrency/performance | listing is a snapshot, as before; O(directories) `listdir` calls |
| Security | none |
| Recovery | restore the old default and override |
| Certainty | high |

## Review

Against Phase 1: criteria 1-3 from the algorithm; 4 from root call; 5 from `has_recipes`
branch; 6 from deleting the override. Against code: both bodies, `listdir`, `removedir`,
`remove`'s recipe-only branch, and the axum caller were read at HEAD.

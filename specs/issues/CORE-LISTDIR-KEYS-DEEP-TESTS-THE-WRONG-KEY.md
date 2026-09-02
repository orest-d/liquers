---
id: CORE-LISTDIR-KEYS-DEEP-TESTS-THE-WRONG-KEY
kind: issue
title: The listdir_keys_deep default recurses on the parent's is_dir instead of the child's
status: draft
priority: P2
complexity: S
area: [core/store, store/backends]
design:
created: 2026-09-02
github:
---
## Problem

`AsyncStore::listdir_keys_deep` (`liquers-core/src/store.rs:517`) walks a subtree:

```rust
let keys = self.listdir_keys(key).await?;
let mut keys_deep = keys.clone();
for sub_key in keys {
    if self.is_dir(key).await? {          // <-- `key`, the parent, not `sub_key`
        let sub = self.listdir_keys_deep(&sub_key).await?;
        keys_deep.extend(sub.into_iter());
    }
}
```

The guard tests **`key`** — the directory being listed — rather than **`sub_key`**, the child about
to be recursed into. `key` is a directory in every call that returns anything, so the condition is
effectively constant:

- **It recurses into every child, including data keys.** `listdir_keys_deep` is called on a file,
  which returns whatever that store's `listdir_keys` says about a non-directory. A store answering
  `Ok(vec![])` merely wastes a round trip per file; one that errors makes the whole walk fail.
- **The check it was meant to perform never happens**, so the cost is one `is_dir` per child that
  decides nothing.

`AsyncStoreRouter::listdir_keys_deep` (`:2058`) carries the same code, and the `AsyncStore::keys`
default (`:466`) is built on `listdir_keys_deep` — so this sits under `keys()` for every store that
does not override it.

## Impact

Wasted calls on every deep listing, and a latent failure mode on any store whose `listdir_keys`
does not tolerate being handed a data key. On a remote backend a round trip per file is not
negligible: `keys()` on a bucket of *n* objects makes *n* pointless `listdir_keys` calls.

No incorrect *result* has been observed — the extra recursions return empty on the stores in tree —
which is why it has survived. The defect is that the guard does not do what it says.

## Expected behaviour

```rust
if self.is_dir(&sub_key).await? {
```

Same fix in `AsyncStoreRouter::listdir_keys_deep`. Both are the sort of thing a rule over
`listdir_keys_deep` would pin; the conformance suite does not currently have one
(`STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE`).

## Discovery

Found on 2026-09-02 by the Phase 4 final review of `design/store-conformance-suite/`, while
checking which `AsyncStore` methods no conformance rule covers. `listdir_keys`,
`listdir_keys_deep`, `listdir_asset_info` and `get_asset_info` were all uncovered; reading the
first two turned this up.

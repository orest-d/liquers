---
id: CORE-STORE-KEYS-MEANS-TWO-DIFFERENT-THINGS
kind: issue
title: keys() returns data keys in one store and data keys plus directories and the root in others
status: closed
priority: P2
complexity: S
area: [core/store, store/backends]
design: store-keys-contract
created: 2026-09-02
github:
---
## Problem

`AsyncStore::keys()` has no written contract, and the implementations answer two different
questions.

**`AsyncMemoryStore`** (`liquers-core/src/store.rs:831`) iterates its data map and returns exactly
the keys that hold data:

```rust
async fn keys(&self) -> Result<Vec<Key>, Error> {
    let mut keys = Vec::new();
    let _ = self.data.iter_async(|key, _| { keys.push(key.clone()); true }).await;
    Ok(keys)
}
```

**The trait default** (`:454`), which `AsyncFileStore` inherits, returns the recursive listing plus
the store's own prefix — so **directories and the root key are included**:

```rust
async fn keys(&self) -> Result<Vec<Key>, Error> {
    let mut keys = self.listdir_keys_deep(&self.key_prefix()).await?;
    keys.push(self.key_prefix().to_owned());
    Ok(keys)
}
```

**`AsyncOpenDALStore`** (`opendal_store.rs:434`) does the same, guarding against pushing a duplicate
root.

Measured on the OpenDAL memory backend with one key `sub/deeper/foo.txt`:
`keys() = ["", "sub", "sub/deeper", "sub/deeper/foo.txt"]` — four entries for one stored object.
`AsyncMemoryStore` with the same content returns one.

## Impact

Anything that iterates a store sees a different key set depending on which backend is behind it, and
`AsyncStoreRouter` mixes backends in one namespace, so a single router can return both shapes at
once. A caller that treats every key as readable will call `get` on a directory key; a caller that
counts keys gets a number that depends on directory depth. `test_opendal_dir` and
`test_opendal_subdir` both assert exact `keys().len()` values, which is why the divergence is
visible in the test suite as magic numbers (`== 2`, `== 3`) that a reader cannot derive from the
content.

`P2`: no data loss and no incorrect result within a single backend, but it makes any cross-store
code depend on which store it happens to be talking to.

## Expected behaviour

A decision, written down, and then made true in the implementations. The two candidates:

1. **Data keys only.** `keys()` is "what can be read"; directories come from `listdir`/`is_dir`.
   Cheap for memory-like stores, and it makes `get(k)` valid for every `k` in the result. Requires
   changing the trait default and `AsyncOpenDALStore`.
2. **Every addressable key, directories and root included.** `keys()` is "the whole namespace".
   Matches the current default. Requires changing `AsyncMemoryStore`, and callers must expect keys
   that `get` will refuse.

Whichever is chosen belongs in the store contract reference planned by
`STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE`, and the conformance suite is what would keep
the implementations honest afterwards.

## Discovery

Found on 2026-09-02 while enumerating `AsyncStore` contract divergences for
[`design/opendal-path-mapping/`](../design/opendal-path-mapping/), whose Phase 1 reproduction output
(`keys = ["", "sub", "sub/deeper", "sub/deeper/foo.txt"]`) is the evidence above.

## Resolution

Closed 2026-09-02. `STORE_SEMANTICS.md` §9 settles it — `keys()` returns data keys, the
directories above them, and the store's own prefix, and every returned key starts with that
prefix. `AsyncMemoryStore::keys` was the outlier and now builds the ancestor directories from its
data keys. Enforced by rules `keys01` and `keys02` in `liquers_core::store_conformance`, run
against every in-tree store. See `design/store-conformance-suite/` Phase 4 steps 1 and 10.

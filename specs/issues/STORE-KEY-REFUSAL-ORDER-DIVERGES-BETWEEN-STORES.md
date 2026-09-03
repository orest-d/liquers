---
id: STORE-KEY-REFUSAL-ORDER-DIVERGES-BETWEEN-STORES
kind: issue
title: A key that is both relative and unrepresentable gets a different error type per store
status: draft
priority: P3
complexity: S
area: [core/store, store/backends]
design: sidecar-colliding-keys
created: 2026-09-03
github:
---
## Problem

Two refusals guard a store's path builders — *this is not a store address* (`KeyNotAbsolute`) and
*this store cannot represent that key* (`KeyNotSupported`) — and the two stores that implement both
check them in opposite orders.

`AsyncOpenDALStore` refuses the shape first:

```rust
// liquers-store/src/opendal_store.rs:154
pub fn key_to_path(&self, key: &Key) -> Result<String, Error> {
    self.reject_ambiguous(key)?;   // KeyNotSupported
    PathMap::data(key)             // → key.as_absolute()? → KeyNotAbsolute
}
```

`AsyncFileStore` and `FileStore` run `as_absolute()?` first in every builder
(`liquers-core/src/store.rs:900`, `:909`, `:916`, `:1256`, `:1265`), so once
`SIDECAR-COLLIDING-KEYS` adds their reserved-name check after it, the same key answers the other
way. `../x.__metadata__` is `KeyNotAbsolute` from a file store and `KeyNotSupported` from the
OpenDAL store.

## Impact

Small and latent. A caller that matches on `ErrorType` — the axum error mapper, a retry that
distinguishes "fix the key" from "use another store" — gets a store-dependent answer for one key.
Nothing in-tree matches on it today, and no conformance rule catches it: `KeyRequest::Relative`
offers `data/../../escape.txt`, which is relative but not reserved, so the overlap is never
exercised.

`STORE_SEMANTICS.md` §8 is about to be restated by `SIDECAR-COLLIDING-KEYS` as *reserved names, in
any segment, declared by the store's metadata layout*, and its Phase 3 `reserved05` pins the
file-store order as deliberate. Neither says which order the contract requires, so the divergence
survives into the document that is supposed to settle it.

## Expected behaviour

One order, stated in `STORE_SEMANTICS.md`. `as_absolute()` first is the better of the two: a
relative key is not a store address at all, so no store can answer the representability question
about it, and `keyabs08`/`keyabs09` already depend on that answer.

The fix is one line — `key.as_absolute()?;` at the top of
`AsyncOpenDALStore::reject_ambiguous` — plus a relative-and-reserved shape in `pathmap03` and a
sentence in §8.

## Discovery

Found on 2026-09-03 in the Phase 4 review of `specs/design/sidecar-colliding-keys/`, whose Phase 2
§Error Handling states that the file stores' ordering "matches `AsyncOpenDALStore::reject_ambiguous`
exactly". On this point it is the opposite. Raised there as an advisory finding; filed here so it
is not lost if that design declines to widen its scope.

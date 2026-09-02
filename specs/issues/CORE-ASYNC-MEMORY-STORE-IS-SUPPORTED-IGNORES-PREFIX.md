---
id: CORE-ASYNC-MEMORY-STORE-IS-SUPPORTED-IGNORES-PREFIX
kind: issue
title: AsyncMemoryStore::is_supported claims keys outside its own prefix
status: draft
priority: P1
complexity: S
area: [core/store]
design:
created: 2026-09-02
github:
---
## Problem

`AsyncMemoryStore` is constructed with a prefix and reports it from `key_prefix()`
(`liquers-core/src/store.rs:677`), but `is_supported` ignores it (`:893`):

```rust
fn is_supported(&self, key: &Key) -> bool {
    // The prefix is deliberately not consulted here; that omission predates the key rule and
    // is out of its scope. See `specs/design/store-key-guard/`.
    !key.is_relative()
}
```

So a memory store mounted at `data` reports that it supports `other/thing.txt`. Every other store
checks: `AsyncFileStore` (`:1252`), `FileStore` (`:1490`), `AsyncOpenDALStore`
(`opendal_store.rs:514`) all test `key.has_key_prefix(&self.prefix)`.

The comment is honest about the omission being out of the key-guard design's scope, but it records
the omission rather than resolving it, and no issue tracked it until now.

## Impact

`AsyncStoreRouter::find_store` (`:1921`) selects on `key.has_key_prefix(&store.key_prefix())` **and**
`store.is_supported(key)`. The first test is what saves this today: a memory store at `data` is not
offered `other/thing.txt` because the prefix check in the router rejects it before `is_supported` is
consulted.

The bug therefore bites wherever `is_supported` is consulted **without** the router's prefix test —
`AsyncStoreRouter::is_supported` itself (`:2149`) delegates to the found store, and layering
constructs (`with_overlay`, `with_fallback`, per the trait's own doc comment) are the documented
reason `is_supported` exists as a separate question from the prefix. A store that answers "yes" for
every absolute key cannot participate in such a layering correctly.

It is `P1` rather than `P0` because the router's own prefix test masks it in the configuration
everybody actually uses.

## Expected behaviour

```rust
fn is_supported(&self, key: &Key) -> bool {
    !key.is_relative() && key.has_key_prefix(&self.prefix)
}
```

matching all four other stores. `MemoryStore` (sync, `:1667`) should be checked for the same
omission at the same time.

Note that the change is not free: any test that constructs `AsyncMemoryStore::new(&parse_key("data"))`
and then reads keys outside `data` is relying on the current behaviour and will start failing —
which is the point, but it means the fix needs a test sweep rather than a one-line edit.

## Discovery

Found on 2026-09-02 while enumerating `AsyncStore` contract divergences for
[`design/opendal-path-mapping/`](../design/opendal-path-mapping/). Part of the set collected in
`STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE`.

---
id: CORE-STORE-ROUTER-KEYS-FAILS-ON-AN-EMPTY-MEMBER
kind: issue
title: AsyncStoreRouter::keys fails outright when one member's prefix path does not exist
status: draft
priority: P2
complexity: S
area: [core/store]
design: store-conformance-suite
created: 2026-09-02
github:
---
## Problem

`AsyncStoreRouter::keys()` walks its members through `listdir_keys_deep`. If any member's prefix
path does not exist in that member's backend, the walk returns `Err(KeyNotFound)` and **the whole
enumeration fails** — including the parts contributed by members that answered perfectly well.

Reproduced by the conformance suite's `C3` (`liquers-core/tests/store_conformance_CONF.rs`): a
router over an `AsyncMemoryStore` at prefix `mem` and an `AsyncFileStore` at prefix `files`, with
the file store's root containing no `files/` directory yet:

```
ERROR  keys01 KeyNotFound: Key not found: 'files'
ERROR  keys02 KeyNotFound: Key not found: 'files'
```

An empty prefix directory is the **normal state of a freshly configured store** — nothing has been
written to it yet. `AsyncFileStore` maps the whole key including the prefix onto its root, so until
the first write there is no directory to list.

## Impact

A deployment whose `stores.yaml` names a store that has not been written to yet cannot enumerate
*any* of its keys, and the error names a key the caller never asked about. `keys()` underpins the
`AsyncStore` default for several other methods, so the failure is not confined to direct callers.

The suite's `C3` works around it by creating the member's prefix directory before running, which
is exactly the kind of setup step that hides a defect if nobody writes it down.

## Expected behaviour

A member that cannot enumerate contributes nothing rather than failing the router. Either
`AsyncStoreRouter::keys` tolerates a member's `KeyNotFound` — treating "the prefix is not there" as
"the member holds nothing" — or `AsyncFileStore::listdir_keys` answers `Ok(vec![])` for a prefix
directory that does not exist, which is arguably the better fix since the same state breaks
`listdir` directly.

Deciding between them needs a view on whether an absent prefix directory is absence (§4: `Ok`) or a
failure. The contract says absence is not an error, which points at the second.

## Discovery

Found on 2026-09-02 by `C3` of the conformance suite, at Phase 4 step 9 of
`design/store-conformance-suite/` — the first time a router was asked the same questions as a plain
store.

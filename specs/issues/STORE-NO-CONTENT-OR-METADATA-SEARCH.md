---
id: STORE-NO-CONTENT-OR-METADATA-SEARCH
kind: feature
title: A store cannot be asked which keys match content or metadata
status: draft
priority: P2
complexity: L
area: [core/store]
design: 
created: 2026-09-15
github:
---
## Problem

`AsyncStore` can enumerate (`keys`, `listdir`, `listdir_keys`, `listdir_keys_deep`) and it can
fetch (`get`, `get_metadata`, `get_asset_info`). It cannot select. There is no method that answers
"which keys contain this text" or "which keys have this metadata field", so every consumer that
needs a subset must list the whole subtree, fetch each entry and filter in its own code.

That is O(corpus) reads per query, at the caller, with no way for a backend to do better even when
it could — an OpenDAL backend over a service with server-side filtering, a future indexed store, or
a store that already keeps a `DirectoryIndex` in memory all have to pretend they cannot.

## Impact

Tolerable at a few hundred keys, which is why this is P2 and not higher. It stops being tolerable
as soon as a store is large or remote: the read amplification is paid over the network, and the
30-second timeout on the `/q` handler becomes reachable.

It also pushes retrieval logic up into every consumer. `scripts/docs_index.py` implements one
version of it for `specs/`; `AGENT-MEMORY-SERVICE` would implement a second inside a command; a
third would appear the next time something needs to find keys by content.

## Expected behaviour

A selection capability on the store trait, with a default implementation over
`listdir_keys_deep` + `get` so no existing store breaks, and an override for backends that can do
better. Shape to be designed; the questions that need answering first:

- What the predicate language is. Substring and a metadata field/value equality cover the known
  cases; a regex or a small expression grammar covers more and is much harder to push down to a
  backend.
- Whether results are keys, `AssetInfo`, or keys with match positions.
- Whether it is paginated, and what ordering it promises (probably none).
- How it interacts with `StoreCapabilities` and the conformance suite — a store that cannot select
  must say so, and the suite must check that the default implementation and an overridden one
  agree.
- Whether an inverted index belongs in `liquers-core` as a derived asset (the index becomes an
  asset depending on every document, which is elegant and puts several hundred dependencies on one
  asset — worth measuring before committing).

## Discovery

Analysis for `AGENT-MEMORY-SERVICE`, 2026-09-15. Verified at HEAD against the `AsyncStore` trait
in `liquers-core/src/store.rs`: no selection method exists, and `STORE_SEMANTICS.md` specifies
none.

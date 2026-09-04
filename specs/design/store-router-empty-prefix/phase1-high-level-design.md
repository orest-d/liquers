# Phase 1: High-Level Design - Empty File-Store Directories

## Feature Name

Empty File-Store Directory Enumeration

## Purpose

An absent directory in a file-backed store represents an empty namespace, not a failed listing.
This lets an `AsyncStoreRouter` enumerate a newly configured file-store member without hiding
the keys from its other members.

## Core Interactions

- **Query, commands, assets, value types, web/UI:** none; no query syntax, command, asset
  lifecycle, value type, endpoint, or UI behavior changes.
- **Store:** `AsyncFileStore` and synchronous `FileStore` return an empty listing for an absent,
  addressable directory. `AsyncStoreRouter::keys()` then inherits the behavior through its
  existing recursive listing; no router dispatch changes.

## Crate Placement

`liquers-core/src/store.rs` owns both file-store implementations and `AsyncStoreRouter`; no
dependency or public trait signature changes are needed.

## Documentation Intent

**Reference:** extend `specs/reference/STORE_SEMANTICS.md` §4 to state `listdir` on an absent,
addressable directory returns `Ok([])` while backend failures remain errors.

**Guide:** extend `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` to require that not-found is
mapped specifically, not by swallowing all listing errors.

**Other documents:** none. **Updates:** close the source issue after proof; Phase 5 reviews the
two documents above. Audience: store implementers and maintainers need not read this design.

## Open Questions

None. The scope includes both file-store variants because the shared store contract cannot leave
their identical `listdir` behavior divergent.

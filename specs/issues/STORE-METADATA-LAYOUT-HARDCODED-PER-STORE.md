---
id: STORE-METADATA-LAYOUT-HARDCODED-PER-STORE
kind: issue
title: Every writable store hard-codes its own metadata layout
status: draft
priority: P2
complexity: L
area: [core/store, store/backends, web, docs]
design:
created: 2026-09-03
github:
---
## Problem

Every writable store answers the question *where does metadata live?* on its own, in code, with no
shared abstraction and no configuration:

| Store | Layout |
|---|---|
| `AsyncFileStore`, `FileStore` (`liquers-core/src/store.rs`) | sidecar `key.__metadata__`, JSON |
| `AsyncOpenDALStore` (`liquers-store/src/opendal_store.rs`) | sidecar `key.__metadata__`, JSON, via `PathMap` |
| `LocalStorageStore` (`liquers-web/src/store/local_storage.rs`) | a separate namespace, `{ns}/{kind}/{key}` |
| `AsyncMemoryStore` (`liquers-core/src/store.rs`) | a field in the value tuple |
| `JsStore` (`liquers-web/src/store/js_store.rs`) | delegated to JavaScript |
| Earlier Liquers versions | a `__metadata__` **folder**: `parent/__metadata__/filename.json` |

The layout decides several things that a store should not be deciding alone, and that no store
currently makes configurable:

- **Where metadata is written** — beside the data, in a sibling folder, in a separate namespace, in
  a database table, or nowhere.
- **Whether metadata is stored at all**, or derived on read from the data and the key. A read-only
  or derived-only store is legitimate; nothing expresses it.
- **Whether derived metadata is cached**, and where.
- **Which keys the layout makes unrepresentable** — and therefore what `is_supported` and the path
  builders must refuse. This is the coupling that makes the layout a cross-cutting concern rather
  than a private detail: today each store hard-codes its own reserved names, and one that gets it
  wrong corrupts data (`CORE-FILE-STORE-WRITES-METADATA-COLLIDING-KEYS`).

A new file-like backend has to rediscover all of this and re-derive the reserved-name rule, which
`STORE_SEMANTICS.md` §8 states but no code shares.

## Impact

Nothing is broken today; this is a design limitation with three consequences.

- **Every new writable backend re-implements the same decision**, including the refusal rule it is
  easiest to get wrong — the failure mode is silent metadata corruption, which is exactly what
  `CORE-FILE-STORE-WRITES-METADATA-COLLIDING-KEYS` was.
- **The reserved-name set is frozen in code.** `SIDECAR-COLLIDING-KEYS` reserves both the suffix
  form (`x.__metadata__`) and the bare folder name (`__metadata__`) in every segment, the second
  purely to keep the legacy folder layout reachable in future. With a pluggable layout that would
  be a property of the *configured* layout instead of a constant, and `is_supported` would consult
  the layout rather than a hard-coded list. **Whatever fixes this issue must revisit
  `is_supported` and the path builders in both file stores and in `PathMap`.**
- **Layouts that are not sidecars cannot be expressed at all** — metadata in a database column
  beside the data, or derived on the fly with a cache.

Workaround: implement `AsyncStore` from scratch per backend, which is what everyone does.

## Expected behaviour

A single place — a `MetadataLayout` trait or a `MetadataStoreMixin`, name to be decided — that a
store composes with, answering at minimum:

- the internal path or address for a key's data and for its metadata;
- the reserved names that layout makes unrepresentable, so `is_supported` and every path builder
  can consult one predicate instead of each carrying a copy;
- whether metadata is stored, derived, or derived-and-cached.

Configurable through `store_config.rs` / `store_factory.rs` like the rest of a store's
construction, so a deployment can pick the sidecar layout, the folder layout or a native one
without a new store type. The conformance suite already has the vocabulary to check a layout's
refusals (`sidecar01`, `sidecar03`, `prefix03`, `sibling05`).

`L` because it crosses `liquers-core`, `liquers-store` and `liquers-web` and adds trait API; a
design folder is owed before work starts.

## Discovery

Raised on 2026-09-03 while designing `SIDECAR-COLLIDING-KEYS`, the fix for
`CORE-FILE-STORE-WRITES-METADATA-COLLIDING-KEYS`. Deciding *which* keys the file stores must refuse
turned on which metadata layouts have to stay reachable — the legacy `__metadata__` folder among
them — and that question has no owner in the code. Recorded then rather than answered, because the
fix needs one reserved-name rule and this needs a subsystem.

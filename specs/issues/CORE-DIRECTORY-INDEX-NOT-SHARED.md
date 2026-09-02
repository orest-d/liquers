---
id: CORE-DIRECTORY-INDEX-NOT-SHARED
kind: issue
title: Directory knowledge for backends without directories is reimplemented in every store
status: accepted
priority: P1
complexity: L
area: [core/store, store/backends, web]
design: opendal-path-mapping
created: 2026-09-02
github:
---
## Problem

Most storage backends have no directory objects. A key set is flat, and `is_dir`, `contains`,
`listdir` and directory metadata have to be *derived* from the keys that exist. Every store that
faces this has solved it privately, and no two solutions are the same:

| Store | Crate | Mechanism | Shape |
|---|---|---|---|
| `AsyncMemoryStore` | `liquers-core` | `dir_index: scc::HashMap<Key, Arc<scc::HashMap<Key, usize>>>`, refcounted, maintained by `set`/`remove` | concurrent, mutable |
| `MemoryStore` (sync) | `liquers-core` | no index — `is_dir` scans every key with `has_key_prefix` on each call | O(n) per call |
| `FetchStore` | `liquers-web` | `directory_index()` builds `BTreeMap<Key, BTreeSet<String>>` once from a configured key set | immutable |
| `LocalStorageStore` | `liquers-web` | `index_key()` maintains `dirs: BTreeMap<Key, BTreeSet<String>>` **plus** `explicit_dirs: BTreeSet<Key>` for empty directories created by `makedir` | mutable, single-threaded |
| `AsyncOpenDALStore` | `liquers-store` | **none** — `is_dir` asks the backend to `stat` a path that does not exist | broken (`STORE-OPENDAL-SLASH-HANDLING` defect 4) |

Four implementations of one idea, a fifth store that needs it and has nothing, and no shared
definition of what the answer should be. The semantics diverge accordingly: `is_dir` on an absent
key is `Ok(false)` in three stores and `Err` in the OpenDAL one; `contains` falls back to `is_dir`
in three and not in the others; only `LocalStorageStore` can represent an empty directory that
genuinely exists.

## Impact

Every new `AsyncStore` implementation is a fresh opportunity to re-invent this and get it subtly
different, and the divergences are invisible until something built on the trait behaves differently
per backend. `AsyncStoreRouter` mixes stores in one namespace, so a router with a memory store and
an OpenDAL store answers `is_dir` two different ways depending on which store a key lands in.

The concrete case that forced this: `AsyncOpenDALStore` has no mechanism at all, so on the memory
backend and on object stores generally — `s3`, `gcs`, `azblob`, the SQL backends, which is most of
`OPENDAL_STORE_TYPES` — `listdir` sees a directory that `is_dir`, `contains`, `get_metadata` and
`get_asset_info` all deny. The same gap is expected in `liquers-web`'s HTTP-backed stores as they
grow beyond a configured key set.

## Expected behaviour

A shared mechanism in `liquers-core`, so a store supplies only its own *source* of directory truth
and inherits the semantics:

1. A reusable directory index — the `AsyncMemoryStore` mechanism generalized, able to be built from
   a key set (as `FetchStore` does), maintained incrementally (as `AsyncMemoryStore` does), and to
   hold explicitly created empty directories (as `LocalStorageStore` does).
2. Shared `AsyncStore` semantics for the questions all backends answer the same way once `is_dir`
   is known: `contains` falls back to `is_dir`; `is_dir` on an absent key is `Ok(false)`, never an
   error; a directory key's metadata is `default_metadata(key, true)` without a recursive subtree
   walk.

Stores with a real directory concept (`AsyncFileStore`) keep asking the filesystem; stores with a
listing but no directory objects (`AsyncOpenDALStore`) answer from a bounded listing; stores with
neither (`FetchStore`, `LocalStorageStore`, `AsyncMemoryStore`) answer from the index. All three
sources feed the same semantics.

The contract itself is written down in `specs/reference/` — see
`STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE`, which is the suite that would enforce it
across implementations.

## Discovery

Raised on 2026-09-02 during the architecture gate of
[`design/opendal-path-mapping/`](../design/opendal-path-mapping/): the OpenDAL directory gap was
about to be fixed with a mechanism private to that store, and the same problem exists in
`liquers-web`'s HTTP store. That design now covers this issue as well.

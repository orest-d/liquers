---
title: Store Behavioural Semantics
kind: reference
audience: internal
area: [core/store, store/backends, web]
reviewed: 2026-09-30
---
# Store Behavioural Semantics

What an [`AsyncStore`](../../liquers-core/src/store.rs) implementation must do, as distinct from how
it is configured (`STORE_CONFIG_FSD.md`) or how a store type is registered
(`guides/STORE_FACTORY_GUIDE.md`).

## Why this document exists

`AsyncStore` has **nine** in-tree implementations — `AsyncMemoryStore`, `AsyncFileStore`,
`AsyncStoreRouter` and `NoAsyncStore` in `liquers-core`, `AsyncOpenDALStore` in `liquers-store`
(over two OpenDAL services that behave differently), and `FetchStore`, `LocalStorageStore` and
`JsStore` in `liquers-web` — plus the trait defaults themselves, which a store inherits by writing
nothing, and whatever a language integration supplies. The count said five until 2026-09-02, when
the conformance suite had to enumerate them.
Until 2026-09-02 the trait's doc comments were the whole specification, and the implementations did
not agree: eleven separate disagreements were enumerated, of which one destroyed data.

`AsyncStoreRouter` mixes implementations in a single namespace, so a deployment answers the same
question two ways depending on which store a key lands in. This document is the contract; the
shared suite that holds every implementation to it is `liquers_core::store_conformance`, and the
*Enforced by* line under each section names the rules that check it. [`guides/STORE_IMPLEMENTATION_GUIDE.md`](../guides/STORE_IMPLEMENTATION_GUIDE.md)
is the operational counterpart — how to implement a store that satisfies this, and how to run the
suite against it.

**Rows marked ⚠ are known to be unsettled.** They name the issue tracking them rather than stating a
rule the code does not follow. One remains: §7's, which is a *parsing* limit rather than a store one.
§2's `children` question was settled on 2026-09-29.

**This document is trait-neutral where the rule is.** `AsyncStore` is the only store trait that
must satisfy it today — the synchronous `Store` is obsolete and unreachable
(`CORE-SYNC-STORE-TRAIT-OBSOLETE`) — but the rules are stated about *a store*, not about one trait,
so that a synchronous store reintroduced for a realm with synchronous evaluation inherits the
contract instead of re-deriving it.

## 1. The sibling rule

> **No operation on a key may read, list, or delete anything under a different key.**

In particular, a key whose name is a *prefix* of another key's name is a different key: `data` and
`database` are unrelated, as are `sub` and `subway`.

This is the rule most easily broken by a store whose backend addresses by string prefix rather than
by path. `AsyncOpenDALStore` broke it in three places, because OpenDAL's `list`, `remove_all` and
`create_dir` treat a path without a trailing `/` as a prefix. `removedir("data")` deleted
`database/`, reachable through `DELETE /api/store/removedir/{*key}`.

A store that maps keys onto strings must therefore have **one** place that produces its directory
form, and every call site that names a directory must use it. Spreading the rule across call sites
is what allowed two of the three to be missed for as long as they were.

The directory form is subject to the same key refusals as the data and metadata forms (§8): a key
the store will not address as data must not become addressable as a directory.

*Enforced by:* `sibling01`, `sibling02`, `sibling03`, `sibling04`, `sibling05`.

## 2. Directories on a backend that has none

Most backends are flat. A key set has no directories in it, and `is_dir`, `contains` and `listdir`
must be *derived* — every proper prefix of a stored key is a directory.

**Three sources of directory truth.** A store uses whichever its backend offers:

| Backend shape | Source | Implementations |
|---|---|---|
| Real directories | `stat` the path | `AsyncFileStore` |
| A listing, but no directory objects | a bounded listing of the directory path | `AsyncOpenDALStore` |
| Neither | [`DirectoryIndex`](../../liquers-core/src/store_dir_index.rs) | `AsyncMemoryStore`, `FetchStore`, `LocalStorageStore` |

**A store whose backend is authoritative must not keep an index.** An object store can be written by
another process, another tool, or a second Liquers instance against the same bucket, so a write-side
index goes stale and rebuilding it means listing everything. Such a store asks the backend and pays
one bounded listing on the branch that would otherwise have failed.

**What every store shares regardless of source:**

- `listdir` and `is_dir` must agree. A directory the listing can see must be addressable.
- `contains` falls back to `is_dir`. Provided by the `AsyncStore` default; a store overriding
  `is_dir` and not `contains` would otherwise have the two disagree silently.
- A directory key's metadata is `default_metadata(key, true)`, and **`default_metadata` must honour
  both arguments** — a record with `is_dir == false` and no key is a file-shaped answer for a
  directory, which is what a caller reading the record directly receives. `get_asset_info` is built
  on `get_metadata`, so a store that cannot produce directory metadata cannot answer `-R-dir/`
  queries.
- **Directory metadata populates `children`, one level deep.** A directory's record carries one
  `AssetInfo` per direct child — the same names `listdir` gives — and a child directory is described
  by a directory-shaped `AssetInfo` without its own children. The `AsyncStore` default
  `get_asset_info` guarantees the depth bound: for a directory it answers from
  `default_metadata(key, true)` instead of reading the directory's metadata, so filling a parent's
  `children` never recurses. The cost that remains is **width**: a directory of *n* entries costs
  one listing plus one `get_asset_info` per entry, which for a data child is one metadata read. The
  price of the bound is that a directory's own stored fields (a title in a sidecar, say) are not in
  its `AssetInfo`; nothing in tree writes directory metadata today.
  Settled on 2026-09-29 in favour of what every implementation already did — the text used to
  forbid `children` — and `AsyncOpenDALStore`, the one store that left it empty, now fills it
  (`design/store-conformance-backlog/`).

*Enforced by:* `dir01`, `dir02`, `dir03`, `dir04`, `dir05`, `dir06`, `dir07`, `dir08`, `data01`,
`data03`, and the refuting rules `nowrite01` and `nodir01`.
`data03` and `dir08` work from keys that already exist, which is the only way a **read-only** store
can be checked at all — until they were added, a store whose read path was entirely broken reported
conformant. `nowrite01` and `nodir01` check that a store declaring it *cannot* do these things
really refuses, so a declaration is a claim rather than a way to skip the rules for it.
The `DirectoryIndex` component keeps its own `diridx01`-`diridx08` unit tests in `liquers-core`.

## 3. Derived and explicit directories are different

A directory **derived** from its children retires when the last child is removed. A directory
**created** by `makedir` has no children and persists until `removedir`.

A derived index alone cannot express the second. `LocalStorageStore` grew a private
`explicit_dirs` set for exactly this; `AsyncMemoryStore`, lacking one, had a `makedir` that
recorded nothing and reported success. `DirectoryIndex` carries both, and `is_dir` answers for
either.

**A recursive `removedir` removes explicit directories beneath it, not only the one named.**
Forgetting a single marker leaves a `makedir` descendant reporting as a directory after the
removal that was supposed to contain it succeeded.

*Enforced by:* `explicit01`, `explicit02`, `explicit03`, `nomakedir01`.

## 4. Absence is not an error

| Call | On a key that is simply absent | On a backend failure |
|---|---|---|
| `is_dir` | `Ok(false)` | `Err` |
| `contains` | `Ok(false)` | `Err` |
| `get`, `get_bytes`, `get_metadata` | `Err(KeyNotFound)` | `Err` |
| `listdir` | `Ok([])` | `Err` |
| `removedir` | `Ok(())` — a no-op | `Err` |

The distinction between "not there" and "could not tell" is load-bearing: a store that reports an S3
403 as `Ok(false)` from `is_dir` tells a caller a directory does not exist when the truth is that
permission was refused. Match the backend's not-found condition specifically rather than testing
whether a result is an error.

`listdir` follows the same distinction: an absent **addressable** directory has no children, so it
returns `Ok([])`. This is what lets a router enumerate a newly configured member whose prefix has
not been created on a filesystem yet. A failed existence check, metadata read, or directory read is
still a backend error, not an empty listing. Key-shape and store-specific refusals are checked
before absence and remain errors.

A store that **delegates** to code in another language must give that code a way to answer
"absent" as distinct from "failed", or it cannot satisfy this table: `JsStore` treats `null` or
`undefined` from a page's `get`/`getMetadata` as `KeyNotFound` and a thrown value as `KeyReadError`.

*Enforced by:* `absence01`, `absence02`, `absence03`, `dir02`.

## 5. Removal

> **`removedir` is specified by its postcondition: if it returns `Ok(())`, the directory does not
> exist afterwards.** Failing to remove it is an error. What is forbidden is claiming success
> without the effect.

Two rules follow from that rather than being stipulated beside it:

- **`removedir` is recursive.** A directory derived from its children exists while any child
  remains (§2), so a removal that left one and reported `Ok(())` would break the postcondition.
  Every implementation has always been recursive; the trait's doc comment claimed otherwise until
  2026-09-02 and was simply wrong.
- **On a directory that does not exist, `Ok(())` is correct.** The postcondition already holds, so
  there is nothing to claim. A store that *cannot* remove directories at all is a different case —
  it declares no `RemoveDirectories` capability and answers `Err(KeyNotSupported)`, which is a
  refusal rather than a false claim of success.

`removedir` is **scoped to the directory** (§1), never to the key's string prefix.

`removedir` is **not atomic** on any backend. `AsyncFileStore` uses `remove_dir_all`,
`AsyncOpenDALStore` deletes entry by entry, `AsyncMemoryStore` iterates its map: a crash part-way
through leaves a partially removed directory. No store offers a transaction, and callers must not
assume one.

`removedir` on the root key empties the store. That is what removing the root directory means.

**The trait default returns `Err(KeyNotSupported)`, and that is correct** under the postcondition
framing: a store that has not implemented `removedir` is refusing, not succeeding silently. It was
recorded as a divergence while the rule was stated as a return convention; restating it as a
postcondition resolves it without changing any code.

*Enforced by:* `remove01`, `remove02`, `remove03`, `absence03`, `sibling01`, `data02`,
`noremove01`, `noremovedir01`.

## 6. Keys, prefixes and routing

A store is constructed with a `prefix: Key`, and:

- **`key_prefix()` reports the configured prefix.** `AsyncStoreRouter::is_dir` and `listdir` select
  on `key_prefix()` **alone** — unlike `find_store`, which also consults `is_supported` — so a store
  that under-reports its prefix answers for keys belonging to stores listed after it.
- **The prefix is part of the path under the backend root**, not a mount point that is stripped.
  `FileStore::key_to_path` pushes the whole key, prefix included, onto its root, and
  `AsyncOpenDALStore` does the same. `liquers-web`'s `FetchStore` is the one store that strips its
  prefix, and documents that it is the exception.
- **`is_supported` answers whether the store supports the key.** The answer is cumulative: the key
  must be absolute, must begin with the configured prefix, and must pass any narrower
  store-specific exclusions such as a reserved folder, unsupported file type, ambiguous metadata
  sidecar, or explicit allowlist.
- **The router repeats the prefix check deliberately.** Router selection must remain safe for
  custom stores, while a store's direct `is_supported` answer must truthfully describe its own
  supported namespace.

An empty prefix does not mean a store must support every absolute key. For example, a single-file
overlay can have an empty prefix and return true only for its intercepted file. When placed before
a general store, it handles that file while unsupported keys pass to subsequent stores.

`AsyncMemoryStore` and `MemoryStore` have no narrower exclusions, so their predicate is exactly
`!key.is_relative() && key.has_key_prefix(&self.prefix)`.

*Enforced by:* `prefix01`, `prefix02`, `prefix03`, `prefix04`, `sibling05`.
`prefix04` is the positive case and the one most easily left out: `prefix02` and `prefix03` both
assert `is_supported` is *false*, and the trait default returns `false` unconditionally, so without
it a store that refuses every key passes both and looks conformant while being unusable in a
layering. `AsyncMemoryStore` keeps its own `memsupport01`-`memsupport06` unit tests, and the router
its `router01`.

## 7. Key shape

A key given to a store is **absolute**: no element may be `.` or `..`. A relative key reaching a
store is refused with `ErrorType::KeyNotAbsolute`, by every method and by the path builders
directly. Relative keys are resolved at plan level; a store never resolves them.

The rule is enforced per method by convention rather than by the type, which is
`STORE-ABSOLUTE-KEY-NOT-TYPE-ENFORCED`.

⚠ Non-ASCII resource names cannot be parsed into a `Key` at all (`RESOURCE-NAME-ASCII-ONLY`), so
they never reach a store. This is a parsing limit, not a store one.

*Enforced by:* `keyshape01` (the reading methods) and `keyshape02` (the mutating ones).
They are separate rules because checking that `set`, `remove` and `removedir` refuse means *calling*
them with a traversal key — and on the nonconforming store the rule exists to find, such a key may
resolve outside the store's namespace. Only the reads are safe below `Scratch`.
The `keyabs` family in `liquers-core/src/query.rs` covers `Key`'s own
relativeness predicate, which is a different subject.

## 8. Metadata sidecars and reserved names

A store that keeps metadata beside its data uses the suffix `.__metadata__`: the metadata for `foo`
lives at `foo.__metadata__`.

A layout that does this makes some names unusable as keys, and **a store must refuse a key it
cannot address unambiguously** rather than let it collide. The reserved set is declared by the
store's own layout, and there are two ways a name is reserved:

| Form | Example | Reserved by |
|---|---|---|
| a suffix | `foo.__metadata__` — its *data* path is byte-identical to the *metadata* path of `foo` | every sidecar store |
| a suffix | `foo.__lock__` — its data path is the lock `AsyncFileStore` takes while writing `foo`, so a file there blocks every later write to `foo` | `AsyncFileStore` only |
| an exact name | `__metadata__` — the metadata *folder* of the predecessor Python implementation (`orest-d/liquer`), where the metadata for `sub/foo.txt` is `sub/__metadata__/foo.txt.json` | every sidecar store, so that layout stays readable |

**A key is refused when *any* segment is reserved, not only its filename.** `dir.__metadata__/child`
needs `dir.__metadata__` to be a directory while the metadata of `dir` needs it to be a file, and a
filesystem will not be both.

**Each store reserves what its own layout uses, and no more.** Over-reserving is a defect in the
same family as under-reserving: `x.__lock__` is a key `AsyncOpenDALStore` can address perfectly
well, because it takes no locks.

Three kinds of caller must consult the rule, and a store that satisfies only the first is the
defect this section exists to prevent:

1. **`is_supported`** — but this is only a *routing hint*. `AsyncStoreRouter` consults it; a caller
   holding the store does not have to, and `liquers-axum`'s store handlers do not.
2. **The path builders**, so that every fallible method inherits the refusal.
3. **The listing filters.** Not optional: a reserved name left in a listing is handed to `is_dir`
   by `listdir_keys_deep`, which now refuses it — turning a refusal into a failed enumeration and
   making the store unlistable. Listings **skip** what the store cannot address.

The refusal is `KeyNotSupported`, and **`as_absolute` is checked first**: a key that is both
relative and reserved reports `KeyNotAbsolute`, because a relative key is not a store address at
all (§7). Every store answers this the same way.

A sidecar found in the backend implies its data key: a listing reports `sub/orphan.__metadata__` as
`sub/orphan`, once even when the data object is there too. A path a store cannot decode is
**skipped** by listings rather than failing them — one unexpected object in a shared bucket must not
make a directory unlistable — and so is a sidecar whose implied name is reserved or empty.

**So a key with metadata and no data is enumerable.** A store that accepts `set_metadata` for such a
key must answer it from `contains` and list it in its parent's `listdir`. A store may instead refuse
the write with `KeyNotFound`, which is consistent; accepting it and then hiding the key is not. Two
consequences follow for callers. An asset whose metadata was recorded before its data — a failed or
unfinished evaluation, say — appears in listings as a key `get` cannot read. And a metadata-only key
whose sidecar cannot be parsed now reaches `listdir_asset_info`, where it fails the parent
directory's `get_metadata`: `get` repairs unparseable metadata only when there is data to repair it
from. Before 2026-09-29 the file stores dropped such keys from listings instead.

One behaviour worth knowing, because recovery from a store corrupted before this rule was enforced
depends on it: **`get` repairs metadata it cannot parse**, synthesizing a fresh record with warnings
and writing it back. So a colliding write that already happened is recoverable — by `get`, by
`set_metadata`, or by `remove`, which unlinks the data path and the metadata path together — even
though the orphan can no longer be addressed as a key.

*Enforced by:* `sidecar01`, `sidecar02`, `sidecar03`, `sidecar04`, and `prefix03` and `sibling05` for stores whose
fixture declares an unsupported shape. `is_supported` is a routing hint, so `sidecar01` checking it
alone would pass a store that refuses to route the key and then accepts it in `set`, overwriting the
very metadata the refusal exists to protect; `sidecar03` checks the operations themselves. The file
stores keep `reserved01`-`reserved08` and the OpenDAL path mapping its own `pathmap02`-`pathmap08`
unit tests.

## 9. What `keys()` returns

> **`keys()` returns data keys, the directories above them, and the store's own prefix. Every key
> it returns starts with that prefix.**

The second sentence is the testable half and the one a router depends on: a store that enumerates
keys it does not own makes a composed namespace unreadable, because the caller cannot tell which
store an answer came from.

**A key returned by `keys()` is therefore not necessarily one that `get` will succeed on** — a
directory is enumerated and cannot be read as data. Use `is_dir` (§2) to tell them apart. This is
the cost of the decision, and it is deliberate: the alternative contract, data keys only, makes
`keys()` unable to describe the shape of the store at all.

`AsyncMemoryStore` returned data keys only, which is the divergence
`CORE-STORE-KEYS-MEANS-TWO-DIFFERENT-THINGS` recorded; the `AsyncStore` default, `AsyncFileStore`
and `AsyncOpenDALStore` already behave as specified here.

*Enforced by:* `keys01`, `keys02`, `nokeys01`.

## History

| Date | Change | Source |
|---|---|---|
| 2026-09-30 | §4: a delegating store must let the delegate express absence, with `JsStore`'s `null` sentinel as the example. Reviewed §2, §8 and §9 against the implementation and the final conformance reports: every in-tree store passes `dir07` and `sidecar04`. | phase-5 (`design/store-conformance-backlog/`) |
| 2026-09-29 | §8: a key with metadata and no data is enumerable — listed by its parent and answered by `contains` — unless the store refuses the write with `KeyNotFound`. The file stores now list the implied key of a sidecar instead of dropping it. Recorded the two consequences for callers. Enforced by the new rule `sidecar04`. | `design/store-conformance-backlog/` step 6 |
| 2026-09-29 | §2 settled: directory metadata populates `children` with the direct children, one level deep, and the `AsyncStore` default `get_asset_info` answers a directory without reading its metadata, which is what bounds the depth. `dir07` now checks this instead of reporting `Blocked`; `AsyncOpenDALStore` fills `children` like every other store. One ⚠ row remains. | `design/store-conformance-backlog/` step 4 |
| 2026-09-04 | §4 now defines `listdir` on an absent addressable directory as `Ok([])`, while retaining errors for failed filesystem operations and invalid or unsupported keys. This lets a router enumerate an uncreated file-store prefix without treating it as a failed backend. | phase-5 |
| 2026-09-03 | §8 restated as **reserved names** rather than one sidecar suffix: reserved in *any* segment rather than only the filename, declared per store by its own layout, and covering both the suffix form and the exact name — the latter being the predecessor Python implementation's `__metadata__` folder, cited because nothing in this repository evidences it. Named the three kinds of caller that must consult the rule, and why satisfying only `is_supported` is the defect the section exists to prevent. Recorded that listings *skip* reserved names, that the refusal is `KeyNotSupported` with `as_absolute` checked first, and that `get` repairs unparseable metadata — which is what makes an already-corrupted store recoverable. | `design/sidecar-colliding-keys/` Phase 5 |
| 2026-09-02 | Completed the contract. §5 restated as a **postcondition** — `Ok(())` means the directory is gone — from which recursion and the absent-directory case follow, and which makes the trait default's `Err(KeyNotSupported)` correct rather than divergent. §9 settled: `keys()` returns data keys, directories and the prefix, and **every returned key starts with the prefix**; the cost, that an enumerated key is not necessarily readable, is stated rather than hidden. Every *Enforced by* line now names rules in `liquers_core::store_conformance`. Stated trait-neutrally against the possible return of a synchronous store. Two of the three ⚠ rows are gone; §6's was cleared by `async-memory-store-prefix-support`. | `design/store-conformance-suite/` Phase 4 step 1 |
| 2026-09-02 | Recorded that a recursive `removedir` takes explicit descendant directories with it, that `default_metadata` must honour both arguments, and that the directory path form is subject to the same key refusals as the data and metadata forms. All three from PR #58 review findings. | `design/opendal-path-mapping/` PR review |
| 2026-09-02 | Defined `is_supported` cumulatively: absolute key, configured-prefix membership, then optional store-specific exclusions. Added the empty-prefix single-file overlay rationale and memory-store conformance tests. | `design/async-memory-store-prefix-support/` Phase 5 |
| 2026-09-02 | Created. Written against the implementation after `STORE-OPENDAL-SLASH-HANDLING` and `CORE-DIRECTORY-INDEX-NOT-SHARED` were fixed: the sibling rule, the three sources of directory truth, derived versus explicit directories, absence versus failure, removal, prefixes and routing, key shape, metadata sidecars. Three questions are recorded as unsettled rather than answered. | `design/opendal-path-mapping/` Phase 5 |

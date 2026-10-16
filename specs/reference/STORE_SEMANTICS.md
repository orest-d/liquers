---
title: Store Behavioural Semantics
kind: reference
audience: internal
area: [core/store, store/backends, web]
reviewed: 2026-10-16
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

Since 2026-10-15 a mount can also be **adapted by configuration** (`read_only`, `metadata`):
`ReadOnlyStore` and `MetadataPipelineStore` (`liquers-core/src/store/`) wrap any store that offers
the data-only methods of §11, and must satisfy this contract like any other implementation;
conformance suites `A1`–`A7` run it against them. §11–§16 state what the adapters add.

`AsyncStoreRouter` mixes implementations in a single namespace, so a deployment answers the same
question two ways depending on which store a key lands in. This document is the contract; the
shared suite that holds every implementation to it is `liquers_core::store_conformance`, and the
*Enforced by* line under each section names the rules that check it. [`guides/STORE_IMPLEMENTATION_GUIDE.md`](../guides/STORE_IMPLEMENTATION_GUIDE.md)
is the operational counterpart — how to implement a store that satisfies this, and how to run the
suite against it.

**Liquers assumes it is the only writer of a store's metadata while it runs.** A store's *data*
may be changed by other programs: a configured mount notices through the backend's facts (size,
modification time, ETag; §13) and the handlers that compare them. Its *metadata* may not. A record
object (sidecar, folder record, grouped file, native record) edited by another program, or by hand,
while Liquers runs is at the editor's own risk: a record has no hash or time of its own, so the
change cannot be detected, and a cached copy (§13) or a later write may silently replace it. Two
Liquers processes writing one store are outside the contract for the same reason, since grouped
writes lock within one process only (§16). The safe way to change metadata is through Liquers
(`set_metadata`), or with Liquers stopped.

**Rows marked ⚠ are known to be unsettled.** They name the issue tracking them rather than stating a
rule the code does not follow. One remains: §7's, which is a *parsing* limit rather than a store one.
§2's `children` question was settled on 2026-09-29.

**This document is trait-neutral where the rule is.** `AsyncStore` is the only store trait; a
synchronous `Store` trait existed and was removed (`CORE-SYNC-STORE-TRAIT-OBSOLETE`) because no
`Environment` could hold one. The rules are still stated about *a store*, not about one trait, so
that a synchronous store reintroduced for a realm with synchronous evaluation inherits the contract
instead of re-deriving it. Such a store would need a synchronous evaluation path as well, not only
the trait back.

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

- **`key_prefix()` reports the configured prefix.** `AsyncStoreRouter::is_dir` and `listdir` ask
  every member whose `key_prefix()` covers the key — unlike `find_store`, which also consults
  `is_supported` and takes the first — so a store that under-reports its prefix is consulted about
  every directory in the router.
- **The prefix is part of the path under the backend root**, not a mount point that is stripped.
  `AsyncFileStore::key_to_path` pushes the whole key, prefix included, onto its root, and
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

**How the router answers.** Data has one owner, directories are a union:

- *Data* — `get`, `get_metadata`, `get_asset_info`, `set`, `remove` and the rest — goes to the
  **owner**, the first member whose prefix covers the key and whose `is_supported` is true. First
  match wins, so a member mounted inside another is added before it.
- *A directory* exists if the key is the root, lies above a member's prefix, is reported as a
  directory by **any** covering member, or is a member's own prefix at which the owner holds no data
  (an empty or not-yet-created member is still a directory; a member mounted at a single file is
  still a file). `contains` is the owner's `contains` or that rule, and is `Ok(false)` — not an
  error — for a key no member owns (§4).
- *Whole-directory questions* — `listdir_asset_info`, and `children` in directory metadata — are
  forwarded to a member when it is the only contributor (the only member covering the key or mounted
  beneath it, and the owner), so its own answer is used. Otherwise the router composes them: one
  entry per name `listdir` gives, each described by its own owner's `get_asset_info`.
- A member's override of `get_asset_info` or `listdir_asset_info` is therefore heard through the
  router, as its `get_metadata` always was.

`AsyncMemoryStore` has no narrower exclusions, so its predicate is exactly
`!key.is_relative() && key.has_key_prefix(&self.prefix)`.

*Enforced by:* `prefix01`, `prefix02`, `prefix03`, `prefix04`, `sibling05`.
`prefix04` is the positive case and the one most easily left out: `prefix02` and `prefix03` both
assert `is_supported` is *false*, and the trait default returns `false` unconditionally, so without
it a store that refuses every key passes both and looks conformant while being unusable in a
layering. `AsyncMemoryStore` keeps its own `memsupport01`-`memsupport06` unit tests, and the router
its `router01` and `routerfwd01`-`routerfwd11`; conformance suite `C8` runs a router whose every
directory answer is composed.

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

**A key with metadata and no data has no data object.** `get` and `get_bytes` on it report
`KeyNotFound`, on every store; they do not return empty bytes. A key written with `set(k, b"", m)`
holds an *empty* data object, and `get_bytes` returns `[]`. The two must stay distinguishable: a
metadata-only `Text` entry read as empty bytes deserializes as `""`, a wrong value, and a re-hash
of stored bytes must skip the first and check the second. `AsyncMemoryStore` answered a
metadata-only key with empty bytes until 2026-10-07.

One behaviour worth knowing, because recovery from a store corrupted before this rule was enforced
depends on it: **`get` repairs metadata it cannot parse**, synthesizing a fresh record with warnings
and writing it back. So a colliding write that already happened is recoverable — by `get`, by
`set_metadata`, or by `remove`, which unlinks the data path and the metadata path together — even
though the orphan can no longer be addressed as a key.

*Enforced by:* `sidecar01`, `sidecar02`, `sidecar03`, `sidecar04`, `sidecar05`, and `prefix03` and `sibling05` for stores whose
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

## 10. The data format of a written key

> **Data written with no filename is stored with the key's last segment as its filename, and the
> data format follows its extension. A declared filename or data format is kept.**

So the same bytes written under `data/input.csv` with empty metadata report `csv` and `text/csv` on
every store. Setting the filename is level-2 seeding (`MetadataRecord::set_filename`): it fills the
data format only when none was declared, and it writes no media type, so an ordinary filename stays
distinguishable from a deliberate media-type override. A key whose last segment has no extension
gets a filename and no data format. Legacy metadata is an opaque document with no filename field
and is stored as written.

Every in-tree store seeds through one function, `liquers_core::store::seed_filename_from_key`,
which the default `finalize_metadata` calls and which the stores that persist the caller's record
as given (`AsyncMemoryStore`, `AsyncFileStore`, `JsStore`) call in `set`. `finalize_metadata` sets
the record's key without `MetadataRecord::with_key`, which would overwrite a declared filename with
the key's. A new store gets it by
calling `finalize_metadata` on the record it persists, or the function directly. The HTTP store's
`infer_metadata` already reported the same. Until 2026-10-11 only the stores whose persisted record
went through `finalize_metadata` (OpenDAL, `LocalStorageStore`) did this, so `AsyncMemoryStore`
reported `application/octet-stream` for the example above.

*Enforced by:* `seed01`, `seed02`.

## 11. Data-only access

`AsyncStore` has seven **data-only methods** — `get_data`, `set_data`, `remove_data`,
`rename_data`, `listdir_data`, `data_facts`, `get_stored_metadata` — and four questions about the
store: `data_capabilities`, `is_read_only`, `sync`, `version_verification`
(`liquers-core/src/store.rs`). Every one has a default, so a store that does not implement them is
unchanged: the data methods refuse with `NotSupported`
("Store '…' does not offer data-only access"), `data_capabilities` is all `false`,
`is_read_only` is `false`, `sync` reports nothing, `version_verification` is `None`.

A data-only method touches **the data object alone**: no sidecar is read, written or repaired, and
no record is synthesized. `listdir_data` returns raw names (sidecars included); `data_facts` returns
`Ok(None)` when no data object exists; `get_stored_metadata` returns the backend's own record exactly
as stored, `Ok(None)` when there is none. They are what a metadata layer above the store and the
read-only adapter build on; ordinary callers keep using `get` / `set`.

`DataCapabilities` says what a store offers, and is checked when a configured mount is built:

| Field | Means | `AsyncMemoryStore` | `AsyncFileStore` | `AsyncOpenDALStore` |
|---|---|---|---|---|
| `data_access` | the data-only methods are implemented | yes | yes | yes |
| `atomic_write` | `set_data` replaces an object atomically | yes | no | object services (`memory`, `s3`, `gcs`, `azblob`, `oss`, `cos`, `obs`) |
| `rename` | `rename_data` exists and is atomic | no | yes (`tokio::fs::rename`) | where the service has `rename` |
| `modified_time` | `data_facts` reports a time | yes | yes (nanoseconds) | no — reported when `stat` has one, never promised |
| `etag` | `data_facts` reports an ETag | yes (a counter) | no | no — reported when present, never promised |

`AsyncStoreRouter` forwards each data-only method to the owning member (or, for a key no member's
`is_supported` accepts, the first member whose prefix covers it — a file store refuses its sidecar
names in `is_supported` but not in `set_data`), refuses a `rename_data` across members, reports the
**intersection** of its members' capabilities, is read-only when it has members and all are, merges
its members' `sync` reports, and answers `version_verification` from the member that owns the key.
`liquers-web`'s `JsStore` reports `data_access` when the page object has `getData`, `setData`,
`removeData`, `listdirData` and `dataFacts`. `LocalStorageStore` implements every data-only method
(`atomic_write`, `rename` and `etag` — a hash of the stored entry — but no `modified_time`).
`FetchStore` implements the read half: `get_data`, `listdir_data` over its configured keys, and
`data_facts` from a HEAD request (size, and the server's ETag and `Last-Modified` when sent, never
promised); `set_data` and `remove_data` refuse. Its `get_stored_metadata` returns the record it
builds from the response headers (media type, `Source` status), the one exception to "never
synthesized" above: an HTTP server has no stored record to return, and without it a read-only
mount would lose the media type.

**Data-only reserved names are the backend's own.** In data-only mode a store reserves only what it
needs itself — `AsyncFileStore` reserves `.__lock__`, not `.__metadata__` — because a metadata
layer keeps records as ordinary data objects. The layer above enforces its own reserved names
(§13).

**Backend times** are `metadata::BackendTime`: RFC 3339, UTC, truncated to the backend's
`TimePrecision` (`Seconds`, `Millis`, `Micros`, `Nanos`), so a time written into a record and read
back compares exactly with a fresh `data_facts`. `BackendTime::parse` brings text from a language
bridge to the same form.

*Enforced by:* unit tests beside each implementation in `store.rs`, `opendal_data_only_access` in
`liquers-store/src/opendal_store.rs`, and the suites of §12–§15.

## 12. Read-only stores

A mount with `read_only: true` is wrapped in `ReadOnlyStore` (`store/read_only.rs`):

- `set`, `set_metadata`, `remove`, `removedir`, `makedir` and the data-only writes refuse with
  `NotSupported` ("Store 'read-only …' is read-only: … refused"); over HTTP that is 501.
- `is_read_only()` is `true`, and `sync` does nothing.
- **Nothing is written on read** — when the inner store has data-only access. Reads then go through
  `get_data` and `get_stored_metadata`, and a key with no stored record gets one synthesized in
  memory (the inner store's `default_metadata` plus `finalize_metadata(update = false)`, with the
  size from `data_facts`), never persisted.
- Over a store **without** data-only access every read is forwarded unchanged, so a store that
  writes on read still does; the builder prints a warning to stderr when it wraps such a store.
  Over a `MetadataPipelineStore` (which reports no data access) reads are forwarded on purpose: the
  pipeline below is built read-only (§13) and runs the configured chains without writing.

The router reports `is_read_only()` only when every member is read-only. A router-level
`read_only: true` wraps the whole store and needs `StoreRouterBuilder::build_store`.

*Enforced by:* `read_only_refuses_every_write_with_not_supported`, `read_only_file_store_creates_nothing`,
`read_only_reports_is_read_only` (`store/read_only.rs`), `read_only_mount_from_config`,
`read_only_over_store_without_data_access_warns` (`tests/store_adapters.rs`), `read_only_store_write_is_501`
(`liquers-axum/tests/store_api_routes.rs`), conformance suite `A1`.

## 13. Configured metadata: the pipeline, storage models and reserved names

A mount with a non-empty `metadata` section is wrapped in `MetadataPipelineStore`
(`store/pipeline.rs`), built from a backend with data-only access, a **storage model**
(`MetadataStorage`, `store/storage.rs`), four compiled **handler chains** (§14) and a **write
strategy** (§15). Building refuses a backend without `data_access` and a strategy the backend's
capabilities cannot carry, as a configuration error naming the mount. A mount with no section, or
`metadata: {}`, is not wrapped and behaves exactly as before.

**The pipeline is not a data backend**: it reports `DataCapabilities::default()` and refuses the
data-only methods, which belong to the store under it. Every other method it answers itself or
forwards to the backend (`is_dir`, `makedir`, `key_prefix`, `default_metadata`).

**Storage models.** Where a key's record lives:

| Model | Records at | Reserves |
|---|---|---|
| `NativeStorage` (`native`, the default) | the backend's own record, through `get_stored_metadata` / `set_metadata` — today's sidecars, byte for byte | nothing (the backend reserves its own) |
| `PerEntryStorage`, layout `sidecar` (default) | `docs/a.md.__metadata__` | suffix `.__metadata__` |
| … layout `root` | `__metadata__/docs/a.md` | exact `__metadata__` |
| … layout `prefix` | `docs/__metadata__a.md.json` | prefix `__metadata__` |
| … layout `folder` | `docs/__metadata__/a.md.json` | exact `__metadata__` |
| `GroupedStorage` (`grouped`, file `__metadata__.json`) | one JSON map name → record per folder | exact: the file name |
| `NoStorage` (`none`) | nothing; every record is synthesized by `on_default` | nothing |
| `MemoryCentralStorage` (`memory_central`, test-only) | a store of its own, by key — central storage on a router | nothing |

Records are JSON, encoded as the file store writes its sidecars; a stored document that does not
parse as a `MetadataRecord` is kept as `LegacyMetadata`, returned untouched, and bypasses every
chain. `central-<mount>` is declared but refuses to build ("declared but not implemented yet").

**Reserved names.** The pipeline refuses, with `KeyNotSupported`, every key in which **any segment**
is reserved by its storage model, its write strategy or a backup's storage model — the §8 rule,
written once in `ReservedNameSet::is_reserved_key` (the owned counterpart of `ReservedNames`, which
gained a prefix form). `is_supported` is false for them (and for whatever the backend's own
`is_supported` refuses), and `listdir` drops them.

**A key with a record and no data is enumerable** (§8): `contains` consults the storage model, and
`listdir` adds the names `MetadataStorage::record_names` reports. `removedir` first removes every
entry through the write strategy, so records kept outside the folder (`root` layout, central
storage) go with it.

**Reads.** `get`, `get_bytes`, `get_metadata` and `listdir_asset_info` first let the write strategy
recover the key (§15). A record is then read from the storage model (`get_folder` gives a grouped
model's whole folder in one read for a listing) or, when there is none, built by `on_default`;
`on_read` shapes it. The result is cached per key and reused while `data_facts` is unchanged, so an
unchanged entry in a listing costs no byte read. The cache is invalidated by this pipeline's own
writes; a record object edited outside the process while the data object is unchanged is not seen
until the data changes or the process restarts. A directory's record is `default_metadata(key,
true)` with its children (§2).

**`persist_defaults`** (default `false`): a record synthesized by `on_default` is stored the first
time it is read, through the write strategy; with `false` a read never writes. A pipeline built for
a read-only mount never persists, never recovers and never restores a backup, whatever the section
says.

**`created` and `etag`.** After every write that changes the data object, the pipeline reads
`data_facts` and stores the backend's modification time as `created` and its ETag as `etag`,
whatever the chain set — a second commit when they moved. So every write, a `Partial` checkpoint
included, moves `created`. A backend that reports no modification time (OpenDAL services whose
`stat` has none, a JavaScript store without `modified`) gets the time of the write as Liquers saw
it, at millisecond precision; it is informative only, since change detection compares `created`
only with a backend time and there is none (`created_falls_back_to_the_write_time`). A streaming write path would need the same hook
(`CORE-STORE-OPENBIN-MISSING`).

**The record cache** keeps each key's record while the key's *data* facts are unchanged, so a
record object edited outside Liquers is not re-read (see the assumption in the introduction).

**Central storage** (a router-level `metadata` section, `build_store` only): one pipeline over the
whole router, whose section governs handlers and the write strategy for every member. A member's own
section may name only `storage`, which becomes its **backup**: written in the same write set as the
central record, and read only when the central record is missing — then copied to central storage
with the warning "Metadata restored from the member's backup." A member with no section keeps no
backup; a member section naming anything but `storage` is a configuration error naming the member.

**Compatibility.** `MetadataRecord` gained `created` and `etag`, both skipped when `None`, so a
record without them serializes as before. `MetadataRecord` is `deny_unknown_fields`: an older build
reading a record that has them parses it as `LegacyMetadata`, losing nothing but its typed accessors.

*Enforced by:* the `store/pipeline.rs` and `store/storage.rs` unit tests (among them
`per_entry_reserved_names_are_refused_by_the_pipeline`, `storage_none_never_writes`,
`persist_defaults_false_writes_nothing_and_is_stable`, `read_only_pipeline_never_persists`,
`listing_reads_no_bytes_for_unchanged_entries`, `created_comes_from_backend_facts_after_commit`,
`legacy_record_bypasses_chains`, `the_pipeline_reports_no_data_access`), the central-storage tests of
`tests/store_adapters.rs`, and conformance suites `A2`–`A5` (core) and `A3` over OpenDAL fs
(`liquers-store`).

## 14. Handler stages

A record is shaped by four chains, each a `store_config` query compiled into one closure when the
store is built (`store/realm.rs`; the catalogue of entries and how to write one are in
[`guides/STORE_CONFIG_GUIDE.md`](../guides/STORE_CONFIG_GUIDE.md), the realm itself in
[`REALMS.md`](REALMS.md)):

| Stage (field) | Runs | On a step's error |
|---|---|---|
| Default (`on_default`) | on a read of a data object with no stored record, starting from an empty record | the read fails |
| Validate (`on_validate`) | on `set` and `set_metadata`, before anything is written | the write is refused and nothing is written |
| Write (`on_write`) | after Validate, on the record about to be stored | the write fails, nothing is written |
| Read (`on_read`) | on every read, over the stored or default record | the step is skipped and `Metadata handler failed: …` is added to the record's log; the read succeeds |

Steps run in order on one `&mut MetadataRecord`; when two set the same field, the later wins. A step
sees `FactsIn`: the key, the `DataFacts`, the bytes when it asked for them or the operation supplied
them (`set`), the record as stored (`previous`) and the operation name. Change detection compares
with `previous`, not with the record being built. A chain's `needs_bytes` is evaluated first, so the
bytes are read only when some step needs them. A Write step may replace the data
(`StepOut::data`, front-matter write-back), which turns a `set_metadata` into a data write.

An entry in a stage it does not allow, an unknown entry, a non-literal parameter, a resource,
asset or CWD step, a header naming another realm, or a component written in a handler field (or the
reverse) fails the **build**, naming the mount and the field — never a later read. The planner's
`Info` / `Warning` / `Error` steps compile to log entries, added once per record.

A `content_hash` step in `on_read` makes the mount answer `version_verification` (`Off` for
`content_hash-off`, otherwise `OnRead`), which overrides the asset manager's policy for its keys
(`ASSETS.md` §Content changed outside Liquers).

*Enforced by:* the `store/realm.rs` and `store/handlers.rs` unit tests, `validator_rejects_and_nothing_is_written`,
`write_chain_overwrites_and_fills`, `read_chain_fills_a_file_written_outside` (`store/pipeline.rs`),
`front_matter_write_back_through_the_pipeline` (`tests/store_adapters.rs`), conformance suite `A7`.

## 15. Write strategies and recovery

One store operation writes a `WriteSet` (`store/writes.rs`): the data object (`WriteTarget::Data`),
the storage model's objects (`Metadata`), the backend's own record (`Native`) and, under central
storage, backups (`Backup(i)`). The write strategy commits it:

| Strategy | Commit | Reserves | Needs | After a crash |
|---|---|---|---|---|
| `direct` (default) | each object in place, in order — data first | nothing | `data_access` | data and record may disagree; nothing marks it |
| `atomic` | each object atomically: in place where `atomic_write`, else staged at `<key>.__staged__` and renamed | `.__staged__` | `atomic_write` or `rename` | no torn object; a leftover staged object is removed (rolled back). Objects of one write are **not** atomic together |
| `transactional` | every object staged, then a journal `<primary>.__txn__` listing them, then the moves (`rename_data`, or copy then delete), then the journal deleted | `.__staged__`, `.__txn__` | `data_access` | all or nothing: a journal present rolls forward; staged objects with no journal roll back |

A native record is staged as `<key>.__native__.__staged__` and moved with `set_metadata`. A strategy
the backend cannot carry is refused at build (`WriteStrategy::check`).

**Recovery on read.** `get`, `get_bytes`, `get_metadata` and each entry of `listdir_asset_info`
call `WriteStrategy::recover` for the key first; a repair appends "An interrupted write of '…' was
completed/rolled back on read." to the returned record's log. `AsyncStore::sync` settles every
interrupted write in the store at once and reports `SyncReport { repaired, errors }`; a router
merges its members' reports. A read-only mount recovers nothing.

**The `Storing` marker is not written under the pipeline.** The unwrapped `AsyncFileStore` and
`AsyncOpenDALStore` mark the sidecar `Storing` before writing the data; the pipeline writes through
the data-only methods and never does, under any strategy. Use `transactional` where an interrupted
write must not be visible.

*Enforced by:* the `store/writes.rs` unit tests (`sync_reports_repairs` among them),
`recovery_on_read_and_listing_completes_the_write` and `each_strategy_follows_its_protocol`
(`store/pipeline.rs`), `atomic_refused_without_rename_or_atomic_write`, conformance suite `A6`
(each strategy) and `A3` over OpenDAL fs with `atomic`.

## 16. Grouped writes: what is guaranteed

A grouped storage model keeps one file per folder, so every write of a key is a read-modify-write
of a file its siblings share. `MetadataStorage::group_of` names the unit (the folder for `grouped`,
the key itself otherwise), and the pipeline holds that unit's lock — a `tokio::sync::Mutex` in an
`scc::HashMap`, evicted once nobody holds it — while the storage model plans and the strategy
commits. So:

- **Within one process**, concurrent writes to siblings are serialized and none is lost.
- **Across processes** nothing is guaranteed: two processes writing one folder can lose a record
  (`STORE-WRITE-HAS-NO-PRECONDITION`). Run one writer per store.
- The group file's bytes are deterministic (a sorted JSON map), so an unchanged record set rewrites
  identical bytes.
- With `transactional`, the data object and the group file are committed together; with `direct`
  or `atomic` they are not.

*Enforced by:* `grouped_sibling_writes_are_serialized` (`store/pipeline.rs`), conformance suite `A4`.

## History

| Date | Change | Source |
|---|---|---|
| 2026-10-16 | §11: `LocalStorageStore` and `FetchStore` (read half) offer data-only access; `FetchStore::get_stored_metadata` returns its header-derived record. | `design/store-adapters/` phase-5 review |
| 2026-10-16 | Introduction: Liquers assumes it is the only writer of a store's metadata while it runs (outside data edits are detected, outside metadata edits are not). §13: `created` falls back to the write time on a backend without modification times; the record cache's limit stated. | `design/store-adapters/` phase-5 review |
| 2026-10-15 | §11–§16 added: data-only access and `DataCapabilities` per store; `ReadOnlyStore`; the metadata pipeline, storage models and their reserved names, enumerable record-only keys, the facts-keyed cache, `persist_defaults`, `created`/`etag` from backend facts, central storage and backups, record compatibility; handler stages and their error rules; write strategies, recovery on read and `sync`, the `Storing` marker not written under the pipeline; grouped-write guarantees (per-process locks only). Introduction names the adapters and suites `A1`–`A7`. | `design/store-adapters/` |
| 2026-10-13 | §6: how the router answers — data by owner, directories as a union of covering members (and an empty member's prefix), `contains` `Ok(false)` for unowned keys, `get_asset_info` / `listdir_asset_info` forwarded to a sole contributor and composed otherwise. Tests `routerfwd01`-`routerfwd11`, suite `C6`. | `design/store-router-forwards-asset-info/` |
| 2026-10-11 | §10 added: a store seeds a written record's filename, and so its data format, from the key; a declared filename or data format is kept. Rules `seed01`, `seed02`. | `design/store-key-format-seeding/` |
| 2026-10-10 | Introduction: the synchronous `Store` was removed, not merely obsolete; the rules stay trait-neutral for a future synchronous realm. §1 cites `AsyncFileStore` / `AsyncMemoryStore` only. | `CORE-SYNC-STORE-TRAIT-OBSOLETE`, `design/sync-store-removal/` |
| 2026-10-07 | §8: a key with metadata and no data has no data object — `get` and `get_bytes` report `KeyNotFound`, while `set(k, b"", m)` stores an empty one. `AsyncMemoryStore` now behaves so. Enforced by the new rule `sidecar05`. | `design/memory-store-metadata-only-entry/` |
| 2026-09-30 | §4: a delegating store must let the delegate express absence, with `JsStore`'s `null` sentinel as the example. Reviewed §2, §8 and §9 against the implementation and the final conformance reports: every in-tree store passes `dir07` and `sidecar04`. | phase-5 (`design/store-conformance-backlog/`) |
| 2026-09-29 | §8: a key with metadata and no data is enumerable — listed by its parent and answered by `contains` — unless the store refuses the write with `KeyNotFound`. The file stores now list the implied key of a sidecar instead of dropping it. Recorded the two consequences for callers. Enforced by the new rule `sidecar04`. | `design/store-conformance-backlog/` step 6 |
| 2026-09-29 | §2 settled: directory metadata populates `children` with the direct children, one level deep, and the `AsyncStore` default `get_asset_info` answers a directory without reading its metadata, which is what bounds the depth. `dir07` now checks this instead of reporting `Blocked`; `AsyncOpenDALStore` fills `children` like every other store. One ⚠ row remains. | `design/store-conformance-backlog/` step 4 |
| 2026-09-04 | §4 now defines `listdir` on an absent addressable directory as `Ok([])`, while retaining errors for failed filesystem operations and invalid or unsupported keys. This lets a router enumerate an uncreated file-store prefix without treating it as a failed backend. | phase-5 |
| 2026-09-03 | §8 restated as **reserved names** rather than one sidecar suffix: reserved in *any* segment rather than only the filename, declared per store by its own layout, and covering both the suffix form and the exact name — the latter being the predecessor Python implementation's `__metadata__` folder, cited because nothing in this repository evidences it. Named the three kinds of caller that must consult the rule, and why satisfying only `is_supported` is the defect the section exists to prevent. Recorded that listings *skip* reserved names, that the refusal is `KeyNotSupported` with `as_absolute` checked first, and that `get` repairs unparseable metadata — which is what makes an already-corrupted store recoverable. | `design/sidecar-colliding-keys/` Phase 5 |
| 2026-09-02 | Completed the contract. §5 restated as a **postcondition** — `Ok(())` means the directory is gone — from which recursion and the absent-directory case follow, and which makes the trait default's `Err(KeyNotSupported)` correct rather than divergent. §9 settled: `keys()` returns data keys, directories and the prefix, and **every returned key starts with the prefix**; the cost, that an enumerated key is not necessarily readable, is stated rather than hidden. Every *Enforced by* line now names rules in `liquers_core::store_conformance`. Stated trait-neutrally against the possible return of a synchronous store. Two of the three ⚠ rows are gone; §6's was cleared by `async-memory-store-prefix-support`. | `design/store-conformance-suite/` Phase 4 step 1 |
| 2026-09-02 | Recorded that a recursive `removedir` takes explicit descendant directories with it, that `default_metadata` must honour both arguments, and that the directory path form is subject to the same key refusals as the data and metadata forms. All three from PR #58 review findings. | `design/opendal-path-mapping/` PR review |
| 2026-09-02 | Defined `is_supported` cumulatively: absolute key, configured-prefix membership, then optional store-specific exclusions. Added the empty-prefix single-file overlay rationale and memory-store conformance tests. | `design/async-memory-store-prefix-support/` Phase 5 |
| 2026-09-02 | Created. Written against the implementation after `STORE-OPENDAL-SLASH-HANDLING` and `CORE-DIRECTORY-INDEX-NOT-SHARED` were fixed: the sibling rule, the three sources of directory truth, derived versus explicit directories, absence versus failure, removal, prefixes and routing, key shape, metadata sidecars. Three questions are recorded as unsettled rather than answered. | `design/opendal-path-mapping/` Phase 5 |

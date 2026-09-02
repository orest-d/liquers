# Phase 2: Solution & Architecture — OpenDAL path mapping

Based on `HEAD` of `liquers-store/src/opendal_store.rs` (736 lines), `liquers-store/src/store_factory.rs`
and `liquers-core/src/store.rs`, read rather than remembered, and re-resolved on 2026-09-02 after
[`design/store-factories-in-core/`](../store-factories-in-core/) merged (`store_builder.rs` is gone;
`create_opendal_store` is now `OpendalStoreFactory::create`, `store_factory.rs:170`).
Nothing here is implemented.

> **Revised 2026-09-02, three times.** The first draft addressed three defects. The second
> reproduction recorded in Phase 1 added two — one destructive — and disproved this document's own
> claim that `make_sub_dirs` satisfies the `//TODO: create_dir` markers. The document was then
> restructured to the `liquers-project` template when that workflow was adopted at the gate. The
> third revision follows the gate's direction that **the directory fallback belongs in
> `liquers-core`, not private to the OpenDAL store** — §3 is rewritten accordingly, the change
> becomes cross-crate, and `CORE-DIRECTORY-INDEX-NOT-SHARED` is filed and covered here.

## Overview

One insight covers five of the six defects: **a directory path needs a trailing `/`, and three call
sites do not add one.** OpenDAL treats a path without it as a *prefix*, which is why
`removedir("sub")` deletes `subway/`.

The solution makes the trailing slash the *mapping's* business rather than each call site's, by
introducing a single private `PathMap` with a **directory form** alongside the data and metadata
forms, and routing every path construction through it. Six changes follow, in the order they should
be committed:

| # | Change | Crate | Defect | Commit |
|---|---|---|---|---|
| 1 | `PathMap` / `DecodedPath`, every call site routed through it | `liquers-store` | 1, 2, 5 | 1 |
| 2 | `key_prefix()` returns the configured prefix | `liquers-store` | 3 | 2 (own commit, Q2) |
| 3a | **`DirectoryIndex` and shared directory semantics in core** | `liquers-core` | — (`CORE-DIRECTORY-INDEX-NOT-SHARED`) | 3 |
| 3b | OpenDAL supplies its own directory truth and inherits the semantics | `liquers-store` | 4 | 4 |
| 4 | `removedir` doc comment corrected | `liquers-store` | 1 | 1 |
| 5 | `make_sub_dirs` deleted; dead synchronous block deleted | `liquers-store` | 6, Q3 | 5-6 |
| 6 | Warning hygiene; two folded-in P3 issues | `liquers-store` | — | 7 |
| 7 | `AsyncMemoryStore::makedir` records the directory | `liquers-core` | — (`CORE-ASYNC-MEMORY-STORE-MAKEDIR-DOES-NOTHING`) | 8 |

**The change is cross-crate.** Commits 1, 2 and 5-6 are confined to `liquers-store`; commit 3
adds a module to `liquers-core` and adjusts two `AsyncStore` trait defaults, and commit 4 is the
OpenDAL store adopting them. That raises the work from `M` to **`L`**, which is why
`CORE-DIRECTORY-INDEX-NOT-SHARED` was filed rather than absorbed silently: an `L` issue owes a
design folder, and this is it.

**Sequencing matters and is deliberate.** Commits 1-2 are the P0 — the data-loss fix and the
routing correction — and depend on nothing in `liquers-core`. Commit 3 is a new core module that
nothing yet uses. So the P0 can ship, and be reverted, independently of the core work: see
"Recovery" in the risk table.

### Gate decisions folded in

**Q1** directory-key gap: in scope (§3). **Q2** `key_prefix()`: fixed here, own commit (§2).
**Q3** the commented-out synchronous `OpenDALStore` (`:16-218`): **deleted** (§5). **Q4** priority:
stays P0.

## Known-Issue Preflight

Open issues touching this design's area, integration points or assumptions, and what each means
here. No blocker remains.

| Issue | Pri/Cx | Status | Bearing on this design | Blocking? |
|---|---|---|---|---|
| `OPENDAL-LOCALFS-TEST-SILENT-ON-WRONG-VALUE-TYPE` | P3/S | draft | `test_opendal_localfs` (`:705`) `eprintln!`s in both branches, so it reports `ok` whether or not `-R-dir/src` returns `Value::AssetInfo`. It is the only end-to-end guard on `get_asset_info`, which §3 changes — so it would not catch a regression this work could cause. **Folded in** (§6). | No |
| `STORE-OPENDAL-WITHOUT-ASYNC-STORE-BROKEN` | P3/S | draft | `store_factory.rs:22` imports `AsyncOpenDALStore` under `#[cfg(feature = "opendal")]` while the type is gated on `async_store`, so `--no-default-features --features opendal` fails to build. One `#[cfg]` line, in a file this change already touches. **Folded in** (§6). | No |
| `STORE-OPENDAL-LIST-OPTION-MISPARSED` | P2/S | draft, design `opendal-list-option-config` | Lives in `store_factory.rs`'s option parsing, not in `opendal_store.rs`. The only textual overlap is the import block §6 touches. No ordering constraint either way. | No |
| `STORE-OPENDAL-ARGUMENTS-NOT-DERIVED` | P3/S | draft, design `store-factories-in-core` | That design is `complete` (PR #46, merged). The remaining feature work is argument metadata in `store_factory.rs` and does not touch `opendal_store.rs`. | No |
| `STORE-ABSOLUTE-KEY-NOT-TYPE-ENFORCED` | P1/L | accepted | The reason `PathMap`'s entry points stay fallible: absoluteness is enforced per method, by convention, not by the type. This change preserves that enforcement and concentrates it, which is a step toward the issue rather than away from it. | No |
| `CORE-ASYNC-MEMORY-STORE-MAKEDIR-DOES-NOTHING` | P0/S | draft (**filed by this design**, 2026-09-02, from Phase 3) | `AsyncMemoryStore::makedir` (`store.rs:888`) records nothing and reports success; `PUT /api/store/makedir/{*key}` is a documented endpoint. `DirectoryIndex::explicit` is precisely the missing capability, so the fix is one call to `insert_directory` — sequenced as its own commit after the extraction, so the extraction stays behaviour-preserving. | No |
| `CORE-DIRECTORY-INDEX-NOT-SHARED` | P1/L | accepted (**filed by this design**, 2026-09-02) | **Now covered by this design.** Four stores each reimplement directory derivation and a fifth has nothing; the gate directed that the fallback live in `liquers-core` so `liquers-web`'s HTTP-backed stores can use it too. §3a is its architecture. | No — it *is* the work |
| `STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE` | P1/L | draft (**filed by this design**, 2026-09-02) | The suite that would enforce the semantics §3a puts in core across all implementations. Still out of scope: §3a gives the semantics one *implementation* to inherit, the suite gives them an *enforcement*, and the second is a separate body of test work. The contract they share is written at Phase 5. | No |
| `CORE-STORE-OPENBIN-MISSING` | — | draft | `openbin` is unimplemented here as everywhere; its `TODO` (`:503`) is untouched. | No |
| `LIBRARY-CODE-USES-UNWRAP-AND-EXPECT` | P1/L | draft | Two of its instances are in this file (`:279`, `:488`). Both are removed by §5 and §1, so this change reduces that issue's surface. | No |
| `STORE-CONFIG-IN-CORE` | — | closed | Merged. Its `opendal03` test carries a comment deferring the `key_prefix()` assertion to *this* design; §2 enables it. | No |

## Data Structures

```rust
/// The one place that maps a `Key` onto a backend path and back.
///
/// A store key is absolute (`liquers_core::store`), so every fallible entry point starts with
/// `Key::as_absolute`. A data path is the key's `encode()` form. A **directory path additionally
/// carries a trailing `/`**, which OpenDAL requires: without it `list`, `remove_all` and
/// `create_dir` treat the path as a prefix or a file, which is how `removedir("sub")` came to
/// delete `subway/`.
struct PathMap;

/// What a backend path denotes. Explicit, so a caller cannot forget that a listing yields
/// metadata sidecars and directory entries alongside data entries.
enum DecodedPath {
    Data(Key),
    Metadata(Key),   // the key of the data it describes
    Directory(Key),
}
```

**Ownership:** `PathMap` is a unit type with associated functions — no state, no lifetimes, nothing
to own, nothing to clone or lock. `DecodedPath` owns its `Key`, which is already an owned type.
Neither is `pub`; both live inside `opendal_store.rs`.

**Serialization:** none. Neither type crosses a process or file boundary; no `serde` derives.

**Not created:** a `liquers-store/src/path_map.rs` module. WP-5 proposed the file, but the
deliverable Phase 1 states is "one place", not "one file", and ~70 lines with a single caller do not
earn public surface. Revisit if a second backend needs the same rules.

**Unchanged:** `AsyncOpenDALStore` keeps its two fields (`op: Operator`, `prefix: Key`, `:222-223`).
No field is added, removed or retyped — the OpenDAL store answers from a listing, not from an
index, so it gains no state.

### In `liquers-core`: `store_dir_index.rs`

A new module beside `store.rs`, following the crate's existing `store_config.rs` /
`store_factory.rs` sibling pattern rather than growing a 2605-line file:

```rust
/// Derived directory structure for a backend that has no directory objects.
///
/// A flat key set implies a tree: every proper prefix of a stored key is a directory. This type
/// owns that derivation so a store supplies only its own source of truth. Concurrent and
/// interior-mutable, matching `AsyncMemoryStore`, which is where the mechanism came from.
pub struct DirectoryIndex {
    /// parent -> child key -> how many stored keys keep that child alive.
    derived: scc::HashMap<Key, Arc<scc::HashMap<Key, usize>>>,
    /// Directories that exist because `makedir` created them, even with no children.
    explicit: scc::HashSet<Key>,
}
```

**Why refcounts and not a set:** removing one key must not delete a directory another key still
occupies. `AsyncMemoryStore` already learned this; the counts come with the extracted code.

**Why `explicit` is a second field:** a derived index cannot represent an empty directory that
genuinely exists. `LocalStorageStore` needed exactly this and grew a private `explicit_dirs` set
next to its `dirs` map; `AsyncMemoryStore` has no such notion and therefore cannot honour `makedir`
on an empty directory. Carrying both is what lets one type serve every caller, and it is why Phase
2 could reject "always synthesize from a listing" for OpenDAL on the ground that it loses the
distinction — the distinction is now representable.

**Ownership and concurrency:** `scc` is already an unconditional `liquers-core` dependency and
compiles for `wasm32-unknown-unknown` (the crate is the base of `liquers-web`), so one concurrent
type serves the native and browser stores alike. `Arc` is the existing choice for the inner map and
is kept. The index owns its keys; `Key` is an owned type.

**Serialization:** none. The index is derived state, rebuilt from the keys, never persisted.

## Trait Implementations

`AsyncStore for AsyncOpenDALStore` is the only trait impl touched. No trait is added, no trait is
modified, and `liquers-core` is not edited.

| Method | Line | Change |
|---|---|---|
| `key_prefix` | `:296` | return `self.prefix.clone()` instead of `Key::new()` |
| `get_metadata` | `:318` | the `KeyNotFound` branch consults `has_children` and returns directory metadata when the backend has children but no directory object |
| `set` / `set_metadata` | `:361`, `:378` | drop the `make_sub_dirs` call and the `//TODO: create_dir` above it |
| `removedir` | `:408` | `PathMap::directory`; doc comment corrected |
| `contains` | `:414` | add the `is_dir` fallback `AsyncMemoryStore` has |
| `is_dir` | `:427` | `NotFound` falls back to `has_children` instead of propagating |
| `listdir` | `:452` | `PathMap::directory`; decode entries through `DecodedPath`, skipping undecodable ones |
| `listdir_keys_deep` | `:481` | `PathMap::directory`; `unwrap()` at `:488` removed |
| `makedir` | `:499` | `PathMap::directory` |
| `keys`, `get`, `get_bytes`, `remove`, `listdir_keys`, `is_supported`, `store_name`, `default_metadata` | — | unchanged in body; `keys` and `store_name` change *behaviour* through `key_prefix()` |

### `AsyncStore` trait defaults, in `liquers-core`

One default changes, so that a store which answers `is_dir` inherits the rest instead of restating
it. It is a widening of a permissive default, not a new obligation, so no implementation is required
to change. (This section originally changed **two**; Phase 4's R2 dropped the second — see the
struck row.)

| Default | Line | Today | After |
|---|---|---|---|
| `contains` | `store.rs:442` | `Ok(false)` | data, else metadata is the store's business; the default falls back to `self.is_dir(key)`, which is what `AsyncMemoryStore` (`:810`) and `LocalStorageStore` already do by hand |
| ~~`get_metadata`~~ | `store.rs:397` | — | **Dropped by Phase 4, refinement R2.** The proposed `directory_metadata_includes_children` hook would have had **no consumer**: every in-tree store overrides `get_metadata`, `AsyncOpenDALStore` included, and it fixes its own override in step 6. A trait method added for a hypothetical caller is speculative API on a trait every integration implements. |

`is_dir`'s own default stays `Ok(false)` — a store with no directory concept should say no, not
guess. What changes is that "absent key means `Ok(false)`, never `Err`" becomes a documented rule
that `AsyncOpenDALStore` now honours.

**`keyabs17_trait_defaults_refuse_relative_keys` (`store.rs:2355`) must stay green unchanged.** It
asserts the defaults refuse a relative key and stay permissive for an ordinary one:
`contains(ok) == false` still holds through the new fallback, because `is_dir`'s default is
`Ok(false)`; and `key.as_absolute()?` is evaluated before the fallback, so the refusal is
unaffected. This was checked against the test body, not assumed.

**Exhaustive matching:** `DecodedPath` is ours, so every match over it is exhaustive with no `_`
arm. `opendal::ErrorKind` is a foreign `#[non_exhaustive]` enum, so a catch-all there is
unavoidable and is the one permitted exception; it is written as
`Err(e) if e.kind() == ErrorKind::NotFound` plus `Err(e)`, which reads as two arms rather than a
wildcard over our own type.

## Sync vs Async

Everything stays `async`; no blocking call is introduced and no sync wrapper is added.

- `PathMap`'s four functions are **synchronous and pure** — they are string arithmetic over a `Key`
  and touch no I/O. Making them `async` would force an `.await` on every call site for no reason.
- `has_children` is `async` because it awaits `Operator::list_with`.
- `DirectoryIndex`'s accessors are `async` because `scc`'s are (`read_async`, `insert_async`). That
  costs nothing: every caller is already inside an `async fn` of `AsyncStore`. Its pure helper —
  the parent/child edge derivation lifted from `AsyncMemoryStore::index_edges_for_key` — stays
  synchronous and is directly unit-testable.
- `liquers-core` compiles for `wasm32-unknown-unknown`, where `AsyncStore` is
  `async_trait(?Send)`. `DirectoryIndex` adds no `Send` requirement of its own beyond what `scc`
  and `AsyncMemoryStore` already impose on that target, so the browser build is unaffected. This is
  checked by the wasm32 rows of `scripts/check-build-matrix.sh`.
- `is_dir`'s new fallback is one additional `.await` on the branch that today returns `Err`; on a
  backend with directory objects the `stat` succeeds and nothing extra is awaited.
- No `spawn`, no `block_on`, no shared mutable state, so no `Send`/`Sync` bound changes. `PathMap`
  is stateless, so the store remains as `Send + Sync` as it is today.

## Function Signatures

```rust
impl PathMap {
    const METADATA: &'static str = ".__metadata__";

    /// True when the key's filename ends in METADATA, so its data path would collide with another
    /// key's metadata path. **Added by Phase 4, R1**: `Error::key_not_supported` needs a store
    /// name, which an associated function cannot reach, and `store_name()` allocates a String per
    /// call on the key-encoding path. So the *predicate* lives here and the *error* on the store,
    /// where `is_supported` and every path entry point consult the same rule.
    fn is_suffix_ambiguous(key: &Key) -> bool;

    /// "sub/foo.txt" — fallible via `Key::as_absolute`; suffix refusal is the store's, per R1.
    fn data(key: &Key) -> Result<String, Error>;

    /// "sub/foo.txt.__metadata__" — same refusals.
    fn metadata(key: &Key) -> Result<String, Error>;

    /// "sub/" — the root key maps to "". The trailing slash is the whole point.
    fn directory(key: &Key) -> Result<String, Error>;

    /// Backend path -> what it denotes. Strips a trailing '/' BEFORE the metadata suffix, and
    /// strips the suffix from the final segment only, once.
    fn decode(path: &str) -> Result<DecodedPath, Error>;
}

impl AsyncOpenDALStore {
    // unchanged signatures, now one-line delegations
    pub fn key_to_path(&self, key: &Key) -> Result<String, Error>;          // -> PathMap::data
    pub fn key_to_path_metadata(&self, key: &Key) -> Result<String, Error>; // -> PathMap::metadata
    pub fn path_to_key(&self, path: &str) -> Result<Key, Error>;            // -> PathMap::decode

    /// New. True when the backend holds anything under this key's directory path.
    async fn has_children(&self, key: &Key) -> Result<bool, Error>;

    // deleted: async fn make_sub_dirs(&self, key: &Key) -> Result<(), Error>
}
```

### The directory form is the fix

| Call site | Line | Today | After |
|---|---|---|---|
| `removedir` | `:408` | `remove_all("sub")` — prefix delete, **destroys `subway/`** | `remove_all("sub/")` |
| `listdir_keys_deep` | `:481` | `list_with("sub").recursive(true)` — **lists `subway/…`** | `list_with("sub/")` |
| `listdir` | `:452` | inline `trim_end_matches('/') + "/"` | `PathMap::directory` |
| `makedir` | `:499` | inline `format!("{}/", …)` | `PathMap::directory` |
| `make_sub_dirs` | `:279` | `create_dir("sub")` — **always fails**, error discarded | deleted (§5) |

Verified against the operator before proposing it (Phase 1 Appendix A records the raw output): with
the trailing slash a recursive `list` of `sub/` returns only `sub/…`, and `remove_all("sub/")`
leaves `subway/b.txt` in place, on both the memory and filesystem backends.

### Encoding rules the decoder must satisfy

Strip the trailing `/` **before** the metadata suffix, and strip the suffix from the final segment
only, once. Today's `path.trim_matches('/').trim_end_matches(Self::METADATA)` (`:242-243`) strips
the suffix *repeatedly*, so `x.__metadata__.__metadata__` decodes to `x`; no reachable path produces
that, but the corpus pins the single-strip rule down.

**Suffix-ending keys are excluded, not round-tripped.** `PathMap::data` for the key
`foo.__metadata__` and `PathMap::metadata` for the key `foo` produce the *same* path, so no decoder
can be injective over both while preserving the on-disk layout. `is_supported` (`:514-520`) already
refuses a key whose filename ends in the suffix, so such a key never reaches this store — but
`key_to_path` accepts it today (confirmed: it returns `Ok("a.__metadata__")`), so the rule lives in
one method and is absent from another. After Phase 4's R1, `is_supported` and the store's path
entry points share the single predicate `PathMap::is_suffix_ambiguous`, and the store raises
`Error::key_not_supported` with its own name. An unambiguous encoding (escaping the suffix) would change the on-disk
layout and is out of scope.

**Lenient in, strict out.** `PathMap::decode` is applied to paths the *backend* returns, which
nothing in Liquers necessarily wrote — a stray `orphan.__metadata__` with no data file is already
reported as key `orphan` today. `listdir` and `listdir_keys_deep` therefore **skip** an entry they
cannot decode rather than failing the whole listing; only `PathMap::data` and `PathMap::metadata`,
which encode a key the caller supplied, return an error.

### §2 — `key_prefix()` returns the configured prefix

```rust
fn key_prefix(&self) -> Key {
    self.prefix.clone()
}
```

Matching `AsyncFileStore` (`liquers-core/src/store.rs:1022`) and `FileStore` (`:1310`). The prefix
convention in this codebase is that the prefix is part of the path under the backend root —
`FileStore::key_to_path` pushes the whole key, prefix included, onto `self.path` — so `key_to_path`
needs no change: only the *advertised* prefix was wrong. Consequences, all intended: `keys()`
(`:434`) enumerates from the prefix instead of the backend root, so it stops reporting the root key
`""`, which is outside the store's own prefix; `AsyncStoreRouter::is_dir` (`store.rs:2053`) and
`listdir` (`:2080-2097`) stop offering this store every key; `store_name()` identifies the store
instead of printing `" OpenDAL Store"`.

### §3a — Shared directory support in `liquers-core`

The gate's direction: the fallback belongs in core, because the same problem exists outside the
OpenDAL store. The evidence is stronger than "will exist" — **four stores already solve it, no two
alike, and a fifth has nothing**:

| Store | Crate | Mechanism |
|---|---|---|
| `AsyncMemoryStore` | `liquers-core` | `dir_index`, refcounted, maintained by `set`/`remove` (`store.rs:580`, `:629-664`) |
| `MemoryStore` (sync) | `liquers-core` | no index; `is_dir` scans every key per call (`:1607`) |
| `FetchStore` | `liquers-web` | `directory_index()` built once from a configured key set (`store/fetch.rs:130`) |
| `LocalStorageStore` | `liquers-web` | `index_key()` plus `explicit_dirs` for empty directories (`store/local_storage.rs:353`, `:98`) |
| `AsyncOpenDALStore` | `liquers-store` | **none** — this issue's defect 4 |

```rust
// liquers-core/src/store_dir_index.rs
impl DirectoryIndex {
    pub fn new() -> Self;

    /// Build from a known key set — what `FetchStore` does at construction.
    pub async fn from_keys(keys: impl IntoIterator<Item = Key>) -> Self;

    /// Maintain incrementally — what `AsyncMemoryStore` does on `set` / `remove`.
    pub async fn insert_key(&self, key: &Key);
    pub async fn remove_key(&self, key: &Key);

    /// Record an empty directory that genuinely exists — what `makedir` needs and what only
    /// `LocalStorageStore` can express today.
    pub async fn insert_directory(&self, key: &Key);
    pub async fn remove_directory(&self, key: &Key);

    /// True when the key has children, or was explicitly created.
    pub async fn is_dir(&self, key: &Key) -> bool;
    /// Child names directly under the key, sorted.
    pub async fn children(&self, key: &Key) -> Vec<String>;
    pub async fn child_keys(&self, key: &Key) -> Vec<Key>;

    /// Every ancestor/child edge a key implies. Pure, synchronous, unit-testable.
    fn edges_for_key(key: &Key) -> Vec<(Key, Key)>;
}
```

**A store supplies its source of directory truth; core supplies the semantics.** Three sources,
one set of answers:

| Backend shape | Source of truth | Stores |
|---|---|---|
| Real directories | `stat` the path | `AsyncFileStore` |
| A listing, no directory objects | a bounded listing (§3b) | `AsyncOpenDALStore` |
| Neither | `DirectoryIndex` | `AsyncMemoryStore`, `FetchStore`, `LocalStorageStore` |

Each store keeps its own `is_dir`. What it stops restating is everything downstream of `is_dir`:
`contains` falling back to it, `is_dir` on an absent key being `Ok(false)` rather than an error, and
a directory key's metadata being `default_metadata(key, true)` — which the trait defaults now
provide (see Trait Implementations).

**Who adopts it in this change:** `AsyncMemoryStore` (the mechanism is extracted *from* it, so its
behaviour must not change).

> **Corrected by Phase 3, Finding 1.** This section originally rested that safety on
> "its existing tests must pass unchanged". Counted at `HEAD`, those are **one** behavioural test
> (`test_async_memory_store_basic`, `store.rs:2194`) plus `keyabs07`, and the behavioural one covers
> a single key, one directory level, and never checks `is_dir` after a removal. It cannot prove a
> refcounted-index extraction faithful. Phase 3's plan therefore **writes characterization tests
> against `HEAD` first** (`MEMDIR01-05`), commits them before the extraction, and requires them to
> pass unchanged after. The claim was not evidence; counting the tests took one grep.

> **Also corrected by Phase 3, Finding 2.** `DirectoryIndex::explicit` gives `AsyncMemoryStore` a
> capability it lacks, and adopting it would change behaviour: `makedir` (`store.rs:888`) is a
> silent no-op today — it validates its key, returns `Ok(())` and records nothing, so `is_dir` is
> `false` immediately afterwards. Filed as `CORE-ASYNC-MEMORY-STORE-MAKEDIR-DOES-NOTHING` (P0/S:
> `PUT /api/store/makedir/{*key}` is specified in `reference/WEB_API_SPECIFICATION.md` §4.1.10, and
> a documented feature that does not work is §4.4's P0; the practical consequence is small). The
> extraction commit therefore **keeps `makedir` a no-op**, so it is provably behaviour-preserving,
> and a separate later commit makes it call `insert_directory`. One behaviour change, one commit,
> visible in the diff.

**Who does not, yet:** and `AsyncOpenDALStore` (§3b). `FetchStore` and `LocalStorageStore`. They work today, they are wasm-only with their own Node/browser/Playwright test
loops that do not fit the native loop, and migrating them is cleanup rather than repair. It is filed
as follow-up under `CORE-DIRECTORY-INDEX-NOT-SHARED` rather than done here — the issue's requirement
is that the mechanism be *available* in core, and it will be. The sync `MemoryStore` is likewise
left alone; its O(n) `is_dir` is a performance matter, not a correctness one.

### §3b — OpenDAL supplies a listing, and inherits the rest

`is_dir` (`:427`) currently propagates the backend's `stat` failure, so an absent key yields `Err`
where every other store yields `Ok(false)`.

```rust
async fn is_dir(&self, key: &Key) -> Result<bool, Error> {
    let path = PathMap::data(key)?;
    match self.op.stat(&path).await {
        Ok(stat) => Ok(stat.is_dir()),
        Err(e) if e.kind() == opendal::ErrorKind::NotFound => self.has_children(key).await,
        Err(e) => Err(/* map_read_error */),
    }
}

/// True when the backend holds anything under this key's directory path.
async fn has_children(&self, key: &Key) -> Result<bool, Error> {
    let path = PathMap::directory(key)?;
    let entries = self.map_read_error(key, self.op.list_with(&path).limit(1).await)?;
    Ok(!entries.is_empty())
}
```

`limit` is a page-size hint, not a hard cap — the memory backend returned two entries for
`limit(1)` — so `has_children` tests for **non-empty**, never for a count. An absent directory
returns `Ok(vec![])`, not an error, on both backends probed, which is what makes `is_dir` return
`Ok(false)` for an absent key.

**Why OpenDAL uses a listing and not a `DirectoryIndex`.** The backend is authoritative and can be
written by anything — another process, another tool, a different Liquers instance against the same
bucket. A write-side index would go stale the moment it was not the only writer, and rebuilding it
at construction means listing the entire bucket. A bounded listing asks the authority the question
directly and costs one call, on the branch that today returns an error. This is the one case where
core's index is the wrong tool, which is precisely why the core piece is an index *plus* semantics
rather than an index alone.

`contains` (`:414`) and `get_metadata` (`:318-357`) then need only what the trait defaults give,
with one exception this design must not lose: **`AsyncOpenDALStore` overrides `get_metadata`
entirely** and never calls `is_dir`. It checks the metadata sidecar, then `op.exists(data_path)`,
then `op.stat` to decide `is_dir()`, and otherwise returns `KeyNotFound`. On a backend with no
directory object both `exists` calls are false, so `get_metadata("sub")` fails — and
`AsyncStore::get_asset_info` (`store.rs:405`) starts with `self.get_metadata(key).await?`, so it
fails too. Fixing `is_dir` alone does **not** satisfy acceptance criterion 4. The `KeyNotFound`
branch must consult `has_children` and, when it reports children, return
`Metadata::MetadataRecord(self.default_metadata(key, true))` — the same value the `stat().is_dir()`
branch already returns.

**Why a listing and not `create_dir`.** OpenDAL's `memory` and `s3` services do not implement
`create_dir` at all (only `fs` does, `services/fs/backend.rs:193`), so on an object store there is
nothing to create. Writing a zero-byte `sub/` object by hand would change the on-disk layout.
**Cost:** one extra listing call, only on the path that today returns an error, asking for one page.

### §4 — `removedir`, beyond the trailing slash

- `remove_all` on a path OpenDAL reports as absent is `Ok(())` — removing a non-existent directory
  is a no-op, matching `AsyncFileStore` (`store.rs:1171-1183`). No change needed; the test asserts
  it so a future rewrite cannot silently turn it into an error.
- The doc comment *"Files are not removed recursively"* (`:405-407`) is false — `remove_all` is
  recursive, and so are the other two async stores. Correct the comment to describe recursive
  removal scoped to the directory. Behaviour is **not** changed to match the comment: a
  non-recursive `removedir` is nobody's contract and would break `AsyncStoreRouter`'s delegation.

### §5 — `make_sub_dirs` deleted; dead synchronous block deleted (Q3)

`make_sub_dirs` has never worked (Phase 1's evidence: `create_dir` without a trailing slash is
rejected by OpenDAL unconditionally, and `let _ignore` at `:281` discards the error). **Delete it,
with its two call sites in `set` (`:362`) and `set_metadata` (`:379`) and the two
`//TODO: create_dir` markers above them.** Deleting a no-op cannot change behaviour, and fixing it
would: on `fs` it would create directories the writer already creates, on `s3` and `memory`
`create_dir` is unimplemented, and on a backend that does implement it the store would start writing
directory markers it never wrote before — a layout change, which the compatibility section forbids.
Explicit empty-directory creation stays available through `makedir`, which adds the slash and works.

**The commented-out synchronous `OpenDALStore` block (`:16-218`) is deleted too** — the gate's Q3
decision. It is 200 lines, 27% of the file, it cannot compile, and it holds the other two of the
issue's four `//TODO: create_dir` citations; leaving it would close the issue with two citations
untouched. It is a pure deletion in its own commit, so it reverts alone and does not complicate
review of the correctness commits. The type it describes is recoverable from git history if a
synchronous OpenDAL store is ever wanted; nothing references it.

Deleting `make_sub_dirs` removes one of the two `unwrap()`s (`:279`). The other, in
`listdir_keys_deep` (`:488`), becomes a `filter_map` that skips a key whose prefix cannot be taken —
unreachable, but `unwrap()` in library code is forbidden by `CLAUDE.md` regardless.

### §6 — Hygiene and folded-in issues

- Delete the stale `FIXME` at `:340` and replace it with what is true: *"Directory children are
  deliberately not populated here — `listdir_asset_info` walks the whole subtree."*
- Fix the two warnings this file emits at `HEAD`, both in lines being touched: unused `Store`
  import (`:8`), unnecessary `mut` (`:339`).
- **`OPENDAL-LOCALFS-TEST-SILENT-ON-WRONG-VALUE-TYPE`**: make `test_opendal_localfs`'s `else`
  branch `panic!`, and assert the computed `names` set contains `"opendal_store.rs"`.
- **`STORE-OPENDAL-WITHOUT-ASYNC-STORE-BROKEN`**: take that issue's option 1 — gate the
  `store_factory.rs:22` import and its uses on
  `#[cfg(all(feature = "opendal", feature = "async_store"))]`, so `--no-default-features --features
  opendal` compiles and offers no OpenDAL store type.

## Integration Points

| Crate / module | Edited? | Effect |
|---|---|---|
| `liquers-store/src/opendal_store.rs` | **yes** | the whole implementation and its colocated tests |
| `liquers-store/src/store_factory.rs` | **yes**, one `#[cfg]` line | `opendal`-without-`async_store` compiles; `opendal03`'s `key_prefix()` assertion enabled |
| `liquers-core/src/store_dir_index.rs` | **new file** | `DirectoryIndex` — the shared mechanism (§3a); registered as `pub mod store_dir_index;` in `lib.rs` |
| `liquers-core/src/store.rs` | **yes**, narrowly | two `AsyncStore` trait defaults (`contains` `:442`, `get_metadata` `:397`); `AsyncMemoryStore` (`:578-900`) adopts `DirectoryIndex` in place of its private `dir_index`, with no behaviour change. Separately, `AsyncStoreRouter::is_dir` (`:2053`), `listdir` (`:2080`) and `listdir_keys_deep` change *behaviour* through `key_prefix()` without being edited |
| `liquers-web/src/store/{fetch,local_storage}.rs` | no | each keeps its private index for now; migration filed as follow-up under `CORE-DIRECTORY-INDEX-NOT-SHARED`. They must keep compiling and passing against the changed trait defaults — the wasm32 build-matrix rows and the `liquers-web` Node test loop are the check |
| `liquers-axum/src/store/handlers.rs` | no | `DELETE /api/store/removedir/{*key}` (`:398`) stops deleting siblings; listing endpoints stop reporting them |
| `liquers-lib/examples/ui_query_console_app.rs` | no | the only in-tree constructor of an `AsyncOpenDALStore` outside tests |
| `liquers-web` (rest) | no | no longer depends on `liquers-store` at all, but **does** depend on `liquers-core`, so the trait-default change reaches it |
| `liquers-py` | no | wraps `Store`, not this type |

**Read, unchanged:** `AsyncStore` and its defaults (`store.rs:329-545`), `AsyncStoreRouter`
(`:1909-2160`), `AsyncMemoryStore` (`:578-900`), `AsyncFileStore` (`:904-1265`),
`OpendalStoreFactory::create` (`store_factory.rs:170-195`).

**Feature gates:** in `liquers-store`, every changed symbol is already inside
`#[cfg(feature = "async_store")]`, so no new gate is needed; §6 is the one place a gate changes.
`store_dir_index` is unconditional in `liquers-core`, like `store.rs` itself, and adds no
dependency — `scc` is already there. `scripts/check-build-matrix.sh` must stay green, including the
`liquers-core` `--no-default-features` row, both wasm32 rows, the `opendal`-off configuration, and
the `opendal`-without-`async_store` row that §6 unblocks. The `liquers-web` Node conformance loop
(`cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`) is run after
a `cargo clean`, per `CLAUDE.md`.

## Documentation Architecture

Phase 1's four decisions, made exact.

| Document | Kind | Action | Audience | Content |
|---|---|---|---|---|
| `specs/reference/STORE_SEMANTICS.md` | reference | **create, at Phase 5** *(confirmed at the gate as desirable and as Phase 5 work)* | internal — anyone implementing or calling an `AsyncStore` | The store behavioural contract that four of the six defects violate, now also the specification `DirectoryIndex` implements: what `is_dir` and `contains` mean when the backend has no directory objects; that `is_dir` on an absent key is `Ok(false)`, not an error; that `removedir` is recursive and **scoped to the directory**, so no operation on a key may reach a sibling key; that removing an absent directory is a no-op; the three sources of directory truth (`stat`, a bounded listing, `DirectoryIndex`) and which backend shape uses which; that an explicitly created empty directory is distinct from a derived one; the prefix convention (a store's `key_prefix` is part of the path under its backend root, and `key_prefix()` must report it). Written at Phase 5 against implemented and tested behaviour — not before, so it describes what shipped. |
| `liquers-core/src/store_dir_index.rs` rustdoc | code | **create** | implementers | Module-level rustdoc carrying the same rules at the point of use, pointing at `STORE_SEMANTICS.md` for the contract. A new store's author reads the module before the spec. |
| `specs/reference/STORE_CONFIG_FSD.md` | reference | **not extended** | — | Specifies configuration, not semantics. A cross-link to `STORE_SEMANTICS.md` is added; no other change. Gets a `## History` row and a `reviewed:` bump for that link. |
| `specs/guides/*` | guide | **neither** | — | No repeatable developer task is introduced; nobody "uses" a path mapping. Reconsider if Phase 3 finds the prefixed-store configuration needs a worked example — the prefix convention is currently folklore, and `STORE_SEMANTICS.md` writing it down may be enough. |
| `specs/issues/STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE.md` | issue | **created** 2026-09-02 | — | The suite that would enforce `STORE_SEMANTICS.md` across all implementations. `L`, hence filed not folded. |
| `specs/issues/CORE-DIRECTORY-INDEX-NOT-SHARED.md` | issue | **created** 2026-09-02; `status: closed` at Phase 5 | — | Covered by this design (§3a). Its `design:` field points here and this design's `issues:` list names it. |
| `specs/README.md` §Stores | map | **updated** 2026-09-02 | — | Corrected statement and the P0 raise; capability-map link kept at `designing` until Phase 5. |
| `specs/issues/STORE-OPENDAL-SLASH-HANDLING.md` | issue | **updated** 2026-09-02; `status: closed` at Phase 5 | — | Evidence update and priority raise done. |
| `specs/issues/OPENDAL-LOCALFS-TEST-SILENT-ON-WRONG-VALUE-TYPE.md` | issue | `status: closed` at Phase 5 | — | Resolution note pointing here. |
| `specs/issues/STORE-OPENDAL-WITHOUT-ASYNC-STORE-BROKEN.md` | issue | `status: closed` at Phase 5 | — | Resolution note naming the option taken. |
| `specs/design/opendal-path-mapping/phase5-documentation.md` | design | **create** at Phase 5 | — | Mandatory under `workflow: liquers-project`. |
| `specs/index.csv` | generated | regenerate | — | `python3 scripts/docs_index.py`. |

**Proposed `affects_docs`:** `[reference/STORE_SEMANTICS.md, reference/STORE_CONFIG_FSD.md]`.
Generated candidates by `area: [core/store, store/backends, web]` also surface
`reference/ENVIRONMENT_CONFIG.md`, `guides/STORE_FACTORY_GUIDE.md`,
`reference/api/DOC_01_ARCHITECTURE_REFERENCE.md` and `guides/LANGUAGE-INTEGRATION_GUIDE.md`. The
first two describe configuration and factories and are expected to be dropped; the last two are
reviewed at Phase 5 against the changed trait defaults, since a language integration implementing
`AsyncStore` inherits them.

**Links to add:** `specs/README.md` §Stores gains a `documented →` entry for `STORE_SEMANTICS.md`
at Phase 5, and the `opendal-path-mapping` entry moves from `designing` to `documented`.

## Relevant Commands

**New commands: none.** This change adds, removes and modifies no command, and touches no command
namespace. `specs/command_registry.yaml` is unaffected and `cargo test -p liquers-lib --test
registry_export` is unaffected.

**Existing commands reached, but not modified** — the store is the data source behind them, so they
are what a reviewer should exercise to see the fix:

| Namespace / form | Relevance |
|---|---|
| `-R/<key>` (resource query) | reads through `get_bytes` / `get_metadata`; unchanged for existing correct paths, and the regression test (c) pins that down |
| `-R-dir/<key>` (directory resource query) | reads through `get_asset_info` → `get_metadata`, which §3 changes: a directory key on an object-store backend becomes addressable. `test_opendal_localfs` exercises exactly this and is the test §6 makes assert |
| `root` namespace `store` commands, `liquers-axum` store routes | `removedir`, `listdir`, `makedir` reach the changed methods directly |

No `ns-pl` (Polars), `ns-img` (image), `lui`/`egui` (UI) command is involved; this is below the
command layer entirely.

## Error Handling

- All errors stay `liquers_core::error::Error`, produced by the existing `map_read_error` /
  `map_write_error` helpers (`:251`, `:264`) and the typed constructors `Error::key_not_found` and
  `Error::key_not_supported`. **No new error type, no `Error::new`.**
- `PathMap::data`, `::metadata` and `::directory` return `Result` because `Key::as_absolute` is
  fallible. This is the `STORE-KEY-GUARD` rule and must not regress:
  `keyabs16_opendal_store_refuses_relative_keys` (`:540`) asserts `ErrorType::KeyNotAbsolute` on
  seven methods and on `key_to_path` directly, and must stay green **unchanged**.
- `PathMap::data`/`::metadata` add one refusal — a filename ending in `METADATA`, as
  `Error::key_not_supported` — which `is_supported` already refused; the rule now lives in one
  place instead of one method having it and another not.
- **No `unwrap()` or `expect()` remains outside tests**: `:279` goes with `make_sub_dirs`, `:488`
  becomes a `filter_map`. No `println!` anywhere; diagnostics use `eprintln!` if any are needed.
- **Error-path behaviour changes**, all corrections: `is_dir` and `contains` return `Ok(true)` where
  they returned `Err` for a directory whose children exist; `is_dir` returns `Ok(false)` where it
  returned `Err` for an absent key. Every other error is unchanged.
- `listdir`/`listdir_keys_deep` **skip** an entry `PathMap::decode` refuses rather than failing the
  listing, so one unexpected object in a bucket cannot make a directory unlistable.

## Rejected Alternatives

| Option | Verdict |
|---|---|
| Patch the trailing slash at `removedir` and `listdir_keys_deep` only | Rejected: that would be the third time this bug is fixed one call site at a time (`listdir` and `makedir` already carry inline slash arithmetic, and the two defects found on 2026-09-02 are the sites that were missed). `PathMap::directory` makes the omission impossible to repeat. |
| A separate `liquers-store/src/path_map.rs`, as WP-5 proposed | Rejected: ~70 lines with one caller. "One place" is satisfied by one private type in the file that uses it; a module adds public surface nobody asked for. |
| Fix `make_sub_dirs` instead of deleting it | Rejected: it would start writing directory markers on backends that support `create_dir`, changing the on-disk layout, to no benefit. |
| Materialize directory markers so directory keys become addressable | Rejected: `memory` and `s3` do not implement `create_dir`, so it does not solve defect 4 where defect 4 exists. |
| Honour the FIXME and populate `children` in directory metadata | Rejected: a full recursive subtree walk per directory read. Whether the `AsyncStore` default that does this (`store.rs:396-403`) is right is a separate question. |
| Make `is_dir` always synthesize from a listing, ignoring `stat` | Rejected: on a filesystem backend it turns an O(1) `stat` into a listing, and it loses the ability to distinguish an empty directory that really exists from one that does not. |
| Fix `key_prefix()` in a separate issue | Considered seriously — it is arguably not a slash problem. **Gate decision Q2: keep it here**, in its own commit, because it is three lines in the same file, found while reproducing this issue, and `store_factory.rs`'s `opendal03` already defers its assertion to this design. |
| Leave the commented-out synchronous block | Considered — it keeps the correctness diff small. **Gate decision Q3: delete it**, as a separate pure-deletion commit, so the issue closes with all four `//TODO: create_dir` citations resolved. |
| Keep the directory fallback private to `AsyncOpenDALStore` | **Rejected at the gate.** It was the first draft's plan. Four stores already reimplement directory derivation and `liquers-web`'s HTTP-backed stores face the same problem, so a fifth private solution would have been the fifth mistake, not the fix. §3a moves the mechanism to `liquers-core`. |
| Give `AsyncOpenDALStore` a `DirectoryIndex` too | Rejected: the backend is authoritative and may be written by another process, so a write-side index goes stale and rebuilding it means listing the whole bucket. OpenDAL asks the authority with a bounded listing and inherits only the *semantics* from core. This is why the core piece is an index **plus** shared semantics rather than an index alone. |
| Migrate `FetchStore` and `LocalStorageStore` to `DirectoryIndex` in this change | Rejected for now: both work today, both are wasm-only with their own Node/browser/Playwright loops, and the migration is cleanup rather than repair. Filed as follow-up under `CORE-DIRECTORY-INDEX-NOT-SHARED`, whose requirement — that the mechanism be *available* in core — is met. |
| Make `DirectoryIndex` a trait rather than a concrete type | Rejected: there is one derivation, and it is the same everywhere. A trait would let each store keep its own divergent implementation, which is the problem being fixed. |
| Build the shared `AsyncStore` conformance suite here | Rejected and filed: an `L` change to `liquers-core`'s test surface would swamp a P0 fix, and §3a gives the semantics an implementation to inherit rather than an enforcement. |
| `proptest` for the round-trip property | Rejected: the workspace has no property-testing dependency, and adding one for a single test is disproportionate in a build-size-constrained repository. A hand-written table of ~20 adversarial keys covers the same ground and is deterministic. |

## API and Backward Compatibility

- **No public signature is changed or removed.** `key_to_path`, `key_to_path_metadata` and
  `path_to_key` keep their types. `key_to_path` gains one refusal (a filename ending in the metadata
  suffix), which `is_supported` already refused. `make_sub_dirs` was private. The deleted
  synchronous block is commented out and therefore not API at all.
- **`liquers-core` gains public API**: the `store_dir_index` module and `DirectoryIndex`. Purely
  additive — nothing is removed, so no downstream crate breaks by compilation.
- **Two `AsyncStore` trait defaults change behaviour** (`contains`, `get_metadata`). Both are
  *widenings* of a permissive default, and neither adds a required method, so every existing
  implementation still compiles. An implementation that overrides them is unaffected; one that
  inherits them gets the semantics it should have had. `liquers-py` wraps the sync `Store`, not
  `AsyncStore`, so the bindings are untouched — checked against `liquers-py/src/store.rs`.
- `directory_metadata_includes_children` is a new default method returning `true`, so the existing
  recursive-walk behaviour is what an unmodified store keeps.
- **Two behavioural changes with reach**, both corrections: `removedir` stops deleting siblings, and
  `key_prefix()` changes router aggregation (Q2).
- **On-disk layout unchanged.** `PathMap::data` produces exactly `key.as_absolute()?.encode()`, as
  today, and deleting `make_sub_dirs` removes calls that never wrote anything. A store written by
  the current code reads identically after the change — the property the round-trip test pins down.

## Reuse

`Key::as_absolute`, `Key::encode`, `Key::parent`, `Key::prefix_of_size`, `Key::filename` and
`liquers_core::parse::parse_key` are all reused. The `is_dir`/`contains` fallback deliberately
mirrors `AsyncMemoryStore` rather than inventing a third semantics; the natural home for sharing it
is a default method on `AsyncStore`, which is the conformance-suite change filed separately.

## Risk Analysis

| Assessment | Record |
|---|---|
| **Files** | **Two crates.** `liquers-store`: `opendal_store.rs` (implementation and colocated tests) and one `#[cfg]` line in `store_factory.rs` — ~150 added, ~60 changed, ~225 deleted (200 of them the dead synchronous block). `liquers-core`: a new `store_dir_index.rs` (~200 lines with its tests), one `lib.rs` line, two trait defaults in `store.rs`, and `AsyncMemoryStore` re-pointed at the extracted index (~90 lines moved out). Specs: `specs/README.md`, four issue files, `phase5-documentation.md`, the new `STORE_SEMANTICS.md`, `specs/index.csv`. No generated or configuration files. |
| **Impact area** | `store/backends`. Downstream: `AsyncStoreRouter` routing, `is_dir` and cross-store `listdir`; every `-R/` and `-R-dir/` query against an OpenDAL store; `liquers-axum` store endpoints including `DELETE /api/store/removedir/{*key}`; `liquers-lib/examples/ui_query_console_app.rs`. |
| **Module/crate reach** | **Cross-crate, which is what raises this from `M` to `L`.** `liquers-core` gains a module and two changed trait defaults; `liquers-store` consumes them; `liquers-web` and `liquers-axum` inherit the trait defaults without being edited; `liquers-py` wraps the sync `Store` and is untouched. Separately, the `key_prefix()` change alters `AsyncStoreRouter` behaviour without editing it. |
| **Existing-test breakage** | Estimated **2-4 in `liquers-store`**, plus a **core surface that must not move at all**. In `liquers-core`: `AsyncMemoryStore`'s existing tests must pass **unchanged** — they are the proof the `DirectoryIndex` extraction is faithful, and any edit to them is a signal the extraction changed behaviour; `keyabs17_trait_defaults_refuse_relative_keys` (`store.rs:2355`) must pass unchanged through the new `contains` fallback (checked against its body: `contains(ok) == false` still holds because `is_dir`'s default is `Ok(false)`). In `liquers-web`: `FetchStore` and `LocalStorageStore` override both changed defaults, so neither is reached; the Node conformance loop is run to confirm rather than to assume. In `liquers-store`, all in `opendal_store.rs`'s own test module. `test_opendal_subdir` (`:663`) asserts `keys().len() == 3` and carries a commented-out block; both change under §1 and §3. `test_opendal_dir` (`:620`) asserts exact counts from `keys()` and `listdir()` at the root, where no trailing-slash change applies, but the counts are tight enough to re-check. `test_async_opendal_store_metadata` (`:595`) constructs an unprefixed store and should be unaffected. `keyabs16_opendal_store_refuses_relative_keys` (`:540`) must stay green **unchanged**. `store_factory.rs`'s `opendal03` gains an assertion rather than losing one. No test outside these two files constructs an `AsyncOpenDALStore`. |
| **New validation** | (a) **Sibling-safety**, the P0 guard: on memory *and* fs, with `sub/` and `subway/` both populated, `removedir("sub")` leaves `subway/b.txt` readable, and `listdir_keys_deep("sub")` and `keys()` return nothing from `subway/`. (b) Round-trip property over ~20 hand-written keys: single and multi-segment, dots inside names, unicode, the root key, and a name ending in `.__metadata__` asserted to be *refused*. (c) Regression test reproducing Phase 1's filesystem output for `sub/deeper/foo.txt`, asserting rather than printing. (d) Memory-backend directory test: `is_dir`, `contains`, `get_metadata`, `get_asset_info` all agree with `listdir`, and `is_dir` on an absent key is `Ok(false)` — the uncommented `test_opendal_subdir`. (e) Prefixed-store test: `prefix: data` reports `key_prefix() == data` and `keys()` stays within it; plus the assertion re-enabled in `opendal03`. (f) `AsyncStoreRouter` test with a prefixed OpenDAL store and a second store, asserting keys route to the right one. (g) `test_opendal_localfs` asserts. (h) **`DirectoryIndex` unit tests in `liquers-core`**: `edges_for_key` over a corpus (root, single segment, deep, unicode); `from_keys` and incremental `insert_key` produce identical indexes for the same key set; refcounting — two keys under one directory, removing one leaves the directory, removing both retires it; an explicit empty directory survives having no children and disappears on `remove_directory`; `children` is sorted and deduplicated. (i) A test that the two changed trait defaults behave as documented on a minimal store. Commands: `cargo test -p liquers-core --lib`, `cargo test -p liquers-store`, `cargo test -p liquers-lib --lib --tests`, `bash scripts/check-build-matrix.sh`, and after `cargo clean`, `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`. |
| **Behavioural risk** | *Data*: the change **prevents** data loss; the only way it could cause any is a `PathMap::directory` producing a shorter path than intended, which (a) covers on both backends. *Compatibility*: `key_prefix()` changes multi-store routing; `removedir` stops destroying siblings, with no caller relying on that. *Persistence*: on-disk paths unchanged by construction and asserted by (b); no migration. *Concurrency*: no shared mutable state is added; `PathMap` is stateless. *Performance*: one extra listing per `is_dir` **only** on the path that currently errors; `stat` still short-circuits on backends with directories; deleting `make_sub_dirs` removes N failed round-trips per write. *Security*: the key-absoluteness guard is preserved and enforced in one place instead of three; removing the prefix-delete closes a path where a key could destroy data outside its own subtree. *Error paths*: as listed under Error Handling. |
| **Recovery** | Six independent commits, ordered so the urgent part does not depend on the broad part. Commits 1-2 (`PathMap` and the trailing slash; `key_prefix()`) are the P0 and touch `liquers-store` only — they can ship, and revert, with no reference to the core work. Commit 3 adds a `liquers-core` module that nothing yet uses, so it is inert on its own; commit 4 is OpenDAL adopting it; commits 5-6 are deletions and hygiene. `key_prefix()` is a one-line revert if a routing regression appears in the field. The trait-default change is the one piece whose revert reaches other crates, and it is deliberately the smallest part of commit 3. The dead-block deletion is recoverable from git history. Nothing is persisted, so no revert needs a migration. |
| **Certainty** | High on the mechanism: every claim was executed against both backends, not inferred, and both fixes were verified at the operator level before being proposed. All four gate questions are answered, and the fifth direction (core placement) is grounded in four implementations read at `HEAD`. Lower on the core extraction, which is *designed* but not probed: that `AsyncMemoryStore`'s behaviour survives being re-pointed at an extracted index is an argument from the code, and the existing tests passing unchanged is the check that settles it — a Phase 3 obligation, not a Phase 2 claim. Unverified: the trailing-slash behaviour of `list_with`/`remove_all` on a *remote* object store (S3, GCS) — probed on `memory` and `fs`, the two shapes available offline; OpenDAL's prefix-versus-directory semantics are backend-independent by design, and the risk of a remote backend differing is low and in the safe direction (a narrower scope than today's). |

## Review Record

*Against Phase 1:* every acceptance criterion has a named change and a named test. Criterion 1
(sibling safety) maps to §1 and validation (a); 2 to §1 and (b); 3 to §2, (e) and (f); 4 to §3 and
(d); 5 to §5 and §6; 6 to (c). The non-goals are respected — the FIXME is deleted rather than
honoured, `path_map.rs` is explicitly *not* created because Phase 1 said the deliverable is "one
place", not "one file", and the conformance suite is filed rather than built. Phase 1's
Documentation Intent is made exact above, including the open question it left (the reference's
path), now settled as `specs/reference/STORE_SEMANTICS.md`.

*Against the codebase:* the four existing directory-derivation implementations were read in full
before §3a proposed replacing them — `AsyncMemoryStore::dir_index` and its refcounting
(`store.rs:580`, `:592-664`), the sync `MemoryStore`'s index-free scan (`:1607`),
`FetchStore::directory_index` (`store/fetch.rs:130`) and `LocalStorageStore::index_key` with its
`explicit_dirs` (`store/local_storage.rs:353`, `:98`). `explicit` is in `DirectoryIndex` because
`LocalStorageStore` proved it necessary, not because it seemed prudent. `scc`'s presence as an
unconditional `liquers-core` dependency, and therefore its availability on wasm32, was checked in
`liquers-core/Cargo.toml:66`. Every other line reference was read at `HEAD` on 2026-09-02, after
`store-factories-in-core` merged; `store_builder.rs` no longer exists and the references that named
it are re-resolved to `store_factory.rs`. The claim that ordinary routing already checks the real
prefix was traced through `store.rs:1921` and `opendal_store.rs:514-520`. `AsyncMemoryStore`'s
`is_dir` was read before proposing the same shape, and `AsyncFileStore`'s and `AsyncMemoryStore`'s
`removedir` were read before calling the OpenDAL one a divergence. OpenDAL's `create_dir`
validation and the `fs`/`memory`/`s3` service capabilities were read in the vendored crate source,
not assumed.

*Rust review (rust-best-practices):* no `unwrap`/`expect` outside tests — this change *removes* the
two that exist; no `println!`; errors go through existing typed constructors, never `Error::new`;
no default match arm over our own enum, and the one unavoidable wildcard is over a foreign
non-exhaustive enum and is called out; `PathMap` is a stateless unit type, so nothing is cloned or
locked that is not cloned today; `async` only where a call is awaited; no blocking I/O in an async
context; no new trait, and no existing trait modified.

*Risk understatement check:* the existing-test estimate is **2-4**, which exceeds the automatic
clearance limit of three on its own; the change is P0; it now spans two crates and changes two
`AsyncStore` trait defaults that three further crates inherit; and it carries two behavioural
changes of external reach. The complexity recorded on `STORE-OPENDAL-SLASH-HANDLING` is `M`, which
was right for the OpenDAL-only scope and is **no longer right for the whole**; the cross-crate work
is carried by `CORE-DIRECTORY-INDEX-NOT-SHARED` at `L`, which is why it was filed rather than
absorbed. This work does not clear a gate automatically and does not claim to.

*On sequencing:* the one structural risk this revision introduces is that a P0 data-loss fix now
sits in the same design as an `L` refactor of shared infrastructure. The commit order (P0 first,
depending on nothing in `liquers-core`) is the mitigation, and Phase 4 should be free to ship those
two commits ahead of the rest if the core work needs another round.

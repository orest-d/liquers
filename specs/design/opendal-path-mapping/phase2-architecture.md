# Phase 2 — Solution and architecture

Based on `HEAD` of `liquers-store/src/opendal_store.rs` (736 lines) and `liquers-core/src/store.rs`,
read rather than remembered, and re-resolved on 2026-09-02 after
[`design/store-factories-in-core/`](../store-factories-in-core/) merged (`store_builder.rs` is gone;
`create_opendal_store` is now `OpendalStoreFactory::create`, `liquers-store/src/store_factory.rs:170`).
Nothing here is implemented.

> **Revised 2026-09-02.** The first draft addressed three defects. The second reproduction recorded
> in Phase 1 added two — one of them destructive — and disproved this document's own claim that
> `make_sub_dirs` satisfies the `//TODO: create_dir` markers. §1 is rewritten around a single
> insight that now covers five of the six defects: **a directory path needs a trailing `/`, and
> three call sites do not add one.**

## Chosen solution

Six changes to `AsyncOpenDALStore`, plus test work. All of it lands in one file and its colocated
tests, except one `#[cfg]` line in `store_factory.rs`.

### 1. One path mapping, in one impl block — including the directory form

The trailing slash is not a detail of two methods; it is the difference between addressing a
directory and addressing a *prefix*. Making it the mapping's business is what fixes defects 1 and 2
rather than patching two call sites and waiting for the third.

```rust
/// The one place that maps a `Key` onto a backend path and back.
///
/// A store key is absolute (`liquers_core::store`), so every fallible entry point starts with
/// `Key::as_absolute`. A data path is the key's `encode()` form. A **directory path additionally
/// carries a trailing `/`**, which OpenDAL requires: without it `list`, `remove_all` and
/// `create_dir` treat the path as a prefix or a file, which is how `removedir("sub")` came to
/// delete `subway/`.
struct PathMap;

impl PathMap {
    const METADATA: &'static str = ".__metadata__";

    fn data(key: &Key) -> Result<String, Error>;      // "sub/foo.txt"
    fn metadata(key: &Key) -> Result<String, Error>;  // "sub/foo.txt.__metadata__"
    fn directory(key: &Key) -> Result<String, Error>; // "sub/" — the root key maps to ""
    fn decode(path: &str) -> Result<DecodedPath, Error>;
}

/// What a backend path denotes. Explicit, so a caller cannot forget that a listing yields
/// metadata sidecars and directory entries alongside data entries.
enum DecodedPath {
    Data(Key),
    Metadata(Key),   // the key of the data it describes
    Directory(Key),
}
```

`key_to_path`, `key_to_path_metadata` and `path_to_key` stay `pub` (they are `pub` today at `:238`,
`:248`, `:241` and may have external callers) and become one-line delegations, with `path_to_key`
mapping every `DecodedPath` variant to its `Key`.

**Every call site that names a directory uses `PathMap::directory`.** That is the whole fix for
defects 1 and 2:

| Call site | Line | Today | After |
|---|---|---|---|
| `removedir` | `:408` | `remove_all("sub")` — prefix delete, **destroys `subway/`** | `remove_all("sub/")` |
| `listdir_keys_deep` | `:481` | `list_with("sub").recursive(true)` — **lists `subway/…`** | `list_with("sub/")` |
| `listdir` | `:452` | inline `trim_end_matches('/') + "/"` | `PathMap::directory` |
| `makedir` | `:499` | inline `format!("{}/", …)` | `PathMap::directory` |
| `make_sub_dirs` | `:279` | `create_dir("sub")` — **always fails**, error discarded | see §5 |

Verified against the operator before proposing it (Phase 1 records the raw output): with the
trailing slash, a recursive `list` of `sub/` returns only `sub/…` on both the memory and filesystem
backends, and `remove_all("sub/")` leaves `subway/b.txt` in place on both.

The decode order must be got right and asserted: strip the trailing `/` **before** stripping the
metadata suffix, and strip the metadata suffix only from the final segment, only once. Today's
`path.trim_matches('/').trim_end_matches(Self::METADATA)` (`:242-243`) strips the suffix
*repeatedly*, so `x.__metadata__.__metadata__` decodes to `x`; no reachable path produces that,
but the corpus pins the single-strip rule down.

**Suffix-ending keys are excluded, not round-tripped.** `PathMap::data` for the key
`foo.__metadata__` and `PathMap::metadata` for the key `foo` produce the *same* path, so no decoder
can be injective over both while preserving the on-disk layout. `is_supported` (`:514-520`) already
refuses a key whose filename ends in the suffix, so such a key never reaches this store — but
`key_to_path` accepts it today (confirmed: it returns `Ok("a.__metadata__")`), so the rule lives in
one method and is absent from another. `PathMap::data` and `PathMap::metadata` refuse it too, with
`Error::key_not_supported`. An unambiguous encoding (escaping the suffix) would change the on-disk
layout and is out of scope.

**Decoding is lenient on the way in, strict on the way out.** `PathMap::decode` is applied to paths
the *backend* returns, which nothing in Liquers necessarily wrote — a stray `orphan.__metadata__`
with no data file is already reported as key `orphan` today. `listdir` and `listdir_keys_deep`
therefore **skip** an entry they cannot decode rather than failing the whole listing; only
`PathMap::data` and `PathMap::metadata`, which encode a key the caller supplied, return an error.

### 2. `key_prefix()` returns the configured prefix

```rust
fn key_prefix(&self) -> Key {
    self.prefix.clone()
}
```

Matching `AsyncFileStore` (`liquers-core/src/store.rs:1022`) and `FileStore` (`:1310`). The prefix
convention in this codebase is that the prefix is part of the path under the backend root —
`FileStore::key_to_path` pushes the whole key, prefix included, onto `self.path` — so `key_to_path`
needs no change: only the *advertised* prefix was wrong.

Consequences, all intended: `keys()` (`:434`) enumerates from the prefix instead of the backend
root, so it stops reporting the root key `""`, which is outside the store's own prefix;
`AsyncStoreRouter::is_dir` (`store.rs:2053`) and `listdir` (`:2080-2097`) stop offering this store
every key; `store_name()` identifies the store instead of printing `" OpenDAL Store"`.

### 3. Directory keys on backends with no directory objects

`is_dir` (`:427`) currently propagates the backend's `stat` failure — so an absent key yields `Err`
where every other store yields `Ok(false)`. Replace with: stat first, and when the backend reports
the path absent, fall back to a bounded listing of the directory path.

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

`contains` (`:414`) then gains the same fallback `AsyncMemoryStore` has — data, else metadata, else
`is_dir` (`store.rs:810-820`).

**`get_metadata` needs the same fallback.** `AsyncOpenDALStore` *overrides*
`AsyncStore::get_metadata` (`:318-357`) and never calls `is_dir`: it checks the metadata sidecar,
then `op.exists(data_path)`, then `op.stat` to decide `is_dir()`, and otherwise returns
`KeyNotFound`. On a backend with no directory object both `exists` calls are false, so
`get_metadata("sub")` fails — and `AsyncStore::get_asset_info` (`store.rs:407`) starts with
`self.get_metadata(key).await?`, so it fails too. Fixing `is_dir` alone does **not** satisfy
acceptance criterion 4. The `KeyNotFound` branch must consult `has_children` and, when it reports
children, return `Metadata::MetadataRecord(self.default_metadata(key, true))` — the same value the
`stat().is_dir()` branch already returns.

**Why a listing and not `create_dir`.** Making `make_sub_dirs` create real directory markers would
not work: OpenDAL's `memory` and `s3` services do not implement `create_dir` at all (only `fs`
does, `services/fs/backend.rs:193`), so on an object store there is nothing to create. Writing a
zero-byte `sub/` object by hand would change the on-disk layout. Synthesising from the listing is
what both `liquers-core` memory stores do, it is the only definition that works uniformly, and it
makes `listdir` and `is_dir` agree by construction — which is exactly the inconsistency Phase 1
found.

**Cost.** One extra listing call, only on the path that today returns an error, and asking for one
page. On a backend that does have directories the `stat` succeeds and nothing changes.

### 4. `removedir`, beyond the trailing slash

The trailing slash (§1) fixes the sibling deletion. Two smaller things go with it:

- `remove_all` on a path OpenDAL reports as absent is `Ok(())` — removing a non-existent directory
  is a no-op, matching `AsyncFileStore` (`store.rs:1171-1183`). No change needed; the test asserts
  it so a future rewrite cannot silently turn it into an error.
- The doc comment *"Files are not removed recursively"* (`:405-407`) is false — `remove_all` is
  recursive, and so are the other two async stores. Correct the comment to describe recursive
  removal scoped to the directory. Behaviour is not changed to match the comment: a non-recursive
  `removedir` is nobody's contract and would break `AsyncStoreRouter`'s delegation.

### 5. `make_sub_dirs`: delete it

It has never worked (Phase 1's evidence: `create_dir` without a trailing slash is rejected by
OpenDAL unconditionally, and `let _ignore` at `:281` discards the error). The options are to fix it
with `PathMap::directory` or to remove it.

**Delete it, with its two call sites in `set` (`:362`) and `set_metadata` (`:379`) and the two
`//TODO: create_dir` markers above them.** Deleting a no-op cannot change behaviour, and the
alternative would: on `fs` it would create directories the writer already creates, on `s3` and
`memory` `create_dir` is unimplemented, and on a backend that does implement it the store would
start writing directory markers it never wrote before — a layout change, which §"API and backward
compatibility" forbids. Explicit empty-directory creation stays available through `makedir`, which
adds the slash and works.

This also removes one of the two `unwrap()`s (`:279`). The other, in `listdir_keys_deep` (`:488`),
becomes a `filter_map` that skips a key whose prefix cannot be taken — unreachable, but `unwrap()`
in library code is forbidden by `CLAUDE.md` regardless.

### 6. Comment and warning hygiene, and two folded-in issues

- Delete the stale `FIXME` at `:340` and replace it with what is true: *"Directory children are
  deliberately not populated here — `listdir_asset_info` walks the whole subtree."*
- Fix the two warnings this file emits at `HEAD`, both in lines being touched: unused `Store`
  import (`:8`), unnecessary `mut` (`:339`).
- Leave the commented-out synchronous block (`:16-218`) alone unless Q3 says otherwise.
- **`OPENDAL-LOCALFS-TEST-SILENT-ON-WRONG-VALUE-TYPE`** (P3, S): `test_opendal_localfs` (`:705`)
  `eprintln!`s in both branches, so it reports `ok` whether or not `-R-dir/src` returns
  `Value::AssetInfo`. Make the `else` branch `panic!`, and assert the computed `names` set contains
  `"opendal_store.rs"`. Folded in because the test module is being rewritten and this test is the
  only end-to-end guard on `get_asset_info`, which §3 changes.
- **`STORE-OPENDAL-WITHOUT-ASYNC-STORE-BROKEN`** (P3, S): `store_factory.rs` imports
  `AsyncOpenDALStore` under `#[cfg(feature = "opendal")]` while the type is gated on
  `#[cfg(feature = "async_store")]`, so `--no-default-features --features opendal` fails to build.
  Take that issue's option 1 — gate the import and its uses on
  `#[cfg(all(feature = "opendal", feature = "async_store"))]` — one `#[cfg]` line, and it lets
  `scripts/check-build-matrix.sh` spell the row the obvious way.

## Rejected alternatives

| Option | Verdict |
|---|---|
| Patch the trailing slash at `removedir` and `listdir_keys_deep` only | Rejected: that is the third time this bug would be fixed one call site at a time (`listdir` and `makedir` already carry inline slash arithmetic, and the two defects found on 2026-09-02 are the sites that were missed). `PathMap::directory` makes the omission impossible to repeat. |
| A separate `liquers-store/src/path_map.rs`, as WP-5 proposed | Rejected: ~70 lines with one caller. "One place" is satisfied by one private type in the file that uses it; a module adds a public surface nobody asked for. Revisit if a second backend needs the same rules. |
| Fix `make_sub_dirs` instead of deleting it | Rejected: see §5 — it would start writing directory markers on backends that support `create_dir`, changing the on-disk layout, to no benefit. |
| Materialize directory markers so directory keys become addressable | Rejected: `memory` and `s3` do not implement `create_dir`, so it does not solve defect 4 where defect 4 exists. |
| Honour the FIXME and populate `children` in directory metadata | Rejected: a full recursive subtree walk per directory read. Whether the `AsyncStore` default that does this (`store.rs:396-403`) is right is a separate question. |
| Make `is_dir` always synthesize from a listing, ignoring `stat` | Rejected: on a filesystem backend it turns an O(1) `stat` into a listing, and it loses the ability to distinguish an empty directory that really exists from one that does not. |
| Fix `key_prefix()` in a separate issue | Considered seriously — it is arguably not a slash problem. Kept here because it is three lines, in the same file, found while reproducing this issue, and it is the change with the most behavioural reach, so it is called out at the gate (Q2) rather than buried. |
| A shared `AsyncStore` behavioural conformance suite | Rejected **for this change**, and filed as an issue: four of the six defects are divergences from what `AsyncMemoryStore` and `AsyncFileStore` already do, and one suite run against all three would have caught them. It is an `L` change to `liquers-core`'s test surface and would swamp a P0 fix. |
| `proptest` for the round-trip property | Rejected: the workspace has no property-testing dependency, and adding one for a single test is disproportionate in a build-size-constrained repository. A hand-written table of ~20 adversarial keys covers the same ground and is deterministic. |

## Exact symbols involved

**Changed** — `liquers-store/src/opendal_store.rs` unless noted:

| Symbol | Line | Change |
|---|---|---|
| `PathMap`, `DecodedPath` | new | the one mapping, with a directory form |
| `AsyncOpenDALStore::key_to_path` | `:238` | delegate to `PathMap::data` |
| `key_to_path_metadata` | `:248` | delegate to `PathMap::metadata` |
| `path_to_key` | `:241` | delegate to `PathMap::decode` |
| `key_prefix` | `:296` | return `self.prefix.clone()` |
| `has_children` | new | bounded listing of the directory path |
| `get_metadata` | `:318` | `KeyNotFound` branch consults `has_children`; stale `FIXME` (`:340`) replaced; stray `mut` (`:339`) dropped |
| `set` / `set_metadata` | `:361`, `:378` | drop the `make_sub_dirs` call and the `//TODO: create_dir` above it |
| `removedir` | `:408` | `PathMap::directory`; doc comment corrected |
| `contains` | `:414` | add the `is_dir` fallback |
| `is_dir` | `:427` | `NotFound` falls back to `has_children` |
| `listdir` | `:452` | `PathMap::directory`; decode entries through `DecodedPath`, skipping undecodable ones |
| `listdir_keys_deep` | `:481` | `PathMap::directory`; `unwrap()` at `:488` removed |
| `makedir` | `:499` | `PathMap::directory` |
| `make_sub_dirs` | `:277` | **deleted** (never worked; `unwrap()` at `:279` goes with it) |
| `use … store::{AsyncStore, Store}` | `:8` | drop the unused `Store` import |
| `test_opendal_localfs` | `:705` | assert instead of `eprintln!` |
| `use crate::opendal_store::AsyncOpenDALStore` | `store_factory.rs:22` | gate on `all(opendal, async_store)` |

**Read, unchanged** — `AsyncStore` and its defaults (`liquers-core/src/store.rs:329-545`),
`AsyncStoreRouter` (`:1909-2160`), `AsyncMemoryStore` (`:578-900`), `AsyncFileStore` (`:904-1265`),
`OpendalStoreFactory::create` (`liquers-store/src/store_factory.rs:170-195`).

**Not touched** — the commented-out synchronous `OpenDALStore` block (`:16-218`), `liquers-core`.

## Data ownership, errors, sync/async

- `PathMap` is a unit type with associated functions: no state, no lifetimes, nothing to own.
  `DecodedPath` owns its `Key` (`Key` is already an owned type).
- Errors stay `liquers_core::error::Error` via the existing `map_read_error` / `map_write_error`
  helpers (`:251`, `:264`), `Error::key_not_found` and `Error::key_not_supported`; no new error
  type, no `Error::new`. `PathMap::data` and friends keep returning `Result` because
  `Key::as_absolute` is fallible — this is the `STORE-KEY-GUARD` rule and must not regress
  (`keyabs16_opendal_store_refuses_relative_keys`, `:540`, asserts `ErrorType::KeyNotAbsolute` on
  seven methods and on `key_to_path` directly).
- **No `unwrap()` or `expect()` remains outside tests**: `:279` goes with `make_sub_dirs`, `:488`
  becomes a `filter_map`.
- Matching on `DecodedPath` is exhaustive — no `_` arm. `opendal::ErrorKind` is a foreign
  non-exhaustive enum, so a catch-all there is unavoidable and is the one permitted exception;
  it is written as `Err(e) if e.kind() == ErrorKind::NotFound` plus `Err(e)`, two arms rather than
  a wildcard over our own type.
- Everything stays `async`; no blocking call is introduced.

## API and backward compatibility

- No public signature changes. `key_to_path`, `key_to_path_metadata` and `path_to_key` keep their
  types. `key_to_path` gains one refusal (a filename ending in the metadata suffix), which
  `is_supported` already refused.
- **Two behavioural changes with reach**, both corrections: `removedir` stops deleting siblings
  (Phase 1 defect 1), and `key_prefix()` changes router aggregation (Q2).
- On-disk layout is unchanged: `PathMap::data` produces exactly `key.as_absolute()?.encode()`, as
  today, and deleting `make_sub_dirs` removes calls that never wrote anything. A store written by
  the current code reads identically after the change — this is the property the round-trip test
  pins down.

## Reuse

`Key::as_absolute`, `Key::encode`, `Key::parent`, `Key::prefix_of_size`, `Key::filename` and
`liquers_core::parse::parse_key` are all reused. The `is_dir`/`contains` fallback deliberately
mirrors `AsyncMemoryStore` rather than inventing a third semantics; if the two ever need to be
shared, the natural home is a default method on `AsyncStore`, which is the conformance-suite change
filed separately.

## Related open issues

- `OPENDAL-LOCALFS-TEST-SILENT-ON-WRONG-VALUE-TYPE` (P3, `draft`) — **folded in**, §6.
- `STORE-OPENDAL-WITHOUT-ASYNC-STORE-BROKEN` (P3, `draft`) — **folded in**, §6.
- `STORE-OPENDAL-LIST-OPTION-MISPARSED` (P2, `draft`, design `opendal-list-option-config`) — in
  `store_factory.rs`, not this file. Not folded in; no ordering constraint. The only overlap is the
  one `#[cfg]` line §6 touches, in a different import block.
- `STORE-OPENDAL-ARGUMENTS-NOT-DERIVED` (P3, feature, design `store-factories-in-core`) — that
  design is `complete`; the remaining feature work does not touch `opendal_store.rs`.
- `STORE-ABSOLUTE-KEY-NOT-TYPE-ENFORCED` — the reason `PathMap` entry points are fallible.
- `STORE-CONFIG-IN-CORE` / `design/store-factories-in-core/` — **merged.** Its `store_factory.rs`
  already carries a comment deferring to this design: `opendal03` documents that it *"deliberately
  does not assert `key_prefix()`"* because the assertion fails today. That assertion is enabled by
  §2 and belongs in this change's validation.
- New, to be filed with this work: a shared `AsyncStore` behavioural conformance suite (see
  Rejected alternatives), and the undocumented directory/deletion contract noted in Phase 1's
  documentation assessment.

## Risk analysis

| Assessment | Record |
|---|---|
| **Files** | 1 source file (`liquers-store/src/opendal_store.rs`) for implementation and colocated tests, plus one `#[cfg]` line in `store_factory.rs`; ~150 lines added, ~60 changed, ~25 deleted. Specs: `specs/README.md` §Stores, the issue file (`priority`, an update section, `status` at Phase 5), two folded-in issue files closed, `specs/index.csv` regenerated. No generated or configuration files. |
| **Impact area** | `store/backends`. Downstream: `AsyncStoreRouter` routing, `is_dir` and cross-store `listdir`; every `-R/` and `-R-dir/` query against an OpenDAL store; `liquers-axum` store endpoints including `DELETE /api/store/removedir/{*key}`; `liquers-lib/examples/ui_query_console_app.rs`. `liquers-web` no longer depends on this crate. |
| **Module/crate reach** | Two modules in one crate. **But** the `key_prefix()` change alters the behaviour of `AsyncStoreRouter`, which lives in `liquers-core`, so the *impact* crosses a crate boundary even though the edit does not. |
| **Existing-test breakage** | Estimated **2-4**, all in `opendal_store.rs`'s own test module. `test_opendal_subdir` (`:663`) asserts `keys().len() == 3` and carries a commented-out block; both change under §1 and §3. `test_opendal_dir` (`:620`) asserts exact counts from `keys()` and `listdir()` at the root, where no trailing-slash change applies, but the counts are tight enough to be worth re-checking. `test_async_opendal_store_metadata` (`:595`) constructs an unprefixed store and should be unaffected. `keyabs16_opendal_store_refuses_relative_keys` (`:540`) must stay green **unchanged** — it is the guard on the fallible-mapping rule. `store_factory.rs`'s `opendal03` gains an assertion rather than losing one. No test outside these two files constructs an `AsyncOpenDALStore`. |
| **New validation** | (a) **Sibling-safety**, the P0 guard: on memory *and* fs, with `sub/` and `subway/` both populated, `removedir("sub")` leaves `subway/b.txt` readable, and `listdir_keys_deep("sub")` and `keys()` return nothing from `subway/`. (b) Round-trip property over a hand-written corpus of ~20 keys: single and multi-segment, dots inside names, unicode, the root key, and a name ending in `.__metadata__` asserted to be *refused*. (c) Regression test reproducing Phase 1's filesystem output for `sub/deeper/foo.txt`, asserting rather than printing. (d) Memory-backend directory test: `is_dir`, `contains`, `get_metadata`, `get_asset_info` all agree with `listdir`, and `is_dir` on an absent key is `Ok(false)` — the uncommented `test_opendal_subdir`. (e) Prefixed-store test: a store with `prefix: data` reports `key_prefix() == data` and `keys()` stays within it; plus the `key_prefix()` assertion re-enabled in `store_factory.rs`'s `opendal03`. (f) An `AsyncStoreRouter` test with a prefixed OpenDAL store and a second store, asserting keys route to the right one. (g) `test_opendal_localfs` asserts. Commands: `cargo test -p liquers-store`, `cargo test -p liquers-core --lib`, `cargo test -p liquers-lib --lib --tests`, `bash scripts/check-build-matrix.sh` (including the `opendal`-without-`async_store` row §6 unblocks). |
| **Behavioural risk** | *Data*: the change **prevents** data loss; the only way it could cause any is a `PathMap::directory` that produces a shorter path than intended, which (a) covers on both backends. *Compatibility*: `key_prefix()` changes multi-store routing — quantified in Q2; `removedir` stops destroying siblings, with no caller relying on that. *Persistence*: on-disk paths unchanged by construction and asserted by (b); no migration. *Concurrency*: no shared mutable state is added; `PathMap` is stateless. *Performance*: one extra listing per `is_dir` **only** on the path that currently errors; `stat` still short-circuits on backends with directories; deleting `make_sub_dirs` removes N failed round-trips per write. *Security*: the key-absoluteness guard is preserved and enforced in one place instead of three; removing the prefix-delete closes a path where a key could destroy data outside its own subtree. *Error paths*: `is_dir` and `contains` return `Ok` where they returned `Err` for a directory whose children exist, and `is_dir` returns `Ok(false)` where it returned `Err` for an absent key; every other error is unchanged. |
| **Recovery** | The changes are independent commits and each reverts alone: the trailing-slash fix (§1), `key_prefix()` (§2), the directory fallback (§3), the `make_sub_dirs` deletion (§5), hygiene and folded-in issues (§6). `key_prefix()` in particular is a one-line revert if a routing regression appears in the field. Nothing is persisted, so revert needs no migration. |
| **Certainty** | High on the mechanism: every claim above was executed against both backends, not inferred, and both fixes were verified at the operator level before being proposed. Two decisions remain open (Q2, Q3). Unverified: the trailing-slash behaviour of `list_with`/`remove_all` on a *remote* object store (S3, GCS) — probed on `memory` and `fs`, the two shapes available offline; OpenDAL's prefix-versus-directory semantics are backend-independent by design, and the risk of a remote backend differing is low and in the safe direction (a narrower scope than today's). |

## Open questions for the gate

**Q1 — is the directory-key gap (Phase 1 defect 4) in scope?** **Answered: yes**, at the
2026-09-02 gate ("handle all opendal store related bugs").

**Q2 — the `key_prefix()` fix changes router behaviour.** Today a prefixed OpenDAL store advertises
the root prefix, so `AsyncStoreRouter::is_dir` (`store.rs:2053`) answers from it for *every* key,
and `listdir` (`:2080`) aggregates it into every listing. Ordinary `get`/`set` routing is already
correct, because `find_store` (`:1921`) also requires `is_supported`, which does check the real
prefix. So the blast radius is narrower than it first looks, but it is not zero, and it is larger
than the first draft of this document said — that draft named only `listdir`.
**Recommendation:** fix it, in its own commit, with the router test (f). Nothing in this repository
configures a prefixed OpenDAL store, so no in-tree behaviour changes either way.
**Alternative:** split it into its own issue so this one stays purely about paths.

**Q3 — dead code.** The commented-out synchronous `OpenDALStore` (`:16-218`, 200 lines) is 27% of
the file and cannot compile. It also holds two of the four `//TODO: create_dir` markers the issue
cites, so leaving it means the issue closes with two of its four citations untouched.
**Recommendation:** leave it, and say so in the Phase 5 summary. Deleting it is a separate,
uncontroversial cleanup that should not ride along inside a P0 correctness fix.

**Q4 — is the priority raise right?** `removedir` destroying a sibling directory is data loss,
which is the guide's own P0 criterion (§4.4), and it is reachable over HTTP. The issue has been
raised from P1 to P0 on that basis. If you would rather the raise waited, say so and it reverts to
P1 — it changes nothing about the work, only about what the queue says.

## Review record

*Against Phase 1:* every acceptance criterion has a named change and a named test. Criterion 1
(sibling safety) maps to §1 and validation (a); 2 to §1 and (b); 3 to §2, (e) and (f); 4 to §3 and
(d); 5 to §5 and §6; 6 to (c). The non-goals are respected — the FIXME is deleted rather than
honoured, `path_map.rs` is explicitly *not* created because Phase 1 said the deliverable is "one
place", not "one file", and the conformance suite is filed rather than built.

*Against the codebase:* every line reference was read at `HEAD` on 2026-09-02, after
`store-factories-in-core` merged; `store_builder.rs` no longer exists and the references that named
it have been re-resolved to `store_factory.rs`. The claim that ordinary routing already checks the
real prefix was traced through `store.rs:1921` and `opendal_store.rs:514-520`. `AsyncMemoryStore`'s
`is_dir` was read before proposing the same shape, and `AsyncFileStore`'s and `AsyncMemoryStore`'s
`removedir` were read before calling the OpenDAL one a divergence. The `#[cfg]` split
(`async_store`, `opendal`) was checked: every changed symbol is inside
`#[cfg(feature = "async_store")]`, and §6 is the one place a gate changes.

*Rust review:* no `unwrap`/`expect` outside tests — this change *removes* the two that exist; no
`println!`; errors go through the existing typed constructors; the one unavoidable wildcard is over
a foreign non-exhaustive enum and is called out; `PathMap` is a stateless unit type; every new
function is `async` only where it awaits.

*Risk understatement check:* the existing-test estimate is **2-4**, which exceeds the automatic
clearance limit of three on its own; the change is now P0 with two behavioural changes of external
reach; and Q2/Q3 are unresolved. This work cannot clear the gate automatically and does not claim
to.

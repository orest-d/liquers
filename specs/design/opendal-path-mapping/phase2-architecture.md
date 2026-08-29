# Phase 2 — Solution and architecture

Based on `HEAD` of `liquers-store/src/opendal_store.rs` (733 lines) and
`liquers-core/src/store.rs`, read rather than remembered. Nothing here is implemented.

## Chosen solution

Three changes to `AsyncOpenDALStore`, in the order they should be made, plus test work. All of it
lands in one file and its colocated tests.

### 1. One path mapping, in one impl block

Collect the mapping into a small private type inside `opendal_store.rs`, holding the
prefix-independent rules and nothing else:

```rust
/// The one place that maps a `Key` onto a backend path and back.
///
/// A store key is absolute (`liquers_core::store`), so every fallible entry point starts with
/// `Key::as_absolute`. A path is the key's `encode()` form; a directory path additionally carries
/// a trailing `/`, which OpenDAL requires for `list` and `create_dir` and refuses elsewhere.
struct PathMap;

impl PathMap {
    const METADATA: &'static str = ".__metadata__";

    fn data(key: &Key) -> Result<String, Error>;      // "sub/foo.txt"
    fn metadata(key: &Key) -> Result<String, Error>;  // "sub/foo.txt.__metadata__"
    fn directory(key: &Key) -> Result<String, Error>; // "sub/" — root maps to ""
    fn decode(path: &str) -> Result<DecodedPath, Error>;
}

/// What a backend path denotes. Explicit, so a caller cannot forget that a listing yields
/// metadata sidecars alongside data entries.
enum DecodedPath {
    Data(Key),
    Metadata(Key),   // the key of the data it describes
    Directory(Key),
}
```

`key_to_path`, `key_to_path_metadata` and `path_to_key` stay as public methods (they are `pub` today
at `:238`, `:248`, `:241` and may have external callers) and become one-line delegations, with
`path_to_key` mapping every `DecodedPath` variant to its `Key`. The trailing-slash arithmetic
currently inline in `listdir` (`:445`) and `makedir` (`:498`) moves to `PathMap::directory`.

The decode order is the part that must be got right and asserted: strip the trailing `/` **before**
stripping the metadata suffix, and strip the metadata suffix only from the final segment. Today's
`path.trim_matches('/').trim_end_matches(Self::METADATA)` (`:242-243`) is correct for the paths
OpenDAL currently returns.

**Suffix-ending keys are excluded, not round-tripped.** `PathMap::data` for the key
`foo.__metadata__` and `PathMap::metadata` for the key `foo` produce the *same* path, so no decoder
can be injective over both while preserving the on-disk layout. That is not a defect to repair
here: `is_supported` (`:514-520`) already refuses a key whose filename ends in the suffix, so such a
key never reaches this store. `PathMap::data` and `PathMap::metadata` therefore refuse it too, with
`Error::key_not_supported`, and the two rules become checkable in one place instead of living in
`is_supported` alone. An unambiguous encoding (escaping the suffix) would change the on-disk layout
and is out of scope.

### 2. `key_prefix()` returns the configured prefix

```rust
fn key_prefix(&self) -> Key {
    self.prefix.clone()
}
```

Matching `AsyncFileStore` (`liquers-core/src/store.rs:1035`) and `FileStore` (`:1323`). The prefix
convention in this codebase is that the prefix is part of the path under the backend root —
`FileStore::key_to_path` pushes the whole key, prefix included, onto `self.path` (`store.rs:1297-1303`)
— so `key_to_path` needs no change: only the *advertised* prefix was wrong.

Consequences, all intended: `AsyncStore::keys` (`store.rs:457`) enumerates from the prefix;
`AsyncStoreRouter` (`store.rs:1711`, `:1843`, `:1846`) routes and lists correctly; `store_name()`
identifies the store.

### 3. Directory keys on backends with no directory objects

`is_dir` (`:427`) currently propagates the backend's `stat` failure. Replace with: stat first, and
when the backend reports the path absent, fall back to a bounded listing —
`op.list_with(dir_path).limit(1)` — and report `true` when it yields anything.

```rust
async fn is_dir(&self, key: &Key) -> Result<bool, Error> {
    let path = self.key_to_path(key)?;
    match self.op.stat(&path).await {
        Ok(stat) => Ok(stat.is_dir()),
        Err(e) if e.kind() == opendal::ErrorKind::NotFound => self.has_children(key).await,
        Err(e) => Err(/* map_read_error */),
    }
}
```

`contains` (`:414`) then gains the same fallback `AsyncMemoryStore` has — data, else metadata, else
`is_dir` (`store.rs:1611-1618`).

**`get_metadata` needs the same fallback, and an earlier draft of this document wrongly said it did
not.** `AsyncOpenDALStore` *overrides* `AsyncStore::get_metadata` (`:317-350`) and never calls
`is_dir`: it checks the metadata sidecar, then `op.exists(data_path)`, then `op.stat` to decide
`is_dir()`, and otherwise returns `KeyNotFound`. On a backend with no directory object both
`exists` calls are false, so `get_metadata("sub")` fails — and `AsyncStore::get_asset_info`
(`store.rs:409-418`) starts with `self.get_metadata(key).await?`, so it fails too. Fixing `is_dir`
alone therefore does **not** satisfy acceptance criterion 3. The `KeyNotFound` branch must consult
the synthetic-directory check and, when it reports children, return
`Metadata::MetadataRecord(self.default_metadata(key, true))` — the same value the `stat().is_dir()`
branch already returns.

**Why a listing and not `create_dir`.** Making `make_sub_dirs` (`:277`) stop discarding errors would
not help: on S3 or the memory backend there is nothing to create. Synthesising from the listing is
what `AsyncMemoryStore` does, it is the only definition that works uniformly, and it makes
`listdir` and `is_dir` agree by construction — which is exactly the inconsistency Phase 1 found.

**Cost.** One extra listing call, only on the path that today returns an error, and bounded to one
entry. On a backend that does have directories the `stat` succeeds and nothing changes.

### 4. Comment hygiene

Delete the stale `FIXME` at `:335` and replace it with what is true: *"Directory children are
deliberately not populated here — `listdir_asset_info` walks the whole subtree."* Delete the two
live `//TODO: create_dir` at `:362` and `:379`, which `make_sub_dirs` already satisfies. Leave the
commented-out synchronous block alone; deleting dead code is a separate decision and this issue
should not make it silently.

Also fix the two compiler warnings this file emits at `HEAD`, both in the lines being touched: an
unused `Store` import (`:8`) and an unnecessary `mut` at `:339`.

## Rejected alternatives

| Option | Verdict |
|---|---|
| A separate `liquers-store/src/path_map.rs`, as WP-5 proposed | Rejected: ~60 lines with one caller. "One place" is satisfied by one private type in the file that uses it; a module adds a public surface nobody asked for. Revisit if a second backend needs the same rules. |
| Honour the FIXME and populate `children` in directory metadata | Rejected: it is a full recursive subtree walk per directory read, and the `AsyncStore` default (`store.rs:399-403`) already does it for stores that do not override `default_metadata`. Whether that default is right is a separate question. |
| Make `is_dir` always synthesize from a listing, ignoring `stat` | Rejected: on a filesystem backend it turns an O(1) `stat` into a listing, and it loses the ability to distinguish an empty directory that really exists from one that does not. |
| Fix `key_prefix()` in a separate issue | Considered seriously — it is arguably not a slash problem. Kept here because it is three lines, it is in the same file, and Phase 1 found it while reproducing this issue. It is the change with the most behavioural reach, so it is called out at the gate (Q2) rather than buried. |
| `proptest` for the round-trip property | Rejected: the workspace has no property-testing dependency, and adding one for a single test is disproportionate in a build-size-constrained repository. A hand-written table of ~20 adversarial keys covers the same ground and is deterministic. |

## Exact symbols involved

**Changed** — all in `liquers-store/src/opendal_store.rs`:

| Symbol | Line | Change |
|---|---|---|
| `AsyncOpenDALStore::key_to_path` | `:238` | delegate to `PathMap::data` |
| `AsyncOpenDALStore::key_to_path_metadata` | `:248` | delegate to `PathMap::metadata` |
| `AsyncOpenDALStore::path_to_key` | `:241` | delegate to `PathMap::decode` |
| `key_prefix` | `:296` | return `self.prefix.clone()` |
| `get_metadata` | `:317-350` | `KeyNotFound` branch consults the synthetic-directory check; stale comment removed |
| `is_dir` | `:427` | `NotFound` falls back to a bounded listing |
| `contains` | `:414` | add the `is_dir` fallback |
| `listdir` | `:445` | use `PathMap::directory`; decode entries through `DecodedPath` |
| `makedir` | `:498` | use `PathMap::directory` |
| `set` / `set_metadata` | `:362`, `:379` | delete the stale `//TODO: create_dir` |
| `make_sub_dirs` | `:277` | unchanged; its `let _ignore` is now documented as intentional |
| `use … store::{AsyncStore, Store}` | `:8` | drop the unused import |

**Read, unchanged** — `AsyncStore` and its defaults (`liquers-core/src/store.rs:329-500`),
`AsyncStoreRouter` (`:1700-1900`), `AsyncMemoryStore` (`:1520-1690`), `AsyncFileStore`
(`:1030-1275`), `create_opendal_store` (`liquers-store/src/store_builder.rs:188-202`).

**Not touched** — the commented-out synchronous `OpenDALStore` block (`:15-215`),
`liquers-store/src/config.rs`, `liquers-core`.

## Data ownership, errors, sync/async

- `PathMap` is a unit type with associated functions: no state, no lifetimes, nothing to own.
  `DecodedPath` owns its `Key` (`Key` is already an owned type).
- Errors stay `liquers_core::error::Error` via the existing `map_read_error` / `map_write_error`
  helpers (`:251`, `:264`) and `Error::key_not_found`; no new error type, no `Error::new`, no
  `unwrap`/`expect` in library code. `PathMap::data` and friends keep returning `Result` because
  `Key::as_absolute` is fallible — this is the `STORE-KEY-GUARD` rule and must not regress
  (`keyabs16_opendal_store_refuses_relative_keys`, `:540`, asserts `ErrorType::KeyNotAbsolute` on
  seven methods and on `key_to_path` directly).
- Matching on `DecodedPath` and on `opendal::ErrorKind` must be exhaustive where the enum is ours
  (`DecodedPath`) — no `_` arm. `opendal::ErrorKind` is a foreign non-exhaustive enum, so a
  catch-all there is unavoidable and is the one permitted exception; it is written as
  `Err(e) if e.kind() == ErrorKind::NotFound` plus `Err(e)`, which reads as two arms rather than a
  wildcard over our own type.
- Everything stays `async`; no blocking call is introduced. The added listing in `is_dir` is
  `await`ed like every other backend call.

## API and backward compatibility

- No public signature changes. `key_to_path`, `key_to_path_metadata` and `path_to_key` keep their
  types.
- **One behavioural change with reach:** `key_prefix()`. See Q2.
- On-disk layout is unchanged: `PathMap::data` produces exactly `key.as_absolute()?.encode()`, as
  today. A store written by the current code reads identically after the change — this is the
  property the round-trip test pins down.

## Reuse

`Key::as_absolute`, `Key::encode`, `Key::parent`, `Key::prefix_of_size`, `Key::filename` and
`liquers_core::parse::parse_key` are all reused. The `is_dir`/`contains` fallback deliberately
mirrors `AsyncMemoryStore` rather than inventing a third semantics; if the two ever need to be
shared, the natural home is a default method on `AsyncStore`, which is a larger change and not
this one.

## Related open issues

- `OPENDAL-LOCALFS-TEST-SILENT-ON-WRONG-VALUE-TYPE` (P3, `draft`) — `test_opendal_localfs`
  (`:702`) `eprintln!`s instead of asserting, so it would not catch a regression this change might
  cause. Not a blocker; worth fixing in the same file while Phase 3 is rewriting these tests, and
  it is listed as a candidate rather than assumed in scope.
- `STORE-ABSOLUTE-KEY-NOT-TYPE-ENFORCED` — the reason `PathMap` entry points are fallible.
- `CORE-STORE-OPENBIN-MISSING` — unaffected.
- `STORE-CONFIG-IN-CORE` — moves `StoreConfig` into `liquers-core`; it does not move
  `opendal_store.rs`, so there is no ordering constraint, but a merge conflict in
  `store_builder.rs` is possible if both land close together. This change does not edit that file.

## Risk analysis

| Assessment | Record |
|---|---|
| **Files** | 1 source file (`liquers-store/src/opendal_store.rs`) for both implementation and colocated tests; ~120 lines added, ~40 changed. Specs: `specs/README.md` §Stores (one corrected sentence, one relinked bullet), the issue file's `status:` at Phase 5, `specs/index.csv` regenerated. No generated or configuration files. |
| **Impact area** | `store/backends`. Downstream: `AsyncStoreRouter` routing and cross-store `listdir`; every `-R/` and `-R-dir/` query against an OpenDAL store; `liquers-axum` store endpoints; `liquers-lib/examples/ui_query_console_app.rs`. `liquers-web` builds `liquers-store` with `opendal` off and is unaffected. |
| **Module/crate reach** | One module in one crate. **But** the `key_prefix()` change alters behaviour of `AsyncStoreRouter`, which lives in `liquers-core` — so the *impact* crosses a crate boundary even though the edit does not. |
| **Existing-test breakage** | Estimated **2-4**, all in `opendal_store.rs`'s own test module: `test_opendal_dir` and `test_opendal_subdir` (`:620`, `:663`) assert exact counts from `keys()` and `listdir()`, and the `is_dir` fallback changes what the memory backend reports — `test_opendal_subdir`'s `keys().len() == 3` and its commented-out block are both expected to move. `test_async_opendal_store_metadata` (`:595`) constructs a store with `parse_key("")` and is not prefixed, so it should be unaffected. `keyabs16_opendal_store_refuses_relative_keys` (`:540`) must stay green unchanged — it is the guard on the fallible-mapping rule. No test outside this file constructs an `AsyncOpenDALStore` except `ui_query_console_app.rs`, which is an example, not a test. |
| **New validation** | (a) Round-trip property over a hand-written corpus of ~20 keys: single and multi-segment, dots inside names, unicode, a name ending in `.__metadata__`, the root key. (b) Regression test reproducing Phase 1's filesystem output for `sub/deeper/foo.txt`, asserting rather than printing. (c) Memory-backend directory test: `is_dir`, `contains`, `get_metadata`, `get_asset_info` all agree with `listdir` — the uncommented `test_opendal_subdir`. (d) Prefixed-store test: a store with `prefix: data` reports `key_prefix() == data` and `keys()` stays within it. (e) An `AsyncStoreRouter` test with a prefixed OpenDAL store and a second store, asserting keys route to the right one. Commands: `cargo test -p liquers-store`, `cargo test -p liquers-core --lib`, `cargo test -p liquers-lib --lib --tests`, `bash scripts/check-build-matrix.sh` (the `opendal`-off configuration must still compile). |
| **Behavioural risk** | *Compatibility*: `key_prefix()` changes multi-store routing — the one genuine risk, quantified in Q2. *Persistence/data*: on-disk paths are unchanged by construction and asserted by the round-trip test; no migration. *Concurrency*: not applicable — no shared mutable state is added; `PathMap` is stateless. *Performance*: one extra bounded `list` per `is_dir` **only** on the path that currently errors; `stat` still short-circuits on backends with directories. *Security*: the key-absoluteness guard is preserved and is now enforced in one place instead of three, which reduces the chance of a future traversal hole. *Error paths*: `is_dir` and `contains` return `Ok(true)` where they returned `Err` for a directory whose children exist; every other error is unchanged. |
| **Recovery** | The three changes are independent commits and each reverts alone. `key_prefix()` in particular is a one-line revert if a routing regression appears in the field. Nothing is persisted, so revert needs no migration. |
| **Certainty** | Two decisions are open (Q1, Q2 below). Unverified: `opendal::Operator::list_with(...).limit(1)` semantics on every backend — checked in the API surface, not executed; if `limit` is not honoured, an unbounded `list` with early exit on the first entry is the fallback. Also unverified: whether any *deployed* configuration uses a prefixed OpenDAL store, which is what determines whether Q2 is theoretical. Nothing in this repository does — `create_opendal_store` is the only constructor with a non-empty prefix, and no committed configuration file exercises it. |

## Open questions for the gate

**Q1 — is the directory-key gap (Phase 1 defect 2) in scope?**
It is the closest thing to the issue's stated symptom and the only part with a real design choice
(synthesize from a listing, versus declaring that directory keys need a backend with directories).
Doing it makes the issue an M; leaving it out makes the issue an S about path mapping and a prefix
bug, and needs a new issue filed for the gap. **Recommendation:** include it — the issue's
"correctness bug against real backends, not a limitation" framing is about exactly this, and object
stores are most of `OPENDAL_STORE_TYPES`.

**Q2 — the `key_prefix()` fix changes routing.**
Today a prefixed OpenDAL store advertises the root prefix, so `AsyncStoreRouter` offers it every
key (`store.rs:1711` gates on `key_prefix()` *and* `is_supported`, and `is_supported` does check the
real prefix — so ordinary `get`/`set` routing is already correct; what is wrong is `listdir`
aggregation at `:1843`/`:1846`, which consults only `key_prefix()`). So the blast radius is smaller
than it first looks, but it is not zero. **Recommendation:** fix it, in its own commit, with the
router test above. **Alternative:** split it into its own issue so this one stays purely about
paths. Nothing in this repository configures a prefixed OpenDAL store, so no in-tree behaviour
changes either way.

**Q3 — dead code.** The commented-out synchronous `OpenDALStore` (`:15-215`, 200 lines) is 27% of
the file and cannot compile. Delete it as part of this work, or leave it? **Recommendation:** leave
it; deleting it is a separate, uncontroversial cleanup that should not ride along inside a
correctness fix.

## Review record

*Against Phase 1:* every acceptance criterion has a named change and a named test. The non-goals
are respected — the FIXME is deleted rather than honoured, `test_opendal_localfs`'s weakness is
raised as a candidate rather than folded in, and `path_map.rs` is explicitly *not* created because
Phase 1 said the deliverable is "one place", not "one file".

*Against the codebase:* every line reference above was read at `HEAD`, not remembered. The claim
that ordinary routing already checks the real prefix (via `is_supported`) was traced through
`store.rs:1711` and `opendal_store.rs:514-520`, which is why Q2's risk is stated as smaller than the
headline suggests. `AsyncMemoryStore`'s synthesize-from-keys `is_dir` was read before proposing the
same shape. The `#[cfg]` split (`async_store`, `opendal`) was checked: every changed symbol is
already inside `#[cfg(feature = "async_store")]`, so no new gate is needed and the
`opendal`-off build is untouched.

*Rust review:* no `unwrap`/`expect` outside tests; no `println!`; errors go through the existing
typed constructors; the one unavoidable wildcard is over a foreign non-exhaustive enum and is
called out; `PathMap` is a stateless unit type so nothing is cloned or locked that is not cloned
today; every new function is `async` only where it awaits.

*Risk understatement check:* the existing-test estimate is **2-4**, which exceeds the automatic
clearance limit of three on its own, and Q1/Q2 are unresolved design choices. This work cannot
clear the gate automatically and does not claim to.

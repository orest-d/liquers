# Phase 3: Examples & Use-cases — OpenDAL path mapping and shared directory support

## High-Level Introduction

This is a repair, so its "examples" are the shapes that were broken. Each scenario below is a
situation a user can be in today, what happens to them, and what happens after the change — and
each one is written as a test that fails at `HEAD` and passes afterwards.

The progression follows Phase 1's two purposes. **Scenario 1** is the urgent one: two directories
whose names share a prefix, and a `removedir` that destroys the wrong one. It exercises
`PathMap::directory` and needs nothing from `liquers-core`. **Scenario 2** builds on it to reach the
broader purpose: the same store on an object-store backend, where a directory has no object to
`stat`, and the shared semantics that `liquers-core` now supplies. **Scenario 3** collects the
pitfalls that the architecture deliberately does *not* solve — the metadata-suffix exclusion, the
prefix convention, stray sidecars — because each is a place where a future reader will otherwise
assume a bug.

**Two findings from writing this phase are recorded before the examples, because they correct
Phase 2 rather than illustrate it.** Writing a test plan is how a design discovers that one of its
claims was not evidence.

### Finding 1 — "AsyncMemoryStore's existing tests prove the extraction faithful" was too strong

Phase 2 §3a rests the safety of extracting `DirectoryIndex` on `AsyncMemoryStore`'s existing tests
passing unchanged. Counted at `HEAD`, those are **one** behavioural test —
`test_async_memory_store_basic` (`store.rs:2194`) — plus `keyabs07`. The behavioural one uses a
single key `a/b/c`, checks `is_dir("a/b")` before and after one `set`, and after `remove` checks
only `contains`. It never covers what the extraction is most likely to break:

| Not covered today | Why it matters to the extraction |
|---|---|
| Two keys under one directory | the refcounts exist only for this case |
| `is_dir` after removing the last child | whether a directory retires is entirely refcount logic |
| `is_dir` after removing one of two children | the exact off-by-one the counts are there to prevent |
| `removedir` and its index cleanup | `remove_key_from_index` is called per key in a loop |
| `listdir` served from the index | the read path, untested at any depth |

**Consequence for the plan: characterize first, extract second.** MEMDIR01-05 below are written
against `HEAD`, must pass against `HEAD`, and are committed *before* the extraction. They then have
to pass unchanged afterwards. A test written after a refactor documents the refactor; a test written
before it documents the behaviour.

### Finding 2 — `AsyncMemoryStore::makedir` does nothing, and `DirectoryIndex` would change that

Writing MEMDIR04 exposed it: `makedir` (`store.rs:888`) validates its key and returns `Ok(())`,
recording nothing. `is_dir` is `false` immediately afterwards. It is structural — a derived index
cannot hold a childless directory — and it is the same wall `LocalStorageStore` hit and answered
with `explicit_dirs`.

Filed as `CORE-ASYNC-MEMORY-STORE-MAKEDIR-DOES-NOTHING` (P0/S: `PUT /api/store/makedir/{*key}` is
specified in `reference/WEB_API_SPECIFICATION.md` §4.1.10, and "a documented feature that does not
work" is §4.4's P0; the practical consequence is small). It bears on this design directly:
`DirectoryIndex::explicit` is exactly the missing capability, so the fix is one call to
`insert_directory`.

**Consequence for the plan:** the extraction commit keeps `makedir` a no-op, so it is provably
behaviour-preserving and MEMDIR04 asserts the current (wrong) behaviour. A **separate, later commit**
makes `makedir` call `insert_directory` and flips MEMDIR04 to assert the right behaviour. Two
commits, one behaviour change, visible in the diff. See the Test Plan's sequencing note.

## Example Type

**Runnable, and destined for the real test modules.** Not a proposal: the reproductions in Phase 1
were executed against both backends before Phase 2 was written, and the fixes were verified at the
operator level. The code below is the same code, promoted from scratch probes into
`liquers-store/src/opendal_store.rs`'s colocated test module and `liquers-core`'s. No
`examples/*.rs` demo is planned — Phase 2's Documentation Architecture requires no guide, and a
"demonstration" of a store not deleting the wrong directory is a test, not a tutorial.

The scenarios are therefore shown as the tests they become, rather than as illustrative snippets
that would then have to be rewritten.

## Overview Table

| # | Type | Name | What it demonstrates or checks | Pass |
|---|---|---|---|---|
| 1 | Example | Sibling directories and a destructive `removedir` | The P0: `PathMap::directory` and why the trailing slash is the mapping's business | primary |
| 2 | Example | A directory key on an object-store backend | The core piece: three sources of directory truth, one set of answers | detail |
| 3 | Example | Four things that look like bugs and are not | The metadata-suffix exclusion, the prefix convention, stray sidecars, `limit` as a hint | pitfalls |
| 4 | Unit tests | `PATHMAP01-06` (`liquers-store`) | Round-trip property, directory form, decode order, refusals | unit |
| 5 | Unit tests | `DIRIDX01-07` (`liquers-core`) | `DirectoryIndex`: edges, refcounts, explicit directories, sibling prefixes | unit |
| 6 | Characterization | `MEMDIR01-05` (`liquers-core`) | `AsyncMemoryStore`'s directory behaviour, pinned **before** the extraction | unit |
| 7 | Unit tests | `TRAITDEF01-02` (`liquers-core`) | The two changed `AsyncStore` defaults, on a minimal store | unit |
| 8 | Integration | `SIBLING01-04` (`liquers-store`) | Sibling safety across `removedir`, `listdir_keys_deep`, `keys()`, on memory **and** fs | integration |
| 9 | Integration | `DIR01-03` (`liquers-store`) | Directory keys addressable on memory; `is_dir` absent is `Ok(false)` | integration |
| 10 | Integration | `PREFIX01`, `ROUTER01` (`liquers-store`) | `key_prefix()` and what the router does with it | integration |
| 11 | Regression | `FSREG01`, `LOCALFS01`, `keyabs16/17` | The filesystem behaviour that already worked, kept working | regression |

> **On the workflow's drafting and review roles.** `liquers-project` specifies parallel Haiku
> drafters and reviewers with a Sonnet synthesizer. This host ran the passes **sequentially in the
> primary agent**, which `SKILL.md`'s Host Compatibility section permits ("perform the same
> independent review passes sequentially and record the same review outcomes"). The choice was
> deliberate rather than forced: this phase depends on probe output, exact line numbers and
> verified backend behaviour held in the primary agent's context, and cold drafters would have
> re-derived it less accurately. The review outcomes are recorded in the Review Record below,
> concern by concern.

## Example 1: Sibling directories and a destructive `removedir`

### Connection to the High-Level Design

This is Phase 1's Purpose, first paragraph, and defects 1 and 2. It touches nothing in
`liquers-core`, which is why Phase 2 puts it in commits 1-2 and lets it ship independently.

### Scenario

A project keeps its inputs under `data/` and a derived database export under `database/`, both in
one S3 bucket behind one OpenDAL store. Someone clears the inputs — through the UI, through
`DELETE /api/store/removedir/data`, or from a recipe. `data/` goes, and so does `database/`,
because `remove_all("data")` is a prefix delete and `database/` starts with `data`.

Nothing warns. The operation reports success. The names have to *share a prefix* for it to happen,
which is why a first reproduction probing one key in isolation missed it entirely.

### Sequence of Steps

1. `AsyncStore::removedir(key)` is called on the OpenDAL store.
2. Today: `key_to_path(key)` produces `"data"` — the **data** form, with no trailing slash.
3. `op.remove_all("data")` lists everything whose path *starts with* `data` and deletes it. That
   set includes `database/export.csv`.
4. After the change: `PathMap::directory(key)` produces `"data/"`, and `remove_all("data/")` is
   scoped to the directory.

The same substitution fixes `listdir_keys_deep`, where `list_with(path).recursive(true)` has the
identical prefix semantics.

### Core Example Code

```rust
/// `sibling01` — an operation on one directory must not reach a directory whose name shares its
/// prefix.
///
/// Run against both backends: the memory backend has no directory objects and the filesystem
/// backend does, and a fix that works on only one would pass a single-backend test.
#[tokio::test]
async fn sibling01_removedir_leaves_a_prefix_sharing_sibling() -> Result<(), Error> {
    for store in [memory_store(), fs_store("sibling01")?] {
        let inside = parse_key("data/input.csv")?;
        let sibling = parse_key("database/export.csv")?;
        store.set(&inside, b"in", &Metadata::new()).await?;
        store.set(&sibling, b"out", &Metadata::new()).await?;

        store.removedir(&parse_key("data")?).await?;

        assert!(!store.contains(&inside).await?, "the named directory is gone");
        assert!(store.contains(&sibling).await?, "the sibling survives");
        assert_eq!(store.get_bytes(&sibling).await?, b"out", "and is intact");
    }
    Ok(())
}
```

**Expected output at `HEAD`:** `sibling01 ... FAILED — the sibling survives`, on the filesystem
backend. This is the reproduction Phase 1 recorded, turned into an assertion.

**Expected output after the change:** `test result: ok`.

### Guide and Executable Example

No guide is planned (Phase 2, Documentation Architecture: no repeatable developer task is
introduced). The canonical code the documentation references is this test and `SIBLING03`, cited
from `specs/reference/STORE_SEMANTICS.md` at Phase 5 as the enforcement of "no operation on a key
may reach a sibling key".

### Validation

- [x] Fails at `HEAD` for the stated reason, on the stated backend — executed 2026-09-02
- [x] Demonstrates the core defect, not a proxy for it
- [x] Uses a realistic key shape (`data/` and `database/` is the case that motivated the issue)
- [x] Asserts the survivor's *content*, so a fix that leaves an empty file does not pass

## Example 2: A directory key on an object-store backend

### Connection to the High-Level Design

Phase 1's second purpose, and defect 4. This is where the `liquers-core` work earns its place:
Scenario 1 needed no shared mechanism, and this one is unfixable without deciding what `is_dir`
*means* on a backend that has no directories.

### Scenario

The same store, now backed by S3 rather than a local filesystem — or by the memory backend, which
behaves the same way and needs no network. A recipe writes `data/reports/q3.csv`. A UI then asks for
`-R-dir/data/reports` to list the folder.

`listdir("data/reports")` returns `["q3.csv"]`. `is_dir("data/reports")` returns an **error**.
`contains` says `false`. `get_metadata` says `KeyNotFound`, so `get_asset_info` fails and the
directory query fails. The listing can see a directory the addressing denies.

The cause is that S3 has no directory object at `data/reports`, so `stat` has nothing to find. The
filesystem backend does, which is why this scenario is invisible on `fs` and why the first
reproduction — run on `fs` — concluded the store was fine.

### Sequence of Steps

Building on Scenario 1's setup rather than repeating it, this is the delta:

1. `is_dir(key)` calls `op.stat(PathMap::data(key))`, as it does today.
2. The backend answers `NotFound` — not an error condition, just an absent object.
3. **New:** `is_dir` falls back to `has_children(key)`, a bounded listing of
   `PathMap::directory(key)` — `"data/reports/"`, with the trailing slash from Scenario 1, so it
   cannot pick up `data/reports_archive/`.
4. `contains` and `get_metadata` inherit from there: `contains` falls back to `is_dir`
   (`liquers-core`'s changed default), and `get_metadata`'s `KeyNotFound` branch consults
   `has_children` and returns `default_metadata(key, true)`.
5. `get_asset_info` needs no change: it is built on `get_metadata`.

The store supplies the *source of truth* — a listing, because its backend is authoritative and may
be written by another process. `liquers-core` supplies the *answers downstream of `is_dir`*, which
is what `AsyncMemoryStore`, `FetchStore` and `LocalStorageStore` each wrote for themselves.

### Core Example Code

```rust
/// `dir01` — on a backend with no directory objects, addressing agrees with listing.
///
/// This is `test_opendal_subdir`'s commented-out block, uncommented. The note it carries at
/// `HEAD` — "memory backend does not support directories explicitly, so not everything works as
/// it should" — is the bug, written down and then tolerated.
#[tokio::test]
async fn dir01_directory_key_is_addressable_without_directory_objects() -> Result<(), Error> {
    let store = memory_store();
    let key = parse_key("data/reports/q3.csv")?;
    let dir = parse_key("data/reports")?;
    store.set(&key, b"rows", &Metadata::new()).await?;

    assert_eq!(store.listdir(&dir).await?, vec!["q3.csv".to_string()]);
    assert!(store.is_dir(&dir).await?, "listing sees it, so addressing must too");
    assert!(store.contains(&dir).await?);
    assert!(store.get_metadata(&dir).await.is_ok());
    assert!(store.get_asset_info(&dir).await.is_ok());
    Ok(())
}

/// `dir02` — an absent key is `Ok(false)`, not an error.
///
/// Every other store answers this way: `AsyncFileStore` (`store.rs:1199`), `AsyncMemoryStore`
/// (`:822`), and the trait default (`:448`). The OpenDAL store returning `Err` is the divergence.
#[tokio::test]
async fn dir02_is_dir_on_an_absent_key_is_false_not_an_error() -> Result<(), Error> {
    let store = memory_store();
    assert!(!store.is_dir(&parse_key("nothing/here")?).await?);
    Ok(())
}
```

**Expected output at `HEAD`:** `dir01 ... FAILED` at the `is_dir` assertion with
`KeyReadError: NotFound … memory doesn't have this path`; `dir02 ... FAILED` with the same error
where `Ok(false)` was expected.

### Validation

- [x] Reproduces on the memory backend, which needs no network and no credentials
- [x] Asserts listing and addressing *agree*, rather than asserting a hard-coded answer
- [x] `dir02` pins the error-versus-`Ok(false)` divergence separately, so a fix for one cannot
      silently regress the other

## Example 3: Four things that look like bugs and are not

Each has a symptom a reader will meet, a cause, the correct expectation, and the test that protects
it. They are here because three of them will otherwise be re-filed as defects.

### 3.1 A key whose filename ends in `.__metadata__` is refused, not stored

**Symptom.** `set(parse_key("notes.__metadata__"), …)` fails with `key_not_supported`.

**Cause.** Metadata is a sidecar: the metadata for `foo` lives at `foo.__metadata__`. So the *data*
path of the key `foo.__metadata__` is byte-identical to the *metadata* path of the key `foo`. No
decoder can be injective over both while preserving the on-disk layout.

**Correct expectation.** Refusal, in one place. `is_supported` already refused such a key at
`HEAD`; what is new is that `PathMap::data` and `PathMap::metadata` refuse it too — at `HEAD`
`key_to_path` happily returns `Ok("notes.__metadata__")`, so the rule lived in one method and was
absent from another. `PATHMAP03` asserts both agree.

**Not a defect to repair here.** An unambiguous encoding (escaping the suffix) would change the
on-disk layout, which is out of scope. The round-trip corpus covers this key by asserting
**refusal**, which is what makes the exclusion explicit rather than accidental.

### 3.2 A store's prefix is part of its backend path, not stripped from it

**Symptom.** A store configured with `prefix: data` against a bucket root writes
`data/input.csv`, not `input.csv`. Someone expects the prefix to be a mount point that is removed.

**Cause and correct expectation.** That is the convention throughout this codebase —
`FileStore::key_to_path` pushes the whole key, prefix included, onto its root. `key_to_path`
therefore needs no change when `key_prefix()` is fixed; only the *advertised* prefix was wrong.
`liquers-web`'s `FetchStore` is the one store that strips its prefix, and it documents that it is
the exception.

**The test.** `PREFIX01` asserts both halves: `key_prefix()` reports `data`, and the backend path
still contains it.

### 3.3 A stray sidecar in the bucket appears in listings as its data key

**Symptom.** A bucket contains `sub/orphan.__metadata__` and no `sub/orphan`. `keys()` reports
`sub/orphan`, which cannot be read.

**Cause.** `PathMap::decode` maps a metadata path to the key of the data it describes — that is
`DecodedPath::Metadata(key)`'s whole purpose, and it is right: a sidecar written by Liquers implies
its data key. Nothing distinguishes "sidecar whose data was deleted out of band" from "sidecar being
written right now".

**Correct expectation.** Report it. This is `HEAD`'s behaviour and the change preserves it
deliberately — confirmed by probe on 2026-09-02, `keys() = ["sub", "sub/orphan", ""]`.

**The related decision.** `listdir` and `listdir_keys_deep` **skip** an entry `decode` refuses,
rather than failing the listing: one unexpected object in a shared bucket must not make a directory
unlistable. `PATHMAP06` covers the skip.

### 3.4 `limit(1)` on a listing is a hint, not a cap

**Symptom.** `has_children` asks for one entry and gets two.

**Cause.** OpenDAL's `limit` is a page-size hint. Measured on 2026-09-02:
`list_with("sub/").limit(1)` returned `["sub/a.txt", "sub/a.txt.__metadata__"]`.

**Correct expectation.** `has_children` tests `!entries.is_empty()` and **never** a count. A test
asserting `len() == 1` would pass on one backend and fail on another. `DIR03` asserts the
non-emptiness contract on a directory holding several children.

## Corner Cases

### 1. Memory

- **Large directories.** `has_children` asks for one page rather than a full listing, so `is_dir` is
  bounded regardless of directory size. `listdir_keys_deep` is *not* bounded — it materializes every
  key under a subtree into a `BTreeSet`, as it does today. The change does not make this worse and
  does not fix it; `keys()` on a million-object bucket is as expensive after as before. Noted, not
  addressed.
- **`DirectoryIndex` growth.** One entry per directory plus one per (parent, child) edge, which is
  `O(total path segments)` — the same order `AsyncMemoryStore::dir_index` already occupies. The
  extraction moves the allocation, it does not add one. The `explicit` set adds one `Key` per
  explicitly created directory, bounded by the number of `makedir` calls.
- **No new leak surface.** `PathMap` is a stateless unit type. `DirectoryIndex` holds `Arc`s in the
  same shape `AsyncMemoryStore` holds them today; `remove_key` retires an empty children map, so a
  create/delete cycle does not accumulate — `DIRIDX03` asserts the retirement.

### 2. Concurrency

- **`DirectoryIndex` is concurrent by construction**, `scc`-based like the code it is extracted
  from. It is not, and must not be claimed to be, *atomic across* operations: `insert_key` walks the
  ancestor edges one at a time, so a concurrent reader can observe a partially inserted path. That
  is `AsyncMemoryStore`'s behaviour at `HEAD` and the extraction preserves it exactly rather than
  quietly strengthening it — a strengthening would be a design change smuggled in as a refactor.
- **`DIRIDX08` (concurrency)**: spawn N tasks inserting distinct keys under one parent, await all,
  assert the child count. This checks the refcounts survive concurrent updates; it does not check
  cross-operation atomicity, which is not promised.
- **The OpenDAL store adds no shared mutable state.** `has_children` is a read. Two concurrent
  `is_dir` calls on the same key issue two listings; that is wasteful, not incorrect, and
  deduplicating it would need a cache with an invalidation story that the authoritative-backend
  argument in Phase 2 §3b rules out.
- **`removedir` is not atomic on any backend** — `remove_all` deletes entry by entry, so a crash
  mid-delete leaves a partial directory. Unchanged by this work, and true of `AsyncFileStore` too.
  Worth stating in `STORE_SEMANTICS.md` at Phase 5 rather than leaving each backend to imply it.

### 3. Errors

- **`is_dir` on an absent key** — `Ok(false)`, not `Err`. `DIR02`.
- **`is_dir` on a genuine backend failure** (permissions, network) — still `Err`, mapped through
  `map_read_error`. The fallback triggers on `ErrorKind::NotFound` alone, so an S3 403 does not get
  silently reported as "not a directory". This distinction is the reason the match is written as two
  arms rather than `is_err()`.
- **Relative keys** — every entry point still refuses with `ErrorType::KeyNotAbsolute`.
  `keyabs16` (`opendal_store.rs:540`) asserts this across seven methods and on `key_to_path`
  directly, and must pass **unchanged**; it is the regression guard on the fallible-mapping rule,
  and any edit to it during this work is a signal the guard moved.
- **A refused path inside a listing** — skipped, not propagated. §3.3.
- **`removedir` on a directory that does not exist** — `Ok(())`, matching `AsyncFileStore`
  (`store.rs:1171-1183`). `REMOVE01` asserts it so a future rewrite cannot quietly make it an error.
- **`removedir` on the root key** — deletes the whole store. `PathMap::directory(Key::new())` is
  `""` and `remove_all("")` is a full wipe, which is what "remove the root directory" means and what
  `AsyncFileStore` would do. `REMOVE02` asserts it deliberately: it is the one case where the
  prefix-scoping fix does *not* narrow anything, and leaving it untested would make a future
  reader wonder whether it was overlooked.

### 4. Serialization

- **Nothing new is serialized.** `PathMap` and `DirectoryIndex` are derived state; neither has
  `serde` derives and neither is persisted.
- **On-disk layout is unchanged, which is the load-bearing compatibility claim.** `PathMap::data`
  produces exactly `key.as_absolute()?.encode()`, as `HEAD` does. `PATHMAP01`'s round-trip corpus is
  what pins it: a store written by the current code must read identically after the change.
- **Metadata sidecars** keep their `.__metadata__` suffix and their JSON body. `get_metadata`'s
  `MetadataRecord`-then-`LegacyMetadata` parse order is untouched.
- **The directory-metadata value** returned by the new `KeyNotFound` branch is
  `default_metadata(key, true)` — the same value the existing `stat().is_dir()` branch returns, so
  the two paths cannot diverge. `DIR01` reaches both.

### 5. Integration (cross-crate)

- **`liquers-core` → everything.** Two `AsyncStore` trait defaults change. Both are widenings of a
  permissive default and neither adds a required method, so every implementation still compiles.
  `TRAITDEF01-02` cover them on a minimal store; `keyabs17` (`store.rs:2355`) is the guard that the
  refusal semantics survive, and was checked against its body: `contains(ok) == false` still holds
  because `is_dir`'s default stays `Ok(false)`.
- **`liquers-web`.** `FetchStore` and `LocalStorageStore` override both changed defaults, so neither
  is reached — but "should not be reached" is the kind of claim this design has already been wrong
  about once, so the Node conformance loop is run rather than reasoned about. It requires a
  `cargo clean` first (`CLAUDE.md`), so it is a checkpoint, not part of the inner loop.
- **`liquers-axum`.** Inherits the trait defaults and serves `removedir`. No handler changes; the
  store endpoints' behaviour changes underneath them, which is the point.
- **`liquers-py`.** Wraps the sync `Store`, not `AsyncStore` — checked at `liquers-py/src/store.rs:102`.
  Untouched.
- **`AsyncStoreRouter`.** Not edited, behaviour changed by `key_prefix()`. `ROUTER01` is the only
  test that exercises a prefixed OpenDAL store inside a router, which is the configuration Phase 2's
  Q2 identified as the one with real reach.
- **Feature matrix.** `store_dir_index` is unconditional in `liquers-core` and adds no dependency.
  In `liquers-store` every changed symbol is already behind `async_store`. The
  `opendal`-without-`async_store` row becomes buildable for the first time (§6), so
  `check-build-matrix.sh` gains a configuration rather than merely keeping one.

## Documentation and Learning Log

### Guide Candidate Workflows and Examples

None. Phase 2's Documentation Architecture records `guide: neither` and this phase does not
overturn it: no repeatable developer task is introduced, and the prefix convention (§3.2) — the one
candidate — is better served by a sentence in `STORE_SEMANTICS.md` than by a guide nobody would
look for. Reconsider only if a reader has to be told how to *configure* a prefixed store, which is
`STORE_CONFIG_FSD.md`'s territory.

### Usage, Meaning, and Connections

For `specs/reference/STORE_SEMANTICS.md` at Phase 5, sourced from this phase:

- The three sources of directory truth, and which backend shape uses which — the table in Phase 2
  §3a is the shape this section should take.
- `is_dir` on an absent key is `Ok(false)`; a genuine backend failure is still `Err`. The
  distinction, not just the happy answer.
- An **explicitly created** empty directory is distinct from a **derived** one: the derived kind
  retires when its last child goes, the explicit kind persists until `removedir`. This is what
  `makedir` needs and what only `LocalStorageStore` could express before.
- `removedir` is recursive and **scoped to the directory** — no operation on a key may reach a
  sibling key. The rule that four of the six defects violated, stated as a rule.
- `removedir` on an absent directory is a no-op; on the root key it empties the store.
- `removedir` is not atomic on any backend.
- The prefix convention: a store's `key_prefix` is part of the path under its backend root, and
  `key_prefix()` must report it. `FetchStore` is the documented exception.

### Repeatable Development Guidance

- **Probe two siblings, not one key.** The 2026-08-29 reproduction was competent and thorough on a
  single key `sub/deeper/foo.txt`, and it concluded the store was correct. The bug needs two
  directories whose names *share a prefix* to become visible. Any store test corpus should include
  `sub/` and `subway/` for this reason, and `STORE_SEMANTICS.md`'s sibling rule is the reason to
  keep them.
- **Probe the memory backend and the filesystem backend.** They differ in exactly the way that
  matters — one has directory objects, the other does not — so a single-backend test can pass with
  the defect intact. This is how defect 4 stayed invisible.
- **Read the vendored dependency when its contract is the question.** `create_dir`'s trailing-slash
  requirement is one line in `opendal/types/operator/operator.rs:460`; that line is what disproved a
  claim two documents had already repeated. The claim had been reasoned about twice and read zero
  times.

### Corrections and Unexpected Learning

Recorded as they happened, for the Phase 5 summary:

1. **The issue's headline was right, and the first reproduction's rebuttal was wrong** — but only in
   a case the rebuttal never tried. Both documents were correct about what they tested.
2. **`make_sub_dirs` has never worked**, on any backend, in any release. Two design documents
   asserted it satisfied the `//TODO: create_dir` markers. `let _ignore` on a call that always fails
   is indistinguishable from a call that always succeeds.
3. **Four stores had already solved the directory problem privately** — this only became visible
   when the gate asked where the fallback belonged. The duplication was not discovered by design
   review; it was discovered by being asked a question that required looking.
4. **`AsyncMemoryStore::makedir` does nothing** (Finding 2). Found by writing a characterization
   test, not by reading the function — reading it, `let key = key.as_absolute()?; Ok(())` looks like
   validation followed by success.
5. **Phase 2's own safety argument was thin** (Finding 1). The claim "existing tests prove the
   extraction faithful" was written without counting the tests. Counting them took one grep.

## Test Plan

### Sequencing

The order is part of the plan, not an implementation detail:

| Step | Commit | Tests | Must be true |
|---|---|---|---|
| 1 | characterization | `MEMDIR01-05` | pass at `HEAD`, before any change |
| 2 | `PathMap` + trailing slash | `PATHMAP01-06`, `SIBLING01-03`, `REMOVE01-02`, `FSREG01` | the P0 is fixed; `keyabs16` unchanged |
| 3 | `key_prefix()` | `PREFIX01`, `ROUTER01`, `SIBLING04`, `opendal03` assertion enabled | routing corrected |
| 4 | `DirectoryIndex` in core | `DIRIDX01-08`; `MEMDIR01-05` pass **unchanged** | the extraction is faithful |
| 5 | trait defaults | `TRAITDEF01-02`; `keyabs17` unchanged | semantics shared |
| 6 | OpenDAL adopts them | `DIR01-03` | defect 4 fixed |
| 7 | deletions, hygiene | existing suite | nothing lost |
| 8 | *(separate)* `makedir` fix | `MEMDIR04` **flipped** | `CORE-ASYNC-MEMORY-STORE-MAKEDIR-DOES-NOTHING` closed |

Step 1 before step 4 is the point of Finding 1. Step 8 after step 4, and separate from it, is the
point of Finding 2: the extraction commit must be behaviour-preserving, so the behaviour change gets
its own commit and its own flipped assertion.

### Unit Tests — `liquers-core/src/store_dir_index.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_key;

    /// `diridx01` — every ancestor edge a key implies, and no others.
    #[test]
    fn diridx01_edges_for_key() -> Result<(), Error> {
        assert!(DirectoryIndex::edges_for_key(&Key::new()).is_empty(), "root implies nothing");
        assert_eq!(DirectoryIndex::edges_for_key(&parse_key("a")?).len(), 1);
        // a/b/c -> (root,a), (a,a/b), (a/b,a/b/c)
        assert_eq!(DirectoryIndex::edges_for_key(&parse_key("a/b/c")?).len(), 3);
        // a name that is not ASCII must not be treated differently
        assert_eq!(DirectoryIndex::edges_for_key(&parse_key("données/rapport.csv")?).len(), 2);
        Ok(())
    }

    /// `diridx02` — building from a key set and inserting incrementally agree.
    ///
    /// `FetchStore` does the first, `AsyncMemoryStore` the second. If they disagreed, one of the
    /// two callers would be getting a different tree from the same keys.
    #[tokio::test]
    async fn diridx02_from_keys_matches_incremental_insertion() -> Result<(), Error> {
        let keys = ["a/b/c.txt", "a/b/d.txt", "a/e.txt", "f.txt"]
            .iter().map(|k| parse_key(k)).collect::<Result<Vec<_>, _>>()?;

        let built = DirectoryIndex::from_keys(keys.clone()).await;
        let incremental = DirectoryIndex::new();
        for key in &keys { incremental.insert_key(key).await; }

        for probe in ["", "a", "a/b", "a/b/c.txt", "g"] {
            let key = parse_key(probe)?;
            assert_eq!(built.is_dir(&key).await, incremental.is_dir(&key).await, "is_dir {probe}");
            assert_eq!(built.children(&key).await, incremental.children(&key).await, "children {probe}");
        }
        Ok(())
    }

    /// `diridx03` — refcounts: a directory outlives all but its last child.
    ///
    /// This is the case the refcounts exist for and the one `AsyncMemoryStore`'s own tests never
    /// reach. Removing `a/b/c.txt` must not retire `a/b`, because `a/b/d.txt` still occupies it.
    #[tokio::test]
    async fn diridx03_directory_retires_only_with_its_last_child() -> Result<(), Error> {
        let index = DirectoryIndex::new();
        let (c, d) = (parse_key("a/b/c.txt")?, parse_key("a/b/d.txt")?);
        index.insert_key(&c).await;
        index.insert_key(&d).await;
        let dir = parse_key("a/b")?;
        assert!(index.is_dir(&dir).await);

        index.remove_key(&c).await;
        assert!(index.is_dir(&dir).await, "one child left, still a directory");

        index.remove_key(&d).await;
        assert!(!index.is_dir(&dir).await, "no children left, no longer a directory");
        assert!(!index.is_dir(&parse_key("a")?).await, "and the retirement propagates upward");
        Ok(())
    }

    /// `diridx04` — an explicitly created directory needs no children.
    ///
    /// The capability `AsyncMemoryStore` lacks and `LocalStorageStore` had to invent privately.
    #[tokio::test]
    async fn diridx04_explicit_directory_survives_without_children() -> Result<(), Error> {
        let index = DirectoryIndex::new();
        let dir = parse_key("empty/folder")?;
        assert!(!index.is_dir(&dir).await);

        index.insert_directory(&dir).await;
        assert!(index.is_dir(&dir).await, "explicit, so childless is fine");
        assert!(index.children(&dir).await.is_empty());
        assert!(index.is_dir(&parse_key("empty")?).await, "its parent is a directory too");

        index.remove_directory(&dir).await;
        assert!(!index.is_dir(&dir).await);
        Ok(())
    }

    /// `diridx05` — an explicit directory that also has children survives losing them.
    ///
    /// The two mechanisms must compose: `makedir` then `set` then `remove` leaves the directory
    /// the user explicitly created, not nothing.
    #[tokio::test]
    async fn diridx05_explicit_and_derived_compose() -> Result<(), Error> {
        let index = DirectoryIndex::new();
        let dir = parse_key("mixed")?;
        let child = parse_key("mixed/file.txt")?;
        index.insert_directory(&dir).await;
        index.insert_key(&child).await;

        index.remove_key(&child).await;
        assert!(index.is_dir(&dir).await, "explicitly created, so it outlives its children");
        Ok(())
    }

    /// `diridx06` — `children` is direct, sorted and deduplicated.
    #[tokio::test]
    async fn diridx06_children_are_direct_sorted_and_unique() -> Result<(), Error> {
        let index = DirectoryIndex::new();
        for k in ["z/1.txt", "a/2.txt", "a/3.txt", "a/deep/4.txt"] {
            index.insert_key(&parse_key(k)?).await;
        }
        assert_eq!(index.children(&Key::new()).await, vec!["a".to_string(), "z".to_string()]);
        let a = index.children(&parse_key("a")?).await;
        assert_eq!(a, vec!["2.txt".to_string(), "3.txt".to_string(), "deep".to_string()],
                   "direct children only — 4.txt is not among them");
        Ok(())
    }

    /// `diridx07` — a directory whose name is a prefix of another is not confused with it.
    ///
    /// The index is keyed by `Key`, so this cannot fail the way the *path*-based store did. The
    /// test exists so that a future rewrite to a string-keyed index fails loudly.
    #[tokio::test]
    async fn diridx07_sibling_prefixes_are_distinct() -> Result<(), Error> {
        let index = DirectoryIndex::new();
        index.insert_key(&parse_key("sub/a.txt")?).await;
        index.insert_key(&parse_key("subway/b.txt")?).await;

        assert_eq!(index.children(&parse_key("sub")?).await, vec!["a.txt".to_string()]);
        assert_eq!(index.children(&parse_key("subway")?).await, vec!["b.txt".to_string()]);
        index.remove_key(&parse_key("sub/a.txt")?).await;
        assert!(!index.is_dir(&parse_key("sub")?).await);
        assert!(index.is_dir(&parse_key("subway")?).await, "the sibling is untouched");
        Ok(())
    }

    /// `diridx08` — concurrent insertion under one parent keeps the counts right.
    ///
    /// Checks the refcounts under contention. It does **not** check cross-operation atomicity:
    /// `insert_key` walks ancestor edges one at a time, so a concurrent reader can see a partially
    /// inserted path. That is `AsyncMemoryStore`'s behaviour and is preserved deliberately.
    #[tokio::test]
    async fn diridx08_concurrent_insertion_is_consistent() -> Result<(), Error> {
        let index = std::sync::Arc::new(DirectoryIndex::new());
        let mut handles = Vec::new();
        for i in 0..32 {
            let index = index.clone();
            handles.push(tokio::spawn(async move {
                if let Ok(key) = parse_key(&format!("shared/file{i}.txt")) {
                    index.insert_key(&key).await;
                }
            }));
        }
        for handle in handles { handle.await.map_err(|e| Error::general_error(e.to_string()))?; }
        assert_eq!(index.children(&parse_key("shared")?).await.len(), 32);
        Ok(())
    }
}
```

### Unit Tests — `liquers-core/src/store.rs` (characterization, written first)

`MEMDIR01-05` pin `AsyncMemoryStore`'s directory behaviour **at `HEAD`**, then must pass unchanged
after the extraction. They live in the existing `mod tests`.

| Test | Asserts (at `HEAD`, and after) |
|---|---|
| `MEMDIR01` | `set("a/b/c")` makes `a` and `a/b` directories; `a/b/c` is not one |
| `MEMDIR02` | two keys under `a/b`; removing one leaves `a/b` a directory; removing both retires `a/b` **and** `a` |
| `MEMDIR03` | `listdir` from the index at each depth: direct children only, sorted |
| `MEMDIR04` | **`makedir("empty")` records nothing — `is_dir` is `false` afterwards.** Asserts the *current* behaviour, and is flipped in step 8 |
| `MEMDIR05` | `removedir("a")` clears the subtree and the index; `is_dir("a")` is `false`, `keys()` no longer lists them |

`MEMDIR04` is deliberately an assertion of wrong behaviour, with a comment saying so and naming
`CORE-ASYNC-MEMORY-STORE-MAKEDIR-DOES-NOTHING`. A characterization test that quietly asserted the
*desired* behaviour would fail at `HEAD` and could not do its job.

### Unit Tests — the changed trait defaults

```rust
/// `traitdef01` — the default `contains` falls back to `is_dir`.
///
/// Built on `keyabs17`'s `MinimalStore`, which implements only the two methods that have no
/// default, so every other method exercised is the trait's own body.
#[tokio::test]
async fn traitdef01_default_contains_falls_back_to_is_dir() -> Result<(), Error> {
    struct DirOnlyStore;   // is_dir true for exactly one key; everything else defaulted
    // ... impl AsyncStore with get/set_metadata/is_dir ...
    let store = DirOnlyStore;
    assert!(store.contains(&parse_key("a/b")?).await?, "a directory is contained");
    assert!(!store.contains(&parse_key("a/c")?).await?);
    Ok(())
}

/// `traitdef02` — a store can inherit directory metadata without the recursive subtree walk.
#[tokio::test]
async fn traitdef02_directory_metadata_without_children() -> Result<(), Error> {
    // A store whose `directory_metadata_includes_children()` is false gets
    // `default_metadata(key, true)` with `children` empty, and `listdir_asset_info` is never called.
    Ok(())
}
```

`keyabs17` (`store.rs:2355`) must pass **unchanged**: `contains(ok) == false` still holds through
the new fallback because `is_dir`'s default stays `Ok(false)`, and `key.as_absolute()?` is evaluated
before the fallback so the refusal is unaffected. Verified against the test body.

### Unit Tests — `liquers-store/src/opendal_store.rs`

| Test | Asserts |
|---|---|
| `PATHMAP01` | Round-trip over a ~20-key corpus: `decode(data(k))` is `Data(k)` for every supported key. Corpus: root, `a`, `a/b`, `a/b/c.txt`, names with dots (`a.b.c/d.e.f`), unicode (`données/rapport.csv`), a name that is a prefix of a sibling (`sub`, `subway`), a long chain, a name containing the suffix but not ending in it (`x.__metadata__.txt`) |
| `PATHMAP02` | `decode(metadata(k))` is `Metadata(k)` — a sidecar path decodes to its **data** key |
| `PATHMAP03` | A key whose filename ends in `.__metadata__` is refused by `data`, `metadata` **and** `is_supported`, all three, with `ErrorType::KeyNotSupported`. The exclusion is explicit |
| `PATHMAP04` | `directory(Key::new())` is `""`; `directory(k)` ends in exactly one `/`; `data(k)` never does |
| `PATHMAP05` | Decode order: `"sub/"` is `Directory`, `"sub/f.txt.__metadata__"` is `Metadata`, and the suffix is stripped **once** from the final segment only |
| `PATHMAP06` | `listdir` skips an entry `decode` refuses instead of failing the listing |
| `keyabs16` | **unchanged** — the relative-key guard across seven methods and `key_to_path` |

### Integration Tests — `liquers-store`

Each runs against **both** the memory and filesystem backends unless noted.

| Test | Asserts | Fails at `HEAD`? |
|---|---|---|
| `SIBLING01` | `removedir("data")` leaves `database/export.csv` readable and intact | yes (fs) |
| `SIBLING02` | `removedir` on a deeper directory does not reach its own prefix-sharing sibling | yes |
| `SIBLING03` | `listdir_keys_deep("sub")` returns nothing from `subway/` | yes (both) |
| `SIBLING04` | a store with `prefix: data` sharing a backend root with `database/`: `keys()` returns only `data/…`. **Needs both the trailing slash and `key_prefix()`** — the one test that fails if either fix is missing | yes |
| `DIR01` | memory: `is_dir`/`contains`/`get_metadata`/`get_asset_info` agree with `listdir` | yes (memory) |
| `DIR02` | `is_dir` on an absent key is `Ok(false)` | yes (memory) |
| `DIR03` | `has_children` is non-emptiness, not a count: a directory with 5 children reports `true` | n/a (new) |
| `REMOVE01` | `removedir` on an absent directory is `Ok(())` | no — pins current behaviour |
| `REMOVE02` | `removedir(Key::new())` empties the store, deliberately | no — pins intent |
| `PREFIX01` | `key_prefix()` is the configured prefix; `store_name()` names it; the backend path still contains it | yes |
| `ROUTER01` | `AsyncStoreRouter` with a prefixed OpenDAL store and a memory store: `get`/`set` route correctly (already true), and `is_dir`/`listdir` no longer answer from the OpenDAL store for keys outside its prefix | yes |
| `FSREG01` | Phase 1's filesystem reproduction (`sub/deeper/foo.txt` through nine methods), as assertions | no — regression guard |
| `LOCALFS01` | `test_opendal_localfs` panics on a non-`AssetInfo` result and asserts `names` contains `"opendal_store.rs"` | no — closes `OPENDAL-LOCALFS-TEST-SILENT-ON-WRONG-VALUE-TYPE` |

Shared helpers, in the test module:

```rust
fn memory_store() -> AsyncOpenDALStore {
    let op = Operator::new(opendal::services::Memory::default())
        .expect("memory operator").finish();
    AsyncOpenDALStore::new(op, Key::new())
}

/// A filesystem-backed store in a uniquely named temp directory, mirroring
/// `store.rs`'s `unique_temp_dir` so two runs cannot collide.
#[cfg(feature = "services-fs")]
fn fs_store(label: &str) -> Result<(AsyncOpenDALStore, TempDirGuard), Error> { /* … */ }
```

`fs_store` returns a guard that removes the directory on drop, so a failing assertion does not leave
the next run's temp space populated — a failure mode the scratch probes hit during Phase 1.

### Manual Validation

| Command | Checks | When |
|---|---|---|
| `cargo test -p liquers-core --lib` | `DIRIDX*`, `MEMDIR*`, `TRAITDEF*`, `keyabs07/17` | every step |
| `cargo test -p liquers-store` | `PATHMAP*`, `SIBLING*`, `DIR*`, `PREFIX01`, `ROUTER01`, `keyabs16` | every step |
| `cargo test -p liquers-lib --lib --tests` | the default loop; nothing here should move | steps 4-7 |
| `bash scripts/check-build-matrix.sh` | 11 configurations plus wasm32; the `opendal`-without-`async_store` row becomes buildable | steps 5-7 |
| `cargo clean && cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles` | `FetchStore`/`LocalStorageStore` against the changed trait defaults | after step 5, once |
| `cargo run -p liquers-lib --features cli --bin export-command-registry -- --format yaml -o specs/command_registry.yaml` | **not needed** — no command changes (Phase 2, Relevant Commands) | — |

The wasm loop needs a `cargo clean` first (`CLAUDE.md`), so it is a checkpoint rather than part of
the inner loop. Budget it once, after the trait defaults land.

## Review Record

The workflow's three Phase 3 reviewer concerns, performed as separate passes:

**Reviewer 1 — Phase 1 conformity.** Every acceptance criterion has a test. Criterion 1 (sibling
safety) → `SIBLING01-04`; 2 (one mapping, round-trip) → `PATHMAP01-06`; 3 (`key_prefix`) →
`PREFIX01`, `ROUTER01`, `SIBLING04`; 4 (directory fallback in core, `AsyncMemoryStore` adopting it
with tests unchanged) → `DIRIDX01-08` and `MEMDIR01-05`, and the criterion's own wording is what
Finding 1 corrects; 5 (markers and dead code resolved) → step 7 against the existing suite; 6 (no
change to what worked) → `FSREG01`. Non-goals respected: no conformance suite is built, no
`liquers-web` store is migrated, `path_map.rs` is not created.

**Reviewer 2 — Phase 2 conformity.** Signatures used in the test code match Phase 2's Function
Signatures exactly: `PathMap::{data, metadata, directory, decode}`, `DecodedPath::{Data, Metadata,
Directory}`, `has_children`, `DirectoryIndex::{new, from_keys, insert_key, remove_key,
insert_directory, remove_directory, is_dir, children, edges_for_key}`. Two divergences found and
carried back: (a) Phase 2's "existing tests prove the extraction faithful" is corrected by Finding 1
— the plan now writes the tests first; (b) Phase 2 did not anticipate that `DirectoryIndex::explicit`
changes `AsyncMemoryStore::makedir`'s behaviour, which Finding 2 records and sequences as its own
commit. `DIRIDX05` (explicit and derived composing) is a case Phase 2 did not specify; the answer
chosen — an explicitly created directory outlives its children — is what `makedir` means and matches
`LocalStorageStore`.

**Reviewer 3 — codebase and query validation.** No queries appear in this phase except
`-R-dir/src`, which is pre-existing in `test_opendal_localfs` and unchanged; there is nothing new to
validate with `liquers-validate`, and no command is added, so `specs/command_registry.yaml` is
untouched. Store availability: every test constructs its own store, and `LOCALFS01` uses the
existing `SimpleEnvironment` setup. API checks against `HEAD`: `Key::{prefix_of_size, iter, filename,
join, encode, len, parent, as_absolute}` all exist as used (`query.rs:1444-1569`); `scc::HashSet`
exists in `scc` 3.4 (`hash_set.rs`, re-exported at `lib.rs:22`) so `DirectoryIndex::explicit` is
constructible; `scc` is an unconditional `liquers-core` dependency (`Cargo.toml:66`) and therefore
available on wasm32. Test-count claims in Finding 1 were obtained by reading `store.rs`'s two test
modules, not estimated.

**Conventions.** Tests return `Result<(), Error>` and use `?`; no `unwrap()`/`expect()` outside test
bodies, and the two in the library are removed by this work; no `println!` — `eprintln!` only, and
this phase adds none; typed error constructors throughout; `#[tokio::test]` for async and `#[test]`
for the one pure function.

## Open Questions for the Gate

1. **`MEMDIR04` asserts wrong behaviour on purpose, then is flipped in step 8.** The alternative is
   to skip step 8 entirely and leave `CORE-ASYNC-MEMORY-STORE-MAKEDIR-DOES-NOTHING` open for its own
   change. **Recommendation:** keep step 8 here — the mechanism that fixes it lands anyway, the fix
   is one call, and leaving a P0-classified issue open next to the change that enables its fix is
   worse than a two-commit sequence.
2. **`DIRIDX05`'s answer is a decision, not a derivation.** An explicitly created directory outliving
   its children is what `makedir` means and what `LocalStorageStore` does, but nothing in the
   codebase states it. It becomes a line in `STORE_SEMANTICS.md`. Flag if you read it differently.
3. **Cross-operation atomicity is deliberately not promised.** `insert_key` walks ancestor edges one
   at a time, so a concurrent reader can observe a partially inserted path — `AsyncMemoryStore`'s
   behaviour today, preserved rather than strengthened. Strengthening it is a design change, not a
   refactor; say so if it should be one.

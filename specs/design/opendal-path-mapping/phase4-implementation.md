# Phase 4: Implementation Plan — OpenDAL path mapping and shared directory support

## Overview

Eight commits in two crates, ordered so the P0 lands first and depends on nothing else. Each step
names its files, its symbols, its tests and the command that proves it. Every line reference is to
`HEAD` at 2026-09-02 and will drift as earlier steps land — **address symbols, not lines**; the line
numbers are navigation aids, not addresses.

| Step | Commit | Crate | Net effect | Fails-first test |
|---|---|---|---|---|
| 1 | characterization tests | core | pins `AsyncMemoryStore`'s directory behaviour at `HEAD` | none — must pass as-is |
| 2 | `PathMap`, trailing slash | store | **the P0**: no operation reaches a sibling | `SIBLING01-03` |
| 3 | `key_prefix()` | store | router aggregation corrected | `PREFIX01`, `SIBLING04` |
| 4 | `DirectoryIndex` | core | the shared mechanism | `DIRIDX01-08` |
| 5 | `contains` default | core | semantics inherited, not restated | `TRAITDEF01` |
| 6 | OpenDAL directory fallback | store | defect 4 | `DIR01-03` |
| 7 | deletions and hygiene | store | `make_sub_dirs`, dead block, warnings, 2 folded-in issues | existing suite |
| 8 | `makedir` records | core | `CORE-ASYNC-MEMORY-STORE-MAKEDIR-DOES-NOTHING` | `MEMDIR04` flipped |

**Steps 2-3 are shippable alone.** They touch `liquers-store` only and are the data-loss fix plus
the routing correction. If steps 4-8 need another design round, 2-3 do not wait.

### Two design refinements this phase makes to Phase 2

Both come from applying the `rust-best-practices` lens to Phase 2's signatures.

**R1 — `PathMap` cannot construct `key_not_supported`, and should not try.**
`Error::key_not_supported(key, store_name)` (`error.rs:307`) needs the store's name; `PathMap`'s
functions are associated (no `&self`) and have none. Threading one through would mean
`PathMap::data(key, &self.store_name())` at every call site, and `store_name()` allocates a `String`
per call — on the key-encoding path, which `CLAUDE.md` lists as performance-sensitive.

**This is an `Error` API gap, not a fact about path mapping** — raised at the gate and worth stating
before the workaround. `ErrorPayload` (`error.rs:58`) carries `query`, `key`, `position` and
`command_key` as fields with builders (`with_query`, `with_key`, `with_position`,
`with_command_key`, `:143-160`); the store name is the one piece of provenance that is prose rather
than data. With a `store: Option<String>` field and a matching `with_store_name(...)`, a helper with
no `&self` could raise the error unattributed and the store boundary could enrich it:

```rust
Err(Error::key_not_supported_unattributed(key))          // in PathMap
    .map_err(|e| e.with_store_name(&self.store_name()))  // at the store boundary
```

Filed as `CORE-ERROR-STORE-NAME-NOT-STRUCTURED` (P2/S). **Not done here**: it changes a `serde`
payload that reaches `liquers-web`, `liquers-py` and the axum API, and this design is already
cross-crate with a P0 in it. When it lands, `PathMap::data` and `::metadata` can enforce the refusal
themselves and the store keeps only the enrichment.

**Resolution for now: split the rule from the message.** `PathMap` owns the *predicate*; the store
owns the *error*. Note that **the predicate is needed regardless of the `Error` API**:
`is_supported(&self, key) -> bool` returns a bool and cannot use an error at all, so the rule has to
exist as a predicate for it to consult. The split is not purely a workaround — what
`with_store_name` would buy is the *second* half, letting the path builders enforce what they
document.

```rust
impl PathMap {
    /// True when the key's filename ends in the metadata suffix, so its data path would be
    /// byte-identical to another key's metadata path. Refused everywhere, in one predicate.
    fn is_suffix_ambiguous(key: &Key) -> bool;

    fn data(key: &Key) -> Result<String, Error>;      // fallible only via Key::as_absolute
    fn metadata(key: &Key) -> Result<String, Error>;
    fn directory(key: &Key) -> Result<String, Error>;
    fn decode(path: &str) -> Result<DecodedPath, Error>;
}

impl AsyncOpenDALStore {
    fn reject_ambiguous(&self, key: &Key) -> Result<(), Error> {
        if PathMap::is_suffix_ambiguous(key) {
            return Err(Error::key_not_supported(key, &self.store_name()));
        }
        Ok(())
    }
}
```

`is_supported` and every path builder call the same predicate, which is what Phase 2 wanted — one
place — while the error text keeps naming the real store and the happy path allocates nothing extra.
`Key::as_absolute` already yields `KeyNotAbsolute` with no store name (`error.rs:319`), so `data`,
`metadata` and `directory` stay fallible with no further help.

**The residual weakness, stated rather than hidden:** `PathMap::data` does not itself refuse an
ambiguous key — the store's entry points do, immediately before calling it. A future call site that
uses `PathMap::data` directly would bypass the rule. A doc comment on `PathMap` says so and names
`CORE-ERROR-STORE-NAME-NOT-STRUCTURED` as what would close the hole.

**R2 — `directory_metadata_includes_children` is dropped.** Phase 2 §"Trait Implementations"
proposed it so a store could inherit directory metadata without the recursive subtree walk. Checked
at `HEAD`: **every** in-tree store overrides `get_metadata` — `AsyncMemoryStore` (`store.rs:714`),
`AsyncFileStore`, `AsyncStoreRouter`, `AsyncOpenDALStore` (`opendal_store.rs:318`),
`LocalStorageStore` (`local_storage.rs:402`). The hook would have **no consumer in this change**, and
`AsyncOpenDALStore` — the store it was meant to serve — fixes its own override instead (step 6).
Adding a trait method for a hypothetical caller is speculative API on a trait every integration
implements. Dropped; if a store ever wants to inherit directory metadata, adding it then is a
one-line change with a real caller to justify it.

The `contains` default fallback (step 5) is **kept**, on a different balance: it is three lines, it
encodes a rule three stores already implement by hand, and its absence is a trap — a future store
that overrides `is_dir` and not `contains` gets the two disagreeing, silently. That is a correctness
default, not a hook.

## Implementation Steps

### Step 1 — Characterization tests for `AsyncMemoryStore`'s directory behaviour

**Why first:** Phase 3, Finding 1. The extraction in step 4 is guarded by these; written afterwards
they would document the refactor instead of the behaviour.

**File:** `liquers-core/src/store.rs`, in the existing `#[cfg(test)] mod tests` (`:2162`).

**Add:** `MEMDIR01-05` per Phase 3's table. `MEMDIR04` asserts that `makedir` records **nothing** —
the current, wrong behaviour — with a comment naming
`CORE-ASYNC-MEMORY-STORE-MAKEDIR-DOES-NOTHING` and stating that step 8 flips it.

**Do not change any source.** This commit is tests only.

**Validate:**
```bash
cargo test -p liquers-core --lib store::tests::memdir
cargo test -p liquers-core --lib          # nothing else moves
```
All five must pass **against unmodified source**. A failure here means the characterization is wrong,
not the code — fix the test.

---

### Step 2 — `PathMap`, `DecodedPath`, and the trailing slash *(the P0)*

**File:** `liquers-store/src/opendal_store.rs`.

**Add**, inside the existing `#[cfg(feature = "async_store")]` region:

- `struct PathMap;` with `METADATA`, `is_suffix_ambiguous`, `data`, `metadata`, `directory`,
  `decode` (signatures in R1 above).
- `enum DecodedPath { Data(Key), Metadata(Key), Directory(Key) }`.
- `AsyncOpenDALStore::reject_ambiguous`.

**Change:**

| Symbol | ~Line | Change |
|---|---|---|
| `key_to_path` | `:238` | `self.reject_ambiguous(key)?; PathMap::data(key)` |
| `key_to_path_metadata` | `:248` | `self.reject_ambiguous(key)?; PathMap::metadata(key)` |
| `path_to_key` | `:241` | `PathMap::decode(path)` mapped to the `Key` of each variant — **exhaustive match, no `_` arm** |
| `removedir` | `:408` | `PathMap::directory(key)` — **this line is the data-loss fix**; correct the doc comment (`:405-407`), which says "Files are not removed recursively" and is false |
| `listdir_keys_deep` | `:481` | `PathMap::directory(key)`; replace `sub.prefix_of_size(i).unwrap()` (`:488`) with a `filter_map` |
| `listdir` | `:452` | `PathMap::directory(key)`; decode entries through `DecodedPath`, **skipping** ones `decode` refuses |
| `makedir` | `:499` | `PathMap::directory(key)` |
| `is_supported` | `:514` | `!PathMap::is_suffix_ambiguous(key)` in place of the inline filename check |

**Decode order, the one thing to get exactly right:** strip a trailing `/` **before** the metadata
suffix, and strip the suffix from the **final segment only, once**. `HEAD`'s
`trim_matches('/').trim_end_matches(METADATA)` (`:242-243`) strips repeatedly.

**Add tests:** `PATHMAP01-06`, `SIBLING01-03`, `REMOVE01-02`, `FSREG01`, plus the `memory_store()` /
`fs_store()` helpers from Phase 3 (the `fs_store` guard removes its temp directory on drop).

**Validate:**
```bash
git stash && cargo test -p liquers-store sibling   # confirm they FAIL first
git stash pop && cargo test -p liquers-store
cargo test -p liquers-store keyabs16               # must pass UNCHANGED
```
Reproducing the failure before fixing it is not optional here: it is the difference between fixing
the bug and fixing something that resembles it.

---

### Step 3 — `key_prefix()` returns the configured prefix

**Files:** `liquers-store/src/opendal_store.rs`, `liquers-store/src/store_factory.rs`.

**Change:** `key_prefix` (`:296`) → `self.prefix.clone()`.

**In `store_factory.rs`:** enable the `key_prefix()` assertion in `opendal03`, and delete the comment
explaining why it was absent — it names this design and its reason is now gone.

**Add tests:** `PREFIX01`, `ROUTER01`, `SIBLING04`.

`SIBLING04` is the one that needs step 2 *and* step 3: a store with `prefix: data` sharing a backend
root with `database/`, asserting `keys()` returns only `data/…`. Remove either fix and it fails.

**Own commit, deliberately.** Phase 2 Q2: this is the change with the widest behavioural reach
(`AsyncStoreRouter::is_dir`, `store.rs:2053`, consults only `key_prefix()`), and a one-line revert is
the mitigation.

**Validate:**
```bash
cargo test -p liquers-store
cargo test -p liquers-core --lib            # router behaviour is exercised there
```

---

### Step 4 — `DirectoryIndex` in `liquers-core`

**New file:** `liquers-core/src/store_dir_index.rs`. **Register:** `pub mod store_dir_index;` in
`lib.rs` (alphabetical, after `store_config`).

**Move**, from `AsyncMemoryStore` — this is an extraction, so it is a move, not a rewrite:
`index_edges_for_key` (`:592`) → `DirectoryIndex::edges_for_key`; `get_or_create_children_map`
(`:606`), `add_key_to_index` (`:629`), `remove_key_from_index` (`:645`) → the corresponding
`DirectoryIndex` methods. The `dir_index` field (`:580`) becomes a `DirectoryIndex`.

**Add:** `from_keys`, `insert_directory`, `remove_directory`, `is_dir`, `children`, `child_keys`, and
the `explicit: scc::HashSet<Key>` field. `scc::HashSet` exists in `scc` 3.4 (`hash_set.rs`,
re-exported at `lib.rs:22`); `scc` is already an unconditional `liquers-core` dependency
(`Cargo.toml:66`) and compiles for wasm32, so nothing is added to any manifest.

**`AsyncMemoryStore` adopts it, with no behaviour change.** In particular `makedir` (`:888`) stays a
no-op in this commit — step 8 changes it, and keeping the two apart is what makes this commit
provably behaviour-preserving.

**Module rustdoc** states the rules at the point of use and points at `STORE_SEMANTICS.md`
(Phase 5).

**Add tests:** `DIRIDX01-08`.

**Validate:**
```bash
cargo test -p liquers-core --lib store_dir_index
cargo test -p liquers-core --lib store::tests::memdir   # MEMDIR01-05 pass UNCHANGED
git diff --stat HEAD~1 -- liquers-core/src/store.rs     # tests must show 0 changed lines
```
The third command is the extraction's real gate: **if `MEMDIR01-05` needed editing, the extraction
changed behaviour.** Stop and find out why rather than adjusting the test.

---

### Step 5 — The `contains` trait default falls back to `is_dir`

**File:** `liquers-core/src/store.rs`, `AsyncStore::contains` (`:442`).

```rust
async fn contains(&self, key: &Key) -> Result<bool, Error> {
    key.as_absolute()?;
    self.is_dir(key).await
}
```

`key.as_absolute()?` stays **first**, so the relative-key refusal is unaffected.

**Not added:** `directory_metadata_includes_children` — see R2.

**Add tests:** `TRAITDEF01`.

**Validate:**
```bash
cargo test -p liquers-core --lib keyabs17     # must pass UNCHANGED
cargo test -p liquers-core --lib
cargo test -p liquers-lib --lib --tests
bash scripts/check-build-matrix.sh
cargo clean && cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles
```
The wasm loop runs **once, here** — this is the commit that can reach `liquers-web`. Budget the
`cargo clean` (`CLAUDE.md`: the native and wasm loops do not share a `target/`).

---

### Step 6 — OpenDAL supplies its directory truth and inherits the rest

**File:** `liquers-store/src/opendal_store.rs`.

**Add:** `async fn has_children(&self, key: &Key) -> Result<bool, Error>` — one bounded listing of
`PathMap::directory(key)`, testing `!entries.is_empty()`. **Never a count**: `limit` is a page-size
hint and returned two entries for `limit(1)` on the memory backend.

**Change:**

- `is_dir` (`:427`): `stat` first; `Err(e) if e.kind() == ErrorKind::NotFound` → `has_children`;
  any other `Err` propagates. Two arms, not `is_err()` — an S3 403 must **not** be reported as "not
  a directory". `opendal::ErrorKind` is foreign and `#[non_exhaustive]`, so this is the one
  permitted catch-all; say so in a comment.
- `contains` (`:414`): data, else metadata, else `is_dir`.
- `get_metadata` (`:318`): the `KeyNotFound` branch consults `has_children` and returns
  `Metadata::MetadataRecord(self.default_metadata(key, true))` — the same value the
  `stat().is_dir()` branch already returns, so the two cannot diverge.

**Add tests:** `DIR01-03`, and **uncomment** `test_opendal_subdir`'s block (`:663`), deleting its
apology — "memory backend does not support directories explicitly, so not everything works as it
should" — which was the bug, written down and tolerated. Its `keys().len() == 3` will need
re-deriving against the corrected behaviour.

**Validate:**
```bash
cargo test -p liquers-store
cargo test -p liquers-lib --lib --tests
```

---

### Step 7 — Deletions and hygiene

**File:** `liquers-store/src/opendal_store.rs` (+ one line in `store_factory.rs`).

Split into **two commits**, because one is a pure deletion and should be reviewable as such:

**7a — deletions.**
- Delete `make_sub_dirs` (`:277`), its calls in `set` (`:362`) and `set_metadata` (`:379`), and the
  two `//TODO: create_dir` markers above them. It has never created a directory on any backend; the
  remaining `unwrap()` (`:279`) goes with it.
- Delete the commented-out synchronous `OpenDALStore` block (`:16-218`) — gate decision Q3. 200
  lines, cannot compile, holds the issue's other two `//TODO: create_dir` citations.

**7b — hygiene and the two folded-in issues.**
- Replace the stale `FIXME` (`:340`) with what is true: *"Directory children are deliberately not
  populated here — `listdir_asset_info` walks the whole subtree."*
- Drop the unused `Store` import (`:8`) and the unnecessary `mut` (`:339`).
- `test_opendal_localfs` (`:705`): `panic!` in the `else` branch; assert `names` contains
  `"opendal_store.rs"`. Closes `OPENDAL-LOCALFS-TEST-SILENT-ON-WRONG-VALUE-TYPE`.
- `store_factory.rs:22`: gate the `AsyncOpenDALStore` import and its uses on
  `#[cfg(all(feature = "opendal", feature = "async_store"))]`. Closes
  `STORE-OPENDAL-WITHOUT-ASYNC-STORE-BROKEN`.

**Validate:**
```bash
cargo build -p liquers-store 2>&1 | grep -c warning     # expect 0
cargo check -p liquers-store --no-default-features --features opendal   # newly buildable
cargo test -p liquers-store
bash scripts/check-build-matrix.sh
```

---

### Step 8 — `AsyncMemoryStore::makedir` records the directory

**Separate from step 4 on purpose**: one behaviour change, one commit, visible in the diff.

**File:** `liquers-core/src/store.rs`, `makedir` (`:888`).

```rust
async fn makedir(&self, key: &Key) -> Result<(), Error> {
    let key = key.as_absolute()?;
    self.dir_index.insert_directory(key).await;
    Ok(())
}
```

`removedir` calls `remove_directory` alongside its per-key cleanup.

**Flip `MEMDIR04`** to assert the correct behaviour, replacing the comment that named the issue with
one naming the commit that fixed it.

**Validate:**
```bash
cargo test -p liquers-core --lib
cargo test -p liquers-lib --lib --tests
```

Then set `CORE-ASYNC-MEMORY-STORE-MAKEDIR-DOES-NOTHING` to `status: closed` at Phase 5, not here.

## Testing Plan

### Per-step gates

Every step ends green on its own commands (above) before the next begins. Three gates are absolute:

1. **`keyabs16` and `keyabs17` pass unchanged**, at every step. They are the key-absoluteness guards;
   editing either during this work means the guard moved.
2. **`MEMDIR01-05` pass unchanged after step 4.** The extraction's proof.
3. **`SIBLING01-03` fail before step 2 and pass after.** Reproduce, then fix.

### Full-suite checkpoints

| After step | Command | Cost |
|---|---|---|
| 3 | `cargo test -p liquers-store && cargo test -p liquers-core --lib` | fast |
| 5 | `bash scripts/check-build-matrix.sh` | ~cargo check × 11 |
| 5 | `cargo clean && cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles` | rebuild; **once** |
| 7 | `cargo test -p liquers-lib --lib --tests` (the default loop) | ~3 min cold |
| 8 | all of the above | final |

**Disk.** `CLAUDE.md`'s budget applies: the native loop is ~4.2 GB and the wasm loop needs its own
`target/`, so `cargo clean` between them is mandatory, not tidiness. If a build reports "No space
left on device", `cargo clean` and re-run — deletes still succeed while writes fail.

**Not needed:** `export-command-registry`. No command is added, removed or changed (Phase 2,
Relevant Commands), so `specs/command_registry.yaml` and `registry_export` are untouched.

### What is not covered, and is said so rather than implied

- **No remote object store is exercised.** `memory` and `fs` are the two shapes available offline,
  and they differ in the way that matters (directory objects present/absent). OpenDAL's
  prefix-versus-directory semantics are backend-independent by design, and any divergence on S3 would
  be in the safe direction — a narrower scope than today's. An S3 integration test needs credentials
  and is out of scope.
- **`FetchStore` and `LocalStorageStore` are not migrated**, only kept compiling and passing.
- **Cross-operation atomicity is not tested** because it is not promised (Phase 3, corner cases).

## Agent Assignment

The workflow specifies capability tiers per step. This host executes them **in the primary agent**,
which `SKILL.md`'s Host Compatibility section permits; the tiers are recorded because they say how
much judgement each step needs, which is useful whoever runs it.

| Step | Tier | Skills | Knowledge the executor needs |
|---|---|---|---|
| 1 | Sonnet | `liquers-unittest` | Phase 3's `MEMDIR` table; `store.rs` `AsyncMemoryStore` (`:578-900`) and its existing tests (`:2162-2237`). Judgement: the tests must describe `HEAD`, including behaviour known to be wrong |
| 2 | **Opus** | `rust-best-practices`, `liquers-unittest` | Phase 1 Appendix A and B; Phase 2 §1 and R1; `opendal_store.rs` in full; OpenDAL's `create_dir`/`remove_all`/`list_with` contracts. **The highest-judgement step: the P0, the decode order, and the one place a mistake re-introduces data loss** |
| 3 | Haiku | `rust-best-practices` | Phase 2 §2; `store.rs` router (`:1909-2160`); `store_factory.rs` `opendal03`. Three lines plus tests |
| 4 | **Opus** | `rust-best-practices`, `liquers-unittest` | Phase 2 §3a; `AsyncMemoryStore`'s index in full; `scc` semantics. **An extraction that must not change behaviour, guarded by step 1** |
| 5 | Sonnet | `rust-best-practices` | Phase 2 Trait Implementations and R2; `keyabs17`'s body; which stores override `contains` |
| 6 | Sonnet | `rust-best-practices`, `liquers-unittest` | Phase 2 §3b; `opendal_store.rs` `get_metadata`/`is_dir`/`contains`; the `limit`-is-a-hint measurement |
| 7 | Haiku | — | Phase 2 §5, §6; the two folded-in issue files. Mechanical, but 7a deletes 200 lines and must delete exactly those |
| 8 | Haiku | `rust-best-practices` | `CORE-ASYNC-MEMORY-STORE-MAKEDIR-DOES-NOTHING`; step 4's `DirectoryIndex` |

Steps 2 and 4 are the two that would be worth a second reader if one is available.

## Rollback Plan

Each commit reverts alone; none leaves the tree in a state that needs a migration, because nothing
is persisted and the on-disk layout is unchanged by construction.

| Step | Revert | Consequence of reverting |
|---|---|---|
| 1 | `git revert` | tests only; the extraction loses its guard, so revert step 4 too |
| 2 | `git revert` | **the data-loss bug returns.** Only if the fix itself proves wrong |
| 3 | `git revert` — one line | router aggregation returns to `HEAD`'s behaviour. The designated first response to any routing regression in the field |
| 4 | `git revert` | `AsyncMemoryStore` returns to its private index; step 6 depends on nothing here, so it stands |
| 5 | `git revert` — three lines | the `contains` default returns to `Ok(false)`; no in-tree store is affected |
| 6 | `git revert` | defect 4 returns; steps 2-3 stand |
| 7a | `git revert` restores the deleted code verbatim | recovers `make_sub_dirs` and the dead block if either is wanted |
| 8 | `git revert` | `makedir` returns to a no-op; flip `MEMDIR04` back |

**Ordering constraints on revert:** 6 depends on 2 (`PathMap::directory`) and on 5 only for the
`contains` default's presence, not its behaviour. 8 depends on 4. 4 depends on 1 for its guard.
Nothing else is coupled.

**The one irreversible-feeling change is 7a**, a 200-line deletion — and it is not irreversible:
`git revert` restores it verbatim, and it is commented-out code that cannot compile, so nothing
depends on it.

## Phase 5 Entry Criteria

Phase 5 starts when **all** of these hold:

1. Steps 1-8 are implemented, committed, and every command in the Testing Plan passes — including
   `check-build-matrix.sh` and the wasm loop, each run at least once after step 5.
2. `keyabs16`, `keyabs17` and `MEMDIR01-05` pass **unchanged** (`MEMDIR04` excepted, flipped by
   step 8 alone).
3. No `unwrap()` or `expect()` remains in `opendal_store.rs` outside tests, and
   `cargo build -p liquers-store` emits zero warnings.
4. Every review comment on the PR is answered or incorporated.
5. Nothing in the design's scope is left undone without an issue recording it.

Phase 5 then delivers:

- `phase5-documentation.md` — one to three pages: what was implemented, deviations and why, issues
  filed, learning. The Corrections log in Phase 3 is its raw material.
- **`specs/reference/STORE_SEMANTICS.md`** — the store behavioural contract, written against what
  shipped. Content list in Phase 3's "Usage, Meaning, and Connections", plus the rows of
  `STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE`'s divergence table that this work settles
  (1, 2, 3, 4, 9, 11 — and 5 and 6, which are documentation-only: `removedir` **is** recursive,
  contrary to its doc comment, and is a no-op on an absent directory). Rows 7, 8 and 10 name their
  own open issues rather than being answered.
- `specs/reference/STORE_CONFIG_FSD.md` — a cross-link, plus a `## History` row and a `reviewed:`
  bump.
- Issue closures: `STORE-OPENDAL-SLASH-HANDLING`, `CORE-DIRECTORY-INDEX-NOT-SHARED`,
  `CORE-ASYNC-MEMORY-STORE-MAKEDIR-DOES-NOTHING`, `OPENDAL-LOCALFS-TEST-SILENT-ON-WRONG-VALUE-TYPE`,
  `STORE-OPENDAL-WITHOUT-ASYNC-STORE-BROKEN`, each with a resolution note.
- **Deliberately left open, each with a reason on the issue itself:**
  `STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE` (the umbrella, now carrying the full
  eleven-row divergence table), `CORE-ERROR-STORE-NAME-NOT-STRUCTURED` (R1's real fix),
  `CORE-ASYNC-MEMORY-STORE-IS-SUPPORTED-IGNORES-PREFIX`,
  `CORE-STORE-KEYS-MEANS-TWO-DIFFERENT-THINGS`. `STORE_SEMANTICS.md` answers the rows this work
  touches; the umbrella issue owns completing it.
- Follow-up recorded on `CORE-DIRECTORY-INDEX-NOT-SHARED`: migrating `FetchStore` and
  `LocalStorageStore` to `DirectoryIndex`.
- `specs/README.md` §Stores: the design moves `designing` → `documented`; `STORE_SEMANTICS.md` is
  linked.
- `DESIGN.md`: `status: complete`, `phase` removed. `specs/index.csv` regenerated.

## Review Record

**Against Phase 1.** Every acceptance criterion has a step and a test: 1 (sibling safety) → step 2,
`SIBLING01-04`; 2 (one mapping, round-trip) → step 2, `PATHMAP01-06`; 3 (`key_prefix`) → step 3;
4 (directory fallback in core, `AsyncMemoryStore` adopting it) → steps 4-6, `DIRIDX01-08`,
`MEMDIR01-05`, `DIR01-03`; 5 (markers, dead code, no `unwrap`, no warnings) → step 7 and entry
criterion 3; 6 (no change to what worked) → `FSREG01`. Non-goals hold: no conformance suite, no
`liquers-web` migration, no `path_map.rs`.

**Against Phase 2.** Every §maps to a step. Two documented refinements (R1, R2) change signatures
Phase 2 specified — both are recorded here rather than silently implemented, and R2 *removes*
proposed API rather than adding it.

**Against Phase 3.** The sequencing table is Phase 3's, unchanged, with step 7 split into 7a/7b for
reviewability. Every named test appears in a step. Findings 1 and 2 are honoured: characterization
before extraction, `makedir` as its own commit.

**Against the codebase.** `Error::key_not_supported`'s signature (`error.rs:307`) and
`key_not_absolute`'s (`:319`) were read, which is what produced R1. Every store's `get_metadata`
override was checked, which is what produced R2. `scc::HashSet`'s existence and `scc`'s
unconditional presence in `liquers-core` were verified before the plan depended on them.

**Rust review (`rust-best-practices`).** No `unwrap`/`expect` outside tests — the work removes the
two that exist. All errors via typed constructors; no `Error::new`; no new error type — R1 exists
precisely to avoid inventing one. Exhaustive matching on `DecodedPath`; the single wildcard is over
foreign `#[non_exhaustive]` `opendal::ErrorKind` and is commented. Async default preserved; the one
new `async fn` awaits. `AsyncStore` stays object-safe: no generic method, no `Self` by value, and R2
means no method is added at all. Dependency flow respected — `liquers-store` gains a dependency on a
`liquers-core` module, never the reverse. `PathMap` is a stateless unit type; `DirectoryIndex` uses
the `Arc`/`scc` shape it is extracted from rather than a new one. No `println!`.

**Certainty.** High on steps 2-3, 5-8: the mechanisms were executed against both backends, and the
sizes are small. Moderate on step 4, which is an extraction whose faithfulness is argued from the
code and *proved* by step 1's tests — which is why step 1 exists and why the diff gate on the test
module is written into the validation commands.

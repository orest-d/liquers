# Phase 4: Implementation Plan — Sidecar-colliding keys refused by the path builders

## Overview

Nine steps, in an order chosen so that **every step compiles and its tests pass before the next
begins**. The predicate lands first with its own test; each store then adopts it; the conformance
bookkeeping goes last, because it is the step that proves the others worked.

The ordering is not arbitrary. Step 3 is the fix for the filed issue and could be done alone — but
doing it alone is the *half-fix* Example 2 shows to be worse than the bug, so steps 3 and 4 are
written as one unit and neither is a stopping point. Similarly, step 6 removes a `pub` function
from `liquers-store`, so it is isolated in its own commit and is the only step with a distinct
rollback.

Nothing here changes a trait, so no implementor outside these three stores is touched. Every step's
validation command is one a contributor runs locally in under a minute, except step 9.

**Preflight recheck after the merge from `main` (2026-09-03).** `main` brought
`specs/design/error-store-name-payload/` (`CORE-ERROR-STORE-NAME-NOT-STRUCTURED`, `in_review`,
phase `implementation`), which is adjacent: it plans to add store provenance to `ErrorPayload` and
touches `liquers-core/src/store.rs`. Its Phase 2 states that **"existing constructors populate it
without changing their signatures"**, so `Error::key_not_supported(key, store_name)` is stable and
this plan is unaffected. It is a *merge-conflict* risk in the same file, not a design conflict;
whichever lands second rebases. No other open `core/store` issue blocks any step, and every line
number cited in Phases 2 and 3 was re-verified against `store.rs` after the merge.

## Implementation Steps

### Step 1 — `ReservedNames` and the two suffix constants

**File:** `liquers-core/src/store.rs`.
**Placement is load-bearing:** insert **before line 874**, which is where
`#[cfg(not(target_arch = "wasm32"))]` begins guarding `AsyncFileStore`. Inside that region the type
would vanish from the wasm32 library build for no reason — it is pure string comparison.

```rust
pub const METADATA_SUFFIX: &str = ".__metadata__";
pub const LOCK_SUFFIX: &str = ".__lock__";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReservedNames { suffixes: &'static [&'static str] }

impl ReservedNames {
    pub const fn new(suffixes: &'static [&'static str]) -> Self { Self { suffixes } }
    pub fn is_reserved_name(&self, name: &str) -> bool;
    pub fn is_reserved_key(&self, key: &Key) -> bool;
}
```

Bodies are pinned in Phase 2 §Function Signatures. Doc comments matter here more than usual: this
type is the answer a future store implementer will find, so it carries *why* it is a predicate
rather than a fallible function, and why both name forms are reserved.

**Validate:** `cargo check -p liquers-core`

### Step 2 — `reserved01`, and the module that will hold the rest

**File:** `liquers-core/src/store.rs`, a new `mod reserved_name_tests` after `mod
key_absolute_tests` (line 2515). Carries `use crate::error::ErrorType`, `use crate::parse::parse_key`,
its own `unique_temp_dir` (a third copy of six lines, following the precedent of the module above),
and the `assert_not_supported<T>` helper. Then `reserved01`.

Test-first for the predicate specifically, because `reserved01`'s negatives are the part most
easily got wrong and the cheapest to get feedback on.

**Validate:** `cargo test -p liquers-core --lib reserved01`

### Step 3 — `AsyncFileStore` adopts the predicate

**File:** `liquers-core/src/store.rs`, `impl AsyncFileStore` (882) and its `AsyncStore` impl (985).

1. Replace `const METADATA` (883) and `const LOCK` (884) with
   `const RESERVED: ReservedNames = ReservedNames::new(&[METADATA_SUFFIX, LOCK_SUFFIX]);`
2. Add `fn reject_reserved(&self, key: &Key) -> Result<(), Error>`, raising
   `Error::key_not_supported(key, &self.store_name())`. Private; mirrors
   `AsyncOpenDALStore::reject_ambiguous`.
3. Call it in `key_to_path` (899), `key_to_path_metadata` (908) and `key_to_lock_path` (915),
   **after** `as_absolute()?` in each — the order `reserved05` pins.
4. `is_supported` (1224): replace the two `filename()` checks with
   `!Self::RESERVED.is_reserved_key(key)`.
5. `listdir` (1209): replace the two `ends_with` calls with
   `!Self::RESERVED.is_reserved_name(&name)`.

Every other use of `Self::METADATA` / `Self::LOCK` in the impl must be updated to `METADATA_SUFFIX`
/ `LOCK_SUFFIX`; the compiler finds them all, since the constants are being removed rather than
kept as aliases.

**Validate:** `cargo test -p liquers-core --lib` — `keyabs08`, `keyabs09` and
`test_async_file_store_basic` must still pass. **Do not stop here** (see step 4).

### Step 4 — the `AsyncFileStore` tests

**File:** same module as step 2. `reserved02`, `reserved03`, `reserved05`, `reserved06`,
`reserved08`, verbatim from Phase 3.

`reserved06` is the one that justifies step 3.5 (the listing filter) existing; if step 3 were done
without it, `reserved06` is the test that fails, and it fails by the store becoming unlistable.

**Validate:** `cargo test -p liquers-core --lib reserved`

### Step 5 — `FileStore`, the synchronous twin

**File:** `liquers-core/src/store.rs`, `impl FileStore` (1241) and its `Store` impl.

Identical to step 3, **minus the lock**: `ReservedNames::new(&[METADATA_SUFFIX])`, guards in
`key_to_path` (1255) and `key_to_path_metadata` (1264), `is_supported` (1461), `listdir` (1444).
There is no `key_to_lock_path` and no lock suffix — that asymmetry is the point of `reserved04`.

Then `reserved04` and `reserved07`.

**Validate:** `cargo test -p liquers-core --lib reserved`

### Step 6 — `PathMap`, and one `pub` function removed

**File:** `liquers-store/src/opendal_store.rs`. **Its own commit**, because it is the only step that
removes public API.

1. Replace `const METADATA` (71) with
   `pub const RESERVED: ReservedNames = ReservedNames::new(&[METADATA_SUFFIX]);` — importing both
   from `liquers_core::store`.
2. **Delete `pub fn is_suffix_ambiguous`** (79-82).
3. `reject_ambiguous` (144) and `is_supported` (521) call `PathMap::RESERVED.is_reserved_key(key)`.
4. `listdir` (459): after the `PathMap::decode` guard, `continue` on a reserved decoded key.
5. `listdir_keys_deep` (492): the same guard, **before** `list.extend(…)` inserts the prefixes, so a
   reserved interior segment takes its whole subtree with it.
6. Rewrite the module doc comment at 57-65, which names the deleted function as "the rule".
7. Rename `pathmap03_suffix_ambiguous_keys_are_refused_everywhere` →
   `pathmap03_reserved_keys_are_refused_everywhere` and
   `pathmap07_directory_form_refuses_suffix_ambiguous_keys` →
   `pathmap07_directory_form_refuses_reserved_keys`, **keeping the IDs**, and extend both with the
   newly reserved shapes. Add `pathmap08`.

**Validate:** `cargo test -p liquers-store`

### Step 7 — the `C2` conformance fixture

**File:** `liquers-core/tests/store_conformance_CONF.rs`, `c2_async_file_store` (84).

Drop the `AllowedFailure` for `sidecar03`; add
`.with_unsupported_shape(parse_key("collide.__metadata__").expect("key"))`; rewrite the comment to
say why the entry is gone. Verbatim from Phase 3.

**This step cannot be skipped or deferred:** `H5` fails the assertion when an allowed rule starts
passing, so after step 3 the suite is *red* until this lands. That is deliberate — it is what makes
a fixed issue force its own bookkeeping out — but it means steps 3-7 are one landing.

**Validate:** `cargo test -p liquers-core --features store-conformance --test store_conformance_CONF`

### Step 8 — the issue, and the index

Set `CORE-FILE-STORE-WRITES-METADATA-COLLIDING-KEYS` to `status: closed` with a resolution note,
and correct its `complexity: M` → `L` (the change reaches `liquers-store`, which the issue did not
anticipate). Regenerate with `python3 scripts/docs_index.py`.

**Validate:** `python3 scripts/docs_index.py --check` — 0 errors.

### Step 9 — the full matrix

**Validate:** the four commands in Phase 3's Test Plan, ending with
`bash scripts/check-build-matrix.sh` (11 configurations; the wasm32 core row is the one that proves
step 1's placement was right).

## Testing Plan

| When | Command | Proves |
|---|---|---|
| After 1 | `cargo check -p liquers-core` | the type compiles and is outside the wasm gate |
| After 2 | `cargo test -p liquers-core --lib reserved01` | the predicate, including its negatives |
| After 3, 4, 5 | `cargo test -p liquers-core --lib` | both file stores, and no `keyabs` regression |
| After 6 | `cargo test -p liquers-store` | OpenDAL, `pathmap01`-`pathmap08` |
| After 7 | `cargo test -p liquers-core --features store-conformance --test store_conformance_CONF` | `C1`-`C5`; `C2` with no allowed failures |
| After 8 | `python3 scripts/docs_index.py --check` | the documentation index |
| After 9 | `bash scripts/check-build-matrix.sh` | 11 configurations including wasm32 |

**The one expected report change:** `C2` loses its `sidecar03` allowed failure, and `prefix03` and
`sibling05` move from "not run" to passing. Any other change in any suite is a regression to
investigate, not to accommodate.

**Known-red window.** Between step 3 and step 7 the conformance suite fails on `H5`. Land steps
3-7 together; do not push a branch parked inside that window.

## Agent Assignment

| Step | Tier | Skills | Knowledge it must be given |
|---|---|---|---|
| 1 | general-purpose | `rust-best-practices` | Phase 2 §Data Structures and §Function Signatures; `store.rs:860-890` for the placement boundary |
| 2 | focused | `liquers-unittest` | Phase 3 §Unit Tests preamble and `reserved01`; `store.rs:2514-2540` for the module precedent |
| 3 | general-purpose | `rust-best-practices` | Phase 2 §Function Signatures; `store.rs:881-1235`; `opendal_store.rs:140-180` as the worked model |
| 4 | focused | `liquers-unittest` | Phase 3 `reserved02`, `03`, `05`, `06`, `08`; the step-2 module |
| 5 | general-purpose | `rust-best-practices`, `liquers-unittest` | Phase 2's `FileStore` block; `store.rs:1241-1470`; Phase 3 `reserved04`, `reserved07` |
| 6 | **deepest** | `rust-best-practices` | Phase 2 §Function Signatures `PathMap` block; the whole of `opendal_store.rs`. **Deepest tier because it deletes public API and edits two listing loops whose decode order is subtle** — `pathmap02` and `pathmap05` exist to protect that order |
| 7 | focused | — | Phase 3 §Conformance; `store_conformance_CONF.rs:84-122`; `store_conformance/mod.rs:679` for `H5` |
| 8 | focused | — | `DOCS_STRUCTURE_GUIDE.md` §4.3; the issue file |
| 9 | focused | — | Phase 3 §Test Plan |

Steps 1-2, 3-4 and 5 are sequential; step 6 is independent of 3-5 and may run in parallel with
them; steps 7-9 need everything before them.

## Rollback Plan

Each step is its own commit, so `git revert` of a single commit is the unit of rollback. Two are
not clean reverts:

| Step | Risk | Rollback |
|---|---|---|
| 3-7 | Reverting step 3 alone leaves `C2` asserting against a fixed bug that is no longer fixed — `H5` red in the other direction | Revert 3, 4, 5 and 7 together; the branch is only ever pushed with all of them |
| 6 | `PathMap::is_suffix_ambiguous` is `pub`; anything out-of-tree calling it breaks | Restore it as a two-line wrapper over `PathMap::RESERVED.is_reserved_key`, rather than reverting the whole step. Cheaper than the alternative and keeps the widened rule |

Nothing here migrates data or changes a serialized format, so there is no state to unwind — a
revert restores the previous behaviour exactly, including, deliberately, the bug.

## Phase 5 Entry Criteria

Phase 5 begins when **all** of these hold:

1. Steps 1-9 are complete and every validation command passes.
2. `C2` reports no allowed failures, and `prefix03` / `sibling05` are passing rather than "not run".
3. `scripts/check-build-matrix.sh` is clean across all 11 configurations.
4. Every review comment on the PR is answered or incorporated.

Phase 5 then delivers, per the Phase 2 documentation architecture:

- `STORE_SEMANTICS.md` §8 restated as *reserved names, in any segment, declared by the store's
  metadata layout*, with a `## History` row and `reviewed:` bump. **Plus** the point carried from
  the Phase 3 review: `get` repairs metadata it cannot parse, which the recovery path depends on
  and which the contract does not mention anywhere.
- `STORE_IMPLEMENTATION_GUIDE.md` §"The key space" extended with the technique, the listing filter
  as the third caller, and the recovery note for a store that already holds an orphan.
- `CORE-FILE-STORE-WRITES-METADATA-COLLIDING-KEYS` closed (step 8 does this early, since the
  suite's own bookkeeping depends on it).
- `phase5-documentation.md`: what was built, what was left, and the two issues this design filed —
  `STORE-METADATA-LAYOUT-HARDCODED-PER-STORE` and
  `CORE-FILE-STORE-LISTDIR-DROPS-METADATA-ONLY-KEYS`.

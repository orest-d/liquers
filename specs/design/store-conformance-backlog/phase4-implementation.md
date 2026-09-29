# Phase 4: Implementation Plan - Store Conformance Backlog

## Overview

**Feature:** Store conformance backlog: make every store, the contract and the test IDs agree
(seven issues, listed in `DESIGN.md`).

**Architecture:** Areas A–F of Phase 2 are implemented against the tests of Phase 3.
- Areas A–D and E: one shared default body changes (`AsyncStore::get_asset_info`), and the rest
  are store-local or rule-local changes.
- Area C: one documented protocol break (`getMetadata` → `null`).
- Area F: a test correction only.

**Estimated complexity:** Medium. Many small steps across three crates, plus a browser test loop.

**Estimated time:** 10–14 hours for an experienced Rust developer, including the three separate
`liquers-web` loops.

**Prerequisites:**
- Phases 1–3 approved; no open questions (Phase 2 preflight found no blockers).
- No new dependencies.
- The Phase 3 test code is the specification; every step lands its tests with its code.

**Branch and commits:** work on the design branch. There is one commit per step. A step that
touches `STORE_SEMANTICS.md` or `STORE_IMPLEMENTATION_GUIDE.md` adds a `## History` row and bumps
`reviewed:` in the **same commit** (`CLAUDE.md`; `DOCS_STRUCTURE_GUIDE.md` §9.2).

**Why some documentation lands before Phase 5.** Test `D1` (`conformance_docs_CONF.rs`) fails when
a registered rule is not cited in both the contract and the guide. Steps 4 and 6 therefore edit
those documents for `dir07` and `sidecar04`, where the code change happens. Phase 5 then reviews
all documentation against the finished code and writes the remaining prose.

**Gated test files pass vacuously without their feature.** Both conformance test files carry a
file-level cfg: `liquers-core/tests/store_conformance_CONF.rs` needs `store-conformance`, and
`liquers-store/tests/store_conformance_CONF.rs` needs `store-conformance` and `opendal`. Built
without the feature, such a file compiles to **zero tests and reports `ok`**. Every command below
therefore passes `--features store-conformance`. A validation counts only if its output shows a
non-zero `running N tests` line.

**Disk:** follow `CLAUDE.md` §Building and testing. The native loop is
`cargo test -p liquers-lib --lib --tests` plus the crate-specific commands below, with
`CARGO_INCREMENTAL=0`. Run `cargo clean` before the wasm and browser loops.

## Implementation Steps

### Step 0: Baseline evidence

**Files:** the seven issue files in `specs/issues/`, which move from `status: draft` to
`status: in_progress` (`DOCS_STRUCTURE_GUIDE.md` §4.3 allows `draft → closed` only for work found
already fixed at triage). No code changes.

**Evidence** is kept **outside the repository**, in a scratch directory (`$EVIDENCE`, for example
the session scratchpad), and is never committed. Phase 5 summarises it in
`phase5-documentation.md`.

**Action:** record the conformance reports at HEAD so every later change in them is attributable.

**Validation:**
```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-core --features store-conformance \
  --test store_conformance_CONF -- --nocapture 2>&1 | tee $EVIDENCE/baseline-core.txt
CARGO_INCREMENTAL=0 cargo test -p liquers-store --features store-conformance --test store_conformance_CONF -- --nocapture \
  2>&1 | tee $EVIDENCE/baseline-store.txt
# Expected: all pass; dir07 reported Blocked wherever Directories is declared.
```

**Rollback:** `git revert` the issue-status commit. The evidence is outside the repository.

**Agent Specification:**
- **Model:** haiku
- **Skills:** none
- **Knowledge:** `CLAUDE.md` §Building and testing
- **Rationale:** mechanical command execution

---

### Step 1: Land the Phase 3 examples, failing first

**Files:**
- new `liquers-core/tests/store_directory_children_STORE.rs`, verbatim from Phase 3 Example 1
- `liquers-web/tests/store_js_STORE.rs`: append `STORE12a`, `STORE12b` and `STORE12c`, verbatim
  from Phase 3 Example 2

**Action:** add the tests exactly as written. Confirm that they compile and fail for the reasons
Phase 3 predicts:
- `file_store_…` fails at `listdir`.
- `STORE12a` fails at the directory assertion.
- `STORE12c` gets an empty record.

Record the failures in `$EVIDENCE/examples-at-head.txt`. Commit with the failing tests marked
`#[ignore = "store-conformance-backlog: fixed in step N"]`, so that each later step removes its own
`ignore` and every commit stays green.

**Validation:**
```bash
cargo test -p liquers-core --test store_directory_children_STORE -- --include-ignored
# Expected: the memory test passes; the file test fails on listdir
```
```bash
cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles \
  --test store_js_STORE --no-run
# Expected: STORE12a–c compile. They are run, and their ignores removed, in Step 7.
```

**Rollback:** `git revert` the commit.

**Agent Specification:**
- **Model:** haiku
- **Skills:** liquers-unittest
- **Knowledge:** `phase3-examples.md` Examples 1–2, and the style of `liquers-web/tests/store_js_STORE.rs`
- **Rationale:** copies approved code; no design judgment

---

### Step 2: Area A: one-level `get_asset_info` default

**File:** `liquers-core/src/store.rs`, `AsyncStore::get_asset_info` (currently around line 409)

**Action:** replace the default body. The signature is unchanged.

```rust
async fn get_asset_info(&self, key: &Key) -> Result<metadata::AssetInfo, Error> {
    if self.is_dir(key).await? {
        let mut info = self.default_metadata(key, true).get_asset_info();
        info.with_key(key.to_owned());
        info.is_dir = true;
        return Ok(info);
    }
    // `Metadata::get_asset_info` returns `Result` (legacy metadata can fail to convert);
    // `MetadataRecord::get_asset_info` above does not. This branch keeps today's body.
    let mut info = self
        .get_metadata(key)
        .await?
        .get_asset_info()
        .unwrap_or_else(|_e| AssetInfo::new());
    info.with_key(key.to_owned());
    info.is_dir = false;
    Ok(info)
}
```

Add the doc comment given in Phase 2 area A. Add these unit tests to the file's `tests` module:
- `default_directory_metadata_reads_one_level`
- `default_directory_asset_info_is_directory_shaped`

Both use a counting wrapper that forwards only the required methods, so that the trait defaults
run. Because the wrapper does not forward `get_metadata`, it cannot count it; the one-level test
counts `listdir(a/b)` and `get(a/b/c/leaf)` instead, and asserts both are zero. Do **not** change the synchronous `Store` trait.

**Validation:**
```bash
cargo test -p liquers-core --lib store::tests
cargo test -p liquers-core --features store-conformance --test store_conformance_CONF
# Expected: all pass; the reports are unchanged from the baseline apart from timing
```

**Rollback:** `git revert`. The change is self-contained in one default body.

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:**
  - Phase 2 area A
  - `store.rs`: `AsyncStore` defaults, and `AsyncMemoryStore` for the wrapper
  - `metadata.rs`: `MetadataRecord::get_asset_info` returns `AssetInfo`, not a `Result`
- **Rationale:** a shared default that every store inherits; the counting test needs care

---

### Step 3: Area A: OpenDAL populates `children`

**File:** `liquers-store/src/opendal_store.rs`, `get_metadata`: **both** directory branches
(`stat().is_dir()` around lines 307–312, and the `has_children` fallback around lines 333–335)

**Action:** add a private helper, `directory_metadata(&self, key)`, which returns
`default_metadata(key, true)` with `children = self.listdir_asset_info(key).await?`. Call it from
both branches. Replace the "deliberately not populated" comment with a citation of STORE_SEMANTICS
§2. Add the unit test `opendal_directory_metadata_lists_children` for **both** services: memory
takes the fallback branch and fs takes the `stat` branch.

**Validation:**
```bash
cargo test -p liquers-store --lib opendal
```

**Rollback:** `git revert`.

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:** Phase 2 area A; `get_metadata` in full, including the comment at lines 326–332;
  the existing OpenDAL unit-test helpers in the same file
- **Rationale:** two branches reached by different services; the helper keeps them aligned

---

### Step 4: Rule `dir07` goes live, with its contract text

**Files:**
- `liquers-core/src/store_conformance/rules/directories.rs` (`dir07`)
- `liquers-core/src/store_conformance/rules/mod.rs` (registry label)
- `liquers-core/src/store_conformance/mod.rs` tests (refutation wrappers)
- `specs/reference/STORE_SEMANTICS.md` §2 and "Why this document exists"
- `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` §9 notes

**Action:**
1. Rewrite `dir07`: it takes the parent's `get_metadata` record and compares the set of child
   filenames in `children` with `listdir(parent)`.
   - Equal: `Passed`.
   - Empty or different: `failed_at` naming both sets.
   - Legacy metadata: `Passed`, as today.

   The `Blocked` branch is deleted. The doc comment becomes "directory metadata populates `children`
   with the direct children only".
2. Change the registry label to match. Capabilities are unchanged.
3. Add the tests `refute_dir07_passes_one_level_children`, `refute_dir07_fails_empty_children` and
   `refute_dir07_fails_children_that_differ_from_listdir`. Use two wrappers in the `PrefixDeletingStore`
   style: one clears `children`, one appends a grandchild.
4. Replace the ⚠ bullet in `STORE_SEMANTICS.md` §2 with the one-level contract and its cost (width,
   not depth, is unbounded). Update the "two remain" sentence. Add a History row and bump `reviewed:`.
5. In the guide §9, drop "`dir07` blocked…" from the `AsyncMemoryStore` note. Add a History row and
   bump `reviewed:`. That table's rule counts and the `JsStore`/`LocalStorageStore` rows are left
   for Phase 5, which rewrites the whole table from the final reports and cites
   `STORE-GUIDE-STATUS-TABLE-HAS-NO-GENERATOR`.

**Validation:**
```bash
cargo test -p liquers-core --lib store_conformance
cargo test -p liquers-core --features store-conformance --test store_conformance_CONF \
  --test conformance_docs_CONF
cargo test -p liquers-store --features store-conformance --test store_conformance_CONF
# Expected: dir07 Passed in every suite that declares Directories, including OpenDAL memory (C6)
# and fs (C7) after Step 3; D1 green
```

**Rollback:** `git revert` restores the `Blocked` rule and the documents together.

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:**
  - Phase 2 area A
  - `rules/directories.rs` (`nested`, `create`, `failed_at`)
  - the `PrefixDeletingStore` test in `store_conformance/mod.rs`
  - `STORE_SEMANTICS.md` §2
  - `DOCS_STRUCTURE_GUIDE.md` §9.2
- **Rationale:** changes a rule's meaning, and its prose must match exactly

---

### Step 5: Area D: `AsyncFileStore::listdir` reports metadata-only keys

**File:** `liquers-core/src/store.rs`, `AsyncFileStore::listdir` (around lines 1312–1325)

**Action:**
1. For each entry name: if it ends with the metadata suffix, take the implied name (the entry with
   the suffix stripped); otherwise take the entry as it is.
2. Push the name only if `!Self::RESERVED.is_reserved_name(&name)` and it has not been pushed
   already (a `HashSet<String>` guard).
3. Use the existing suffix constant. Do not hard-code the string.

Remove the `#[ignore]` added in Step 1 from `file_store_lists_children_and_metadata_only_keys`. Add
these unit tests:
- `file_store_lists_a_metadata_only_key`
- `file_store_lists_data_and_sidecar_once`
- `file_store_skips_sidecars_implying_reserved_names`

Leave the synchronous `FileStore` untouched (Phase 2 preflight).

**Validation:**
```bash
cargo test -p liquers-core --lib store::tests
cargo test -p liquers-core --test store_directory_children_STORE
cargo test -p liquers-core --features store-conformance --test store_conformance_CONF
# Expected: all pass, and the reserved* and keyabs* unit tests stay green
```

**Rollback:** `git revert`.

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:**
  - Phase 2 area D
  - `STORE_SEMANTICS.md` §8
  - `ReservedNames` in `store.rs` (around line 947)
  - the `reserved01`–`reserved08` tests
- **Rationale:** reserved-name interaction; a mistake here re-opens a corruption defect

---

### Step 6: Rule `sidecar04`, with its contract text

**Files:**
- `liquers-core/src/store_conformance/rules/sidecar.rs`
- `rules/mod.rs`
- `store_conformance/mod.rs` tests
- `STORE_SEMANTICS.md` §8
- guide §8 table

**Action:**
1. Implement `sidecar04` exactly as Phase 2 area D specifies:
   - request `FreshNested { depth: 1 }` and call `require_absent`
   - call `set_metadata`: `KeyNotFound` → `Passed`; any other error → `e.into()`
   - `record_created`, then check `contains` and `listdir_keys(parent)`
2. Register it: `[StoredMetadata, Write]`, `CreateOnly`.
3. Add these tests:
   - `refute_sidecar04_passes_a_store_that_lists_metadata_only_keys`
   - `refute_sidecar04_accepts_a_refusal`
   - `refute_sidecar04_fails_a_store_that_hides_metadata_only_keys`
4. In §8, replace the "file stores do not yet report…" sentence with the rule and the permitted
   refusal, and add `sidecar04` to *Enforced by*.
5. In the guide's "Where each rule comes from" table, add `sidecar04` to the §8 row.
6. Add History rows and bump `reviewed:` in both documents.

**Validation:**
```bash
cargo test -p liquers-core --lib store_conformance
cargo test -p liquers-core --features store-conformance --test store_conformance_CONF \
  --test conformance_docs_CONF
cargo test -p liquers-store --features store-conformance --test store_conformance_CONF
# Expected: sidecar04 Passed for memory, file and OpenDAL; skipped by capability for the trait
# defaults; D1 green
```

**Rollback:** `git revert`.

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:**
  - Phase 2 area D
  - `rules/sidecar.rs` (`sidecar02` as the template)
  - the `rule!` macro in `rules/mod.rs`
  - `Fixture` and `KeyRequest` in `store_conformance/mod.rs`
  - D1
- **Rationale:** a new rule plus its documentation contract

---

### Step 7: Areas B and C: `JsStore` absence and directory metadata

**Files:**
- `liquers-web/src/store/js_store.rs`
- `liquers-web/src/typescript.rs`
- `liquers-web/tests/store_conformance_CONF.rs`
- `liquers-web/tests/store_js_STORE.rs` (remove the `ignore` markers)

**Action:**
1. `get_metadata`, in `getMetadata` mode: if the result is `null` or `undefined`, return
   `Error::key_not_found(key)` *before* calling `metadata_from_js`.
2. Directory fallback, in both modes: on `Err(e)` with `e.error_type == ErrorType::KeyNotFound`:
   - if `self.methods.is_dir` is `Some`, call it;
   - if the answer is truthy, return a `MetadataRecord` from `default_metadata(key, true)` with
     `children = self.listdir_asset_info(key).await?`;
   - otherwise return the original error.

   Every other error is returned unchanged.
3. Module docs: the protocol table states the `null` sentinel, the directory fallback and the
   `getMetadata` break. Update the TypeScript declaration as in Phase 3 Example 2.
4. Conformance stub: `get` returns `null` for a missing key. Delete all four `AllowedFailure`
   entries and their comments from `c10_js_store`, which now calls `report(...)` like the others.
5. Keep `metadata_from_js_value` unchanged, because the write direction depends on it.

**Validation** (after `cargo clean`):
```bash
cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles
# Expected: STORE12a–c pass; C10 conformant with no allowed failures; C8 unchanged
```
If any allowed failure is left behind, H5's stale-allowed-failure report catches it.

**Rollback:** `git revert`. The protocol break is contained in this one commit.

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:**
  - Phase 2 areas B and C
  - `js_store.rs` in full
  - `store/mod.rs` (`metadata_from_js_value`)
  - `CLAUDE.md` §liquers-web
- **Rationale:** a wasm-only protocol change; `!Send` async (`async_trait(?Send)`) and JavaScript
  interop

---

### Step 8: Area D′: `LocalStorageStore`, and C9 in a browser

**Files:**
- `liquers-web/src/store/local_storage.rs`
- `liquers-web/tests/store_conformance_CONF.rs`
- new `liquers-web/tests/store_conformance_browser_CONF.rs`

**Action:**
1. `removedir`: after `self.check(key)?`, return `Ok(())` when `!is_dir`.
2. `contains`, `is_dir` and `listdir` start with `self.check(key)?`.
3. `keys()` returns the de-duplicated union of three sets: `state.keys`, the `state.dirs` keys under
   the prefix, and the prefix itself.
4. The `rescan` arm for `EntryKind::Metadata` inserts the key and indexes its ancestors, exactly as
   the `Data` arm does. `set_metadata` updates `state.keys` and calls `index_key`.

   `RefCell` borrows are released before any `.await`.
5. Move `c9_local_storage_store` into the new file under
   `#![cfg(all(target_arch = "wasm32", feature = "browser-tests"))]` and
   `wasm_bindgen_test_configure!(run_in_browser);`, with its own `report` helper. Remove it and its
   doc comment from the Node file, and update that file's header comment.

**Validation** (after `cargo clean`):
```bash
cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles
CHROMEDRIVER=$(which chromedriver) cargo test -p liquers-web --target wasm32-unknown-unknown \
  --features browser-tests
# Expected: C9 conformant with absence03, keyshape01, keys02 and sidecar04 passing;
# the STORE07/08 localStorage tests still pass
```
If the chromedriver and Chromium major versions cannot be matched, use the `NO_HEADLESS=1` route
in `liquers-web/README.md` (around line 99). Record which route was used in
`$EVIDENCE/browser-run.txt`.

**Rollback:** `git revert`. The moved test goes back together with the store changes.

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:**
  - Phase 2 area D′
  - `local_storage.rs` in full (`State`, `rescan`, `index_key`, `EntryKind`)
  - `store_local_STORE.rs` for the `run_in_browser` pattern
  - `liquers-web/README.md` browser section
- **Rationale:** index consistency and a browser-only test loop

---

### Step 9: Area E: rename the colliding tests, and enforce the rule

**Files:**
- `liquers-store/src/opendal_store.rs` (12 renames)
- `liquers-core/src/store_conformance/mod.rs` (`sibling01_catches_…` → `refute_sibling01_catches_…`)
- `liquers-core/src/store.rs` (`traitdef01` doc comment)
- `liquers-core/tests/conformance_docs_CONF.rs`

**Action:**
1. Apply the rename table from Phase 2 area E. Names only; bodies and doc comments stay, except for
   ID mentions in the doc comments.
2. `traitdef01` gains the "same contract as rule `dir05`, which C4 cannot run" sentence.
3. In D1, extract the existing family derivation (the `trim_end_matches` over `rules()`) into
   `fn rule_families() -> BTreeSet<String>`.
4. Add the tests `owned_rule_families_come_from_the_registry` and
   `no_unit_test_uses_an_owned_rule_id`. The second walks the `src/` and `tests/` directories of
   `liquers-core`, `liquers-store` and `liquers-web` with `std::fs` recursion. Collect the matches
   of `fn <family><digits>_` into a list of `file:line` and assert that it is empty. No regex
   dependency: match on `"fn "` followed by a family prefix and digits, by hand. Like
   `specs_dir()`, the scan skips with an `eprintln!` warning when a sibling crate directory is
   absent (a packaged crate).

**Validation:**
```bash
cargo test -p liquers-store --lib
cargo test -p liquers-core --features store-conformance --test conformance_docs_CONF
# Expected: green. Before the renames, the new D1 test lists all 13 names
# (check this first, then rename).
```

**Rollback:** `git revert`.

**Agent Specification:**
- **Model:** haiku for the renames; sonnet for the D1 test
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:**
  - Phase 2 area E rename table
  - `conformance_docs_CONF.rs`
  - `CLAUDE.md` rule on no `println!` (use `eprintln!` in tests too)
- **Rationale:** mechanical renames; a small new scanner

---

### Step 10: Area E: deletion trials

**Files:** `liquers-store/src/opendal_store.rs`, only if a trial passes.

**Action:** for each row of the Phase 3 deletion-candidates table:
1. On a local scratch commit, apply the break.
2. Run `cargo test -p liquers-store --features store-conformance --test store_conformance_CONF`.
3. Confirm the named rule reports `Failed`.
4. Discard the scratch commit and never push it.

Delete the unit test only if the rule failed. Record each outcome (rule output excerpt, kept or
deleted) in `$EVIDENCE/deletion-trials.txt`.

**Validation:**
```bash
cargo test -p liquers-store --features store-conformance --lib --tests
```

**Rollback:** `git revert` restores a deleted test.

**Agent Specification:**
- **Model:** sonnet
- **Skills:** liquers-unittest
- **Knowledge:** the Phase 3 deletion table; the OpenDAL suite in `liquers-store/tests/`
- **Rationale:** needs judgment on whether a failure really demonstrates coverage

---

### Step 11: Area F: correct `STORE10`

**File:** `liquers-web/tests/e2e/store.spec.ts` (`STORE10`, around line 151)

**Action:** replace the assertions with the Phase 3 version: `data_format === 'csv'`, a
`media_type` that is `null` or absent for `input.csv`, and a non-empty `media_type` for `blob`. Keep
the explanatory comment and cite the metadata level model.

**Validation** (after `cargo clean`):
```bash
./liquers-web/examples-web/quickstart/build.sh
cd liquers-web/tests/e2e && npm install && npx playwright test store.spec.ts
# Expected: STORE10 passes; the rest of store.spec.ts is unchanged
```

**Rollback:** `git revert`.

**Agent Specification:**
- **Model:** haiku
- **Skills:** none
- **Knowledge:** Phase 2 area F; `liquers-web/src/store/fetch.rs` `infer_metadata`
- **Rationale:** a test-only assertion change

---

### Step 12: Final integration

**Files:**
- `liquers-web/README.md`: the `JsStore` absence protocol and the new browser conformance file
- `specs/issues/STORE-GUIDE-STATUS-TABLE-HAS-NO-GENERATOR.md` (new)
- `specs/issues/CORE-STORE-ROUTER-DIRECTORY-ABOVE-MEMBERS-HAS-NO-METADATA.md` (new)
- `specs/issues/JS-STORE-WRAPPER-HAS-NO-EFFECTIVE-MEDIA-TYPE.md` (new)
- `specs/design/store-conformance-backlog/DESIGN.md`: `phase: documentation`, and `gh_pr` set
  when the implementation PR is opened

**Action:**
1. Confirm that no `#[ignore = "store-conformance-backlog…"]` remains.
2. Update the web README.
3. File three issues, `status: draft`, after searching `specs/index.csv` for duplicates
   (`DOCS_STRUCTURE_GUIDE.md` §4.8):
   - `STORE-GUIDE-STATUS-TABLE-HAS-NO-GENERATOR` (P3, S, docs; store/backends). The guide §9 says
     the table is generated from the reports, and no generator exists.
   - `CORE-STORE-ROUTER-DIRECTORY-ABOVE-MEMBERS-HAS-NO-METADATA` (P3, S, core/store).
     `AsyncStoreRouter::get_metadata` returns `KeyNotFound` for a directory above its members'
     prefixes, although `is_dir` is true.
   - `JS-STORE-WRAPPER-HAS-NO-EFFECTIVE-MEDIA-TYPE` (P3, S, web). A page can read declared metadata
     but not the effective media type (Phase 2 area F).
4. Run the full validation set below.
5. Regenerate the index with `python3 scripts/docs_index.py`.

**Validation:**
```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-lib --lib --tests
cargo test -p liquers-core --features store-conformance --lib --tests
cargo test -p liquers-store --features store-conformance --lib --tests
bash scripts/check-build-matrix.sh
python3 scripts/docs_index.py --check
# then, after cargo clean, the three liquers-web loops from Steps 7, 8 and 11
# Expected: all green; the matrix reports its computed total with no failures
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** all phase documents; `CLAUDE.md`; `DOCS_STRUCTURE_GUIDE.md` §4.8
- **Rationale:** cross-crate verification and judgment on anything the matrix reveals

## Testing Plan

### Unit Tests

**When to run:** in the step that adds each test (Steps 2–9). Each one lands with its code.

**Files:** `liquers-core/src/store.rs`, `liquers-core/src/store_conformance/mod.rs`,
`liquers-store/src/opendal_store.rs`.

**Command:**
```bash
cargo test -p liquers-core --lib && cargo test -p liquers-store --lib
```

**Expected:** every new test passes; the existing `reserved*`, `keyabs*`, `diridx*`, `pathmap*` and
`traitdef01` tests pass unchanged.

### Integration Tests

**When to run:** at the end of each step, per its validation, and all together in Step 12.

**Files:**
- `liquers-core/tests/{store_directory_children_STORE, store_conformance_CONF, conformance_docs_CONF}.rs`
- `liquers-store/tests/store_conformance_CONF.rs`
- `liquers-web/tests/{store_js_STORE, store_conformance_CONF, store_conformance_browser_CONF}.rs`
- `liquers-web/tests/e2e/store.spec.ts`

**Expected:** the report changes listed in the Phase 3 integration table, and nothing else.
Compare the reports against the Step 0 baseline.

### Manual Validation

**When to run:** after Step 12.

```bash
# 1. Compare conformance reports before and after
diff <(grep -E 'dir07|sidecar04' $EVIDENCE/baseline-core.txt) \
     <(cargo test -p liquers-core --features store-conformance --test store_conformance_CONF \
       -- --nocapture 2>&1 | grep -E 'dir07|sidecar04')
# Expected output: dir07 moves from Blocked to Passed; sidecar04 appears

# 2. Confirm no parking mechanism is left
grep -rn "AllowedFailure {" liquers-web/tests liquers-core/tests liquers-store/tests
grep -rn "RuleOutcome::Blocked {" liquers-core/src/store_conformance/rules
# Expected output: nothing
```

**Success criteria:** the outputs match the Phase 3 integration table, and both greps are empty.

## Task Splitting (Agent Assignments)

| Step | Model | Skills | Rationale |
|------|-------|--------|-----------|
| 0 | haiku | — | run commands, save output |
| 1 | haiku | liquers-unittest | copy approved tests, mark ignores |
| 2 | sonnet | rust-best-practices, liquers-unittest | shared trait default; counting test |
| 3 | sonnet | rust-best-practices, liquers-unittest | two directory branches, one helper |
| 4 | sonnet | rust-best-practices, liquers-unittest | rule semantics plus contract prose |
| 5 | sonnet | rust-best-practices, liquers-unittest | reserved-name safety |
| 6 | sonnet | rust-best-practices, liquers-unittest | new rule plus contract prose |
| 7 | sonnet | rust-best-practices, liquers-unittest | wasm interop, protocol change |
| 8 | sonnet | rust-best-practices, liquers-unittest | index consistency, browser loop |
| 9 | haiku + sonnet | rust-best-practices, liquers-unittest | renames plus scanner |
| 10 | sonnet | liquers-unittest | judge break-and-fail evidence |
| 11 | haiku | — | test assertion change |
| 12 | sonnet | rust-best-practices | cross-crate verification |

Steps 2–6 are native and ordered: 4 depends on 2 and 3, and 6 depends on 5. Steps 7, 8 and 11 are
wasm or browser work and run after one `cargo clean`. Step 9 can run in parallel with 7 and 8
because it touches other files. Step 10 follows 9.

### Model Selection Guidelines

- **Sonnet:** any step changing behaviour or a rule's meaning.
- **Haiku:** copies, renames, commands.
- **Opus:** only the final Phase 4 review; no implementation step needs it.

## Rollback Plan

### Per-Step Rollback

Every step is one commit, and `git revert <sha>` undoes it. Steps that change a rule and its
contract prose (4, 6) do so in one commit, so a revert never leaves `D1` red. Step 1's `#[ignore]`
markers mean a revert of any later step leaves its tests ignored rather than failing.

### Full Feature Rollback

```bash
git revert --no-edit <step-1-sha>^..<step-12-sha>
# All changes discarded, codebase returns to pre-implementation state
```
No dependencies are added, so nothing needs removing from `Cargo.toml`.

### Partial Completion

A step that cannot finish (for example, the browser loop being unavailable) is not merged half-done.
Either the step is completed, or its commit is reverted and the remaining issue stays `draft`, with
a note naming what blocked it (`DOCS_STRUCTURE_GUIDE.md` §5.6). The design is never
partially complete.

## Documentation Updates

### New Reference and Guide Documents

None (Phase 2).

### Existing Documents and `affects_docs`

| Document | When | What |
|---|---|---|
| `STORE_SEMANTICS.md` | Steps 4 and 6 (required by D1), completed in Phase 5 | §2 `children`; §8 metadata-only rule; §4 delegate-absence note (Phase 5) |
| `STORE_IMPLEMENTATION_GUIDE.md` | Steps 4 and 6, completed in Phase 5 | §8 rule table; §9 status rows; test-naming section and page-store absence paragraph (Phase 5) |
| `liquers-web/README.md` | Step 12 | `JsStore` protocol; browser conformance file |

`affects_docs` stays `[STORE_SEMANTICS.md, STORE_IMPLEMENTATION_GUIDE.md]`.

### Design, Capability, and Cross-Links

- The three superseded stub designs already point here.
- `specs/README.md` is regenerated by `docs_index.py` in Step 12. Its hand-written map line is
  reviewed in Phase 5.

### Phase 5 Evidence Capture

`$EVIDENCE/` holds the baseline and final reports, the examples-at-HEAD failures, the browser route
used, and the deletion trials. It is outside the repository and never committed; Phase 5 quotes
what matters from it in `phase5-documentation.md`.

### CLAUDE.md

No change. The conventions this project relies on (no `_ =>`, `eprintln!`, and the test loops) are
already stated. The new rule, that unit tests must not use rule-family IDs, belongs in the store
guide, where store authors look, and D1 enforces it.

### PROJECT_OVERVIEW.md

No change. No core concept changes.

## Phase 5 Entry Criteria

- Steps 0–12 are committed.
- Every validation command above is green, including all three `liquers-web` loops (or the
  documented browser route, recorded).
- No `#[ignore = "store-conformance-backlog…"]`, `AllowedFailure` or `RuleOutcome::Blocked`
  remains.
- `scripts/check-build-matrix.sh` is green.
- All review comments are resolved.
- The three issues of Step 12 are filed.
- The seven source issues are `in_progress`. In Phase 5 they move to `closed` with a resolution
  note, and their `design:` field is repointed to `store-conformance-backlog`.

## Review Outcome

Four focused reviews (Phases 1, 2, 3, and codebase) and one final review of all phase documents
were run.

**Blocking findings, both fixed:**
1. **The rename table missed a twelfth colliding name,**
   `prefix01_a_prefixed_store_reports_and_respects_its_prefix`, and the D1 scan would also have
   flagged `sibling01_catches_…` and this design's own rule refutation tests. Fix: the rename is
   added, and refutation tests are named `refute_<rule id>_…`, so no scan exclusion is needed
   (Phase 2 area E; Steps 4, 6 and 9).
2. **OpenDAL has two directory branches.** The memory service takes the `has_children` fallback,
   not the `stat` branch. Fix: one `directory_metadata` helper serves both branches; Step 3 is
   upgraded to sonnet and tests both services.

**Advisory findings, all applied:**
- Phase 3's router corner case is corrected, and the router gap is filed as a new issue in Step 12.
- The wider failure surface of listed metadata-only keys, and the dropped sidecar fields of
  directories, are recorded in Phase 2's risk table and Phase 3's corner cases.
- The file-store `listdir` skips an entry named exactly `.__metadata__`.
- Step 1 compiles the wasm tests with `--no-run`.
- The Step 2 counting test counts calls it can observe.
- Phase 5 rewrites the guide's §9 table.
- Issue statuses follow §4.3: the seven issues are `in_progress` from Step 0.
- `phase: documentation` and `gh_pr` are set after implementation.
- `JS-STORE-WRAPPER-HAS-NO-EFFECTIVE-MEDIA-TYPE` is filed.
- Evidence is kept outside the repository.
- The D1 scan skips a missing sibling crate.

**Rejected finding:** one reviewer claimed that Step 2's `unwrap_or_else` does not compile. It does
compile: `Metadata::get_asset_info` returns `Result` (`metadata.rs:1625`), while only
`MetadataRecord::get_asset_info` (`metadata.rs:1160`) returns `AssetInfo`. A comment in Step 2 now
says so.

**Questions resolved by the author:**
- `HTTP-STORE-METADATA-DROPS-THE-EXTENSION-MEDIA-TYPE` ends `closed`, not `rejected`. The failure
  was real; it was in the test, and correcting the test resolves it.
- Dropping a directory's sidecar fields in `get_asset_info` is accepted and recorded as a risk.

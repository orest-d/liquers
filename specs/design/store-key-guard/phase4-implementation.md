# Phase 4: Implementation Plan — Absolute Store Keys

## Overview

**Feature:** a store requires an absolute key and refuses a relative one with `ErrorType::KeyNotAbsolute`.

**Architecture:** three methods on `Key` carry the rule, one `ErrorType` variant names the
violation, every store checks before the key is used, and the path builders of the file and OpenDAL
stores become fallible so the backend cannot be reached without passing.

**Estimated complexity:** Medium. No architectural unknowns; the volume is 43 mechanical `?`
insertions and 5 match sites the compiler enumerates.

**Estimated time:** 4–6 hours for an experienced Rust developer, most of it Steps 4 and 9.

**Prerequisites:** Phases 1–3 approved. No blocking issues (Phase 2 preflight). No new dependencies.

### Build-order warning — read before Step 1

Step 1 adds an `ErrorType` variant, and the codebase has **no `_ =>` arms on `ErrorType`** by
convention. The moment the variant lands, `liquers-axum`, `liquers-py` and `liquers-web` stop
compiling until Steps 7, 8 and 9 close their match sites. This is intended — it is the convention
working — but it means:

- **`cargo check --workspace` is red from Step 1 until Step 8.** Per-crate validation commands are
  given for each step and are the ones to run.
- `liquers-web` is wasm32-only and excluded from `default-members`, so it does **not** break native
  builds; it is validated separately in Step 9.
- Do not "fix" the intermediate breakage by adding a default arm.

The steps are ordered so each one leaves its own crate compiling, and the workspace is green again
at the end of Step 8.

---

## Implementation Steps

### Step 1 — The error

**Files:** `liquers-core/src/error.rs`, `liquers-core/src/assets.rs`

**Action:**
- Add `KeyNotAbsolute` to `ErrorType` (after `KeyNotSupported`, keeping the key family together).
- Add the constructor:
  ```rust
  pub fn key_not_absolute(key: &Key) -> Self {
      Error {
          error_type: ErrorType::KeyNotAbsolute,
          message: format!(
              "Key '{}' is not absolute; a store requires a key without '.' or '..' segments",
              key
          ),
          position: Position::unknown(),
          query: None,
          key: Some(key.encode()),
          command_key: None,
      }
  }
  ```
  No `store_name` parameter — a relative key is invalid for every store (Phase 2).
- Add `ErrorType::KeyNotAbsolute` to the `NotPersisted` arm list in
  `classify_persistence_error` (`assets.rs:1456`).

**Validation:** `cargo check -p liquers-core` — clean.

**Rollback:** `git checkout liquers-core/src/error.rs liquers-core/src/assets.rs`

**Agent:** haiku · rust-best-practices · Knowledge: `error.rs` constructor conventions,
`assets.rs:1456`. *Rationale: follows an established pattern exactly; the compiler finds any site
that is missed.*

---

### Step 2 — The rule on `Key`

**File:** `liquers-core/src/query.rs`

**Action:**
- Add to `impl Key`:
  ```rust
  pub fn is_relative(&self) -> bool;
  pub fn as_absolute(&self) -> Result<&Key, Error>;
  pub fn try_into_absolute(self) -> Result<Key, Error>;
  ```
  `is_relative` iterates **every** segment testing `is_cwd() || is_parent()` — reuse those existing
  `ResourceName` methods rather than comparing strings, so the definition of `.`/`..` stays in one
  place. `try_into_absolute` is implemented over `as_absolute`.
- Rustdoc on all three, each contrasting with `to_absolute`, and a reverse cross-reference **on**
  `to_absolute` ("this *resolves*; `as_absolute` *asserts*").
- Rename `CwdCursor::is_relative` → `needs_cwd`, updating `query.rs:2202`, `:2489`, `:2504`. Keep
  its first-segment-only behaviour and document why the two differ.
- Add unit tests `keyabs01`–`keyabs06` (Phase 3), including the accepted look-alikes
  `...`, `..x`, `a..b`, `.hidden`, `a.b` as **negatives**.

**Validation:**
```bash
cargo check -p liquers-core
cargo test -p liquers-core --lib query::tests
cargo test -p liquers-core --lib -- keyabs
```

**Rollback:** `git checkout liquers-core/src/query.rs`

**Agent:** sonnet · rust-best-practices · Knowledge: `query.rs` `Key`/`CwdCursor`/`to_absolute`,
Phase 2 signatures, Phase 3 `keyabs01`–`keyabs06`. *Rationale: the borrow-returning signature and
the two-predicates distinction need judgement, and the rustdoc here is the primary deliverable of
the whole design.*

---

### Step 3 — State the contract on the traits

**File:** `liquers-core/src/store.rs` (module docs, `Store`, `AsyncStore`)

**Action:** documentation only, no behaviour. Module docs get the precondition, why it exists
(relative keys are plan-level), and that it is **well-formedness, not authorization**
(`CORE-SESSION-AND-KEY-ACL`). Both traits get the implementor's obligation: check every fallible
key-taking method; `is_supported` gates *routing* only and is not sufficient. Include Phase 3's
Example 2 skeleton and the trap it prevents. Note that `openbin`, when implemented
(`CORE-STORE-OPENBIN-MISSING`), must carry the check.

**Validation:** `cargo doc -p liquers-core --no-deps` — no broken intra-doc links.

**Agent:** sonnet · — · Knowledge: Phase 2 documentation architecture, Phase 3 Example 2 and
Pitfalls. *Rationale: this is the rule's primary home; it must be right, and it is prose.*

---

### Step 4 — File stores

**File:** `liquers-core/src/store.rs` (`AsyncFileStore`, `FileStore`)

**Action:**
- Make the path builders fallible:
  ```rust
  pub fn key_to_path(&self, key: &Key) -> Result<PathBuf, Error>;
  pub fn key_to_path_metadata(&self, key: &Key) -> Result<PathBuf, Error>;
  fn key_to_lock_path(&self, key: &Key) -> Result<PathBuf, Error>;   // AsyncFileStore only
  ```
  Each opens with `key.as_absolute()?` before `PathBuf::push`.
- Add `?` at 15 `AsyncFileStore` call sites and 13 `FileStore` call sites. **`store.rs:1019` needs
  restructuring, not a bare `?`** — it calls `.display()` inside a `format!` argument, so bind the
  path first.
- Add `let key = key.as_absolute()?;` to each fallible key-taking method (methods whose only body is
  delegation to a checked method are left alone).
- `is_supported` on both: add `&& !key.is_relative()`.
- Add `keyabs08` and `keyabs09`. **`keyabs08` must `makedir("a")` before testing
  `a/../../SECRET.txt`** — without the intermediate directory the unfixed code fails with `ENOENT`
  and the test passes for the wrong reason (Phase 3, Pitfall 1). Assert `error_type`, cover writes,
  and byte-compare the outside file afterwards.

**Validation:**
```bash
cargo check -p liquers-core
cargo test -p liquers-core --lib store::tests
```
**Mutation check:** delete one `as_absolute()?` from `key_to_path` and confirm `keyabs08` goes red.
Restore.

**Rollback:** `git checkout liquers-core/src/store.rs`

**Agent:** sonnet · rust-best-practices, liquers-unittest · Knowledge: `store.rs:818-1395`, Phase 2
call-site table, Phase 3 `keyabs08`/`keyabs09` and Pitfall 1, the existing
`test_async_file_store_basic` temp-dir convention (nanosecond-unique dir under
`std::env::temp_dir()`; the crate has no `tempfile` dev-dependency). *Rationale: largest step, one
signature change rippling through 28 sites, plus the test whose subtlety is the whole point.*

---

### Step 5 — Memory stores and routers

**File:** `liquers-core/src/store.rs` (`AsyncMemoryStore`, `MemoryStore`, `StoreRouter`, `AsyncStoreRouter`)

**Action:**
- Guard every fallible key-taking method of both memory stores; `is_supported` becomes
  `!key.is_relative()`. Do **not** also fix `AsyncMemoryStore::is_supported` ignoring its prefix —
  pre-existing and out of scope (Phase 2).
- Routers: `key.as_absolute()?` **before** `find_store`, so a relative key reports `KeyNotAbsolute`
  rather than `key_not_supported(key, "store router")`. `is_supported` becomes
  `!key.is_relative() && find_store(key).is_some_and(…)`.
- Add `keyabs07`, `keyabs10`, `keyabs11`. **`keyabs11` asserts against a directly held store, never
  through a router** — routing is the configuration in which a wrong implementation passes.

**Validation:** `cargo test -p liquers-core --lib store::tests`

**Agent:** haiku · rust-best-practices, liquers-unittest · Knowledge: Step 4's output as the
pattern, Phase 3 `keyabs07`/`keyabs10`/`keyabs11`. *Rationale: follows Step 4's established shape.*

---

### Step 6 — OpenDAL store

**File:** `liquers-store/src/opendal_store.rs`

**Action:** same treatment for `AsyncOpenDALStore` — both path builders return `Result<String, Error>`,
15 call sites take `?`, guards in the fallible methods, `is_supported` (`:509`) gains the predicate.
The sync `OpenDALStore` (`:16-218`) is commented out; leave it. Add `keyabs16` against the memory
backend used by the existing tests.

**Validation:** `cargo test -p liquers-store`

**Agent:** haiku · rust-best-practices · Knowledge: Steps 4–5 as the pattern,
`STORE-OPENDAL-SLASH-HANDLING` (do not attempt to fix slash handling here). *Rationale: mechanical
repetition of an established pattern.*

---

### Step 7 — HTTP status

**File:** `liquers-axum/src/api_core/error.rs`

**Action:** add `ErrorType::KeyNotAbsolute => StatusCode::BAD_REQUEST` to `error_to_status_code`
(`:8`). 400, not the 404 `KeyNotSupported` gets: the caller supplied an address that is not a store
address. Add `keyabs15` asserting the mapping directly — not through a handler, since
`AXUM-HANDLER-TEST-COVERAGE` records that no handler scaffolding exists and building it is out of
scope.

**Validation:** `cargo test -p liquers-axum`

**Agent:** haiku · — · Knowledge: `api_core/error.rs`. *Rationale: one match arm and one assertion.*

---

### Step 8 — Python bindings

**File:** `liquers-py/src/error.rs`

**Action:** add `KeyNotAbsolute` to the `#[pyclass]` `ErrorType` (`:7`) and to **both** `From`
implementations (`:32` core←py, `:65` py←core). Missing either direction is a compile error, so the
build enforces it.

**Validation:** `cargo check -p liquers-py` — and now `cargo check --workspace` is green again.

**Agent:** haiku · — · Knowledge: `liquers-py/src/error.rs`. *Rationale: three symmetric additions.*

---

### Step 9 — Browser stores and the error bridge

**Files:** `liquers-web/src/error.rs`, `liquers-web/src/store/key_guard.rs`, and the three
`liquers-web/tests/store_*_STORE.rs`

**Action:**
- `error_type_name` (`:15`) → `"key_not_absolute"`; `error_type_from_name` (`:46`) ← the same.
  Module doc (`:3`) says `ErrorType` has 22 variants; make it 23.
- `key_guard::check_key` keeps its signature and delegates: `key.as_absolute()?` for the relative
  case, then the existing empty-segment check keeping `Error::key_not_supported(key, store_name)` —
  an empty segment is malformed, not relative, and the store name is informative there. Remove the
  "browser's copy until that lands" paragraph and point at the shared rule.
- Update the three test files to expect `KeyNotAbsolute` for `.`/`..`; leave the empty-segment
  expectation alone. **`tests/e2e/store.spec.ts:265,375` assert `key_not_supported` for a read-only
  *write refusal* — a different case; do not touch.**

**Validation** (after `cargo clean`, per CLAUDE.md — this is a different target):
```bash
cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles
```

**Agent:** sonnet · rust-best-practices · Knowledge: `key_guard.rs`, `error.rs`, the three test
files, Phase 2's error-split table. *Rationale: the two-error split is exactly where a careless
change collapses both cases into one.*

---

### Step 10 — Integration tests

**File:** `liquers-core/tests/store_key_absolute.rs` (new)

**Action:** `keyabs12` (the issue's reproduction end-to-end through an `Environment` with an
`AsyncFileStore`, asserting a planted outside file is unread), `keyabs13` (a `recipes.yaml` with
`cwd: ../../etc` refused at the store — Phase 2 finding B2), `keyabs14` (the regression surface:
normal keys, `-R-key/.` CWD resolution, `.hidden` files).

**Validation:**
```bash
cargo test -p liquers-core --test store_key_absolute
cargo test -p liquers-core --test recipe_cwd_resolution --test plan_cwd_freeze   # unchanged
cargo test -p liquers-lib --lib --tests                                          # the full loop
```

**Agent:** sonnet · liquers-unittest, rust-best-practices · Knowledge: `tests/async_hellow_world.rs`
for the environment-construction flow, `tests/recipe_cwd_resolution.rs` for recipe setup, Phase 3
`keyabs12`–`keyabs14`. *Rationale: environment and recipe wiring is the fiddliest part of writing a
liquers test.*

---

### Step 11 — Documentation

**Files:** `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md`, `specs/reference/PROJECT_OVERVIEW.md`,
`specs/reference/WEB_API_SPECIFICATION.md`, `specs/issues/STORE-FILESTORE-PATH-TRAVERSAL.md`

**Action:**
- `STORE05` (`:1767`): `../escape`, `a/../../etc`, `a/./b` expect `KeyNotAbsolute`; add an
  empty-segment case keeping `KeyNotSupported`; add "check on direct calls, not only routing";
  **add Pitfall 1's intermediate-directory trap**, which is the finding most likely to save the next
  implementor.
- `PROJECT_OVERVIEW.md` §5 Storage: state the precondition, link the gap-analysis entry.
- `WEB_API_SPECIFICATION.md` §3.2 error table (`:164`): `KeyNotAbsolute` → 400.
- The issue: set `design: store-key-guard`. **Delete the trailing "Marked P1 rather than P0…"
  paragraph** — git history shows it is the original filing rationale, superseded when `9c35548`
  triaged the front matter P1→P0 and left the prose behind; it also contradicts the issue's own
  "no workaround exists" and argues likelihood where §4.4 grades impact.
- Each reference/guide touched gets a `## History` row and a `reviewed:` bump in the same commit
  (§9.2). `API_DOCS_GAP_ANALYSIS.md` is already done (`30f2f3e`).

**Validation:** `python3 scripts/docs_index.py && python3 scripts/docs_index.py --check` — 0 errors.

**Agent:** sonnet · — · Knowledge: `DOCS_STRUCTURE_GUIDE.md` §9, Phase 2 documentation architecture,
Phase 3 pitfalls. *Rationale: History/`reviewed:` discipline plus judgement about what belongs in
`STORE05`.*

---

## Testing Plan

| When | Command | Expected |
|---|---|---|
| After Steps 1–2 | `cargo test -p liquers-core --lib -- keyabs` | `keyabs01`–`keyabs06` pass |
| After Steps 4–6 | `cargo test -p liquers-core --lib store::tests`, `cargo test -p liquers-store` | `keyabs07`–`keyabs11`, `keyabs16` pass |
| After Step 8 | `cargo check --workspace` | Green again |
| After Step 9 | `cargo clean` then `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles` | Revised `STORE05` passes |
| After Step 10 | `cargo test -p liquers-lib --lib --tests` | The default loop, all suites green |
| Throughout | Mutation check per Step 4 | Removing a guard turns a test red |

**Disk:** `cargo test -p liquers-lib --lib --tests` is the routine loop (~4.2 GB). Do **not** run
`cargo test --workspace`; run the wasm loop only after `cargo clean`, since it builds a different
target (CLAUDE.md → *Building and testing*).

**Manual validation** — the issue's own reproduction, end to end:
```bash
cargo run -p liquers-core --features cli --bin liquers-validate -- -- '-R/../../etc/passwd'
# Expected: still parses and plans (the language is unchanged) — the refusal is the store's job,
# so a clean validate result here is correct, not a failure of the fix.
```

## Agent Assignment

| Step | Model | Skills | Rationale |
|---|---|---|---|
| 1 Error | haiku | rust-best-practices | Established constructor pattern |
| 2 `Key` rule | sonnet | rust-best-practices | Signature judgement; rustdoc is the deliverable |
| 3 Trait docs | sonnet | — | The rule's primary home |
| 4 File stores | sonnet | rust-best-practices, liquers-unittest | Largest step; the subtle test |
| 5 Memory + routers | haiku | rust-best-practices, liquers-unittest | Repeats Step 4's shape |
| 6 OpenDAL | haiku | rust-best-practices | Mechanical repetition |
| 7 HTTP status | haiku | — | One match arm |
| 8 Python | haiku | — | Three symmetric additions |
| 9 Browser stores | sonnet | rust-best-practices | The two-error split is easy to collapse |
| 10 Integration | sonnet | liquers-unittest, rust-best-practices | Environment/recipe wiring |
| 11 Documentation | sonnet | — | History/`reviewed:` discipline and judgement |

## Rollback Plan

**Per step:** `git checkout <file>`; each step touches a disjoint file set except Steps 4 and 5,
which share `store.rs` — commit between them so the rollback stays granular.

**Full:** the branch `claude/filestore-path-traversal-21f42w` holds only specs commits before
implementation starts, so `git reset --hard <last-specs-commit>` restores the pre-implementation
state. New file to delete: `liquers-core/tests/store_key_absolute.rs`. No `Cargo.toml` change to
revert — no dependency is added.

**Partial:** Steps 1–8 must land together or not at all, because Step 1 breaks three crates until
Step 8. Steps 9–11 can be deferred individually; Step 11 must not be, since a design cannot reach
`complete` with its `affects_docs` unreviewed.

## Documentation Updates

**New reference/guide documents:** none (Phase 2). `DOC_07_STORES_PERSISTENCE.md` will carry the
rule when written; the requirement is recorded in `API_DOCS_GAP_ANALYSIS.md` §7 (**done**, `30f2f3e`).

**Expected authoritative `affects_docs`:**
`specs/reference/api/API_DOCS_GAP_ANALYSIS.md`, `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md`,
`specs/reference/PROJECT_OVERVIEW.md`, `specs/reference/WEB_API_SPECIFICATION.md`,
`specs/issues/STORE-FILESTORE-PATH-TRAVERSAL.md`.
**Discarded:** `STORE_CONFIG_FSD.md`, `DOC_02_QUERY_LANGUAGE_REFERENCE.md`, `ASSETS.md`/`DOC_03`,
and `specs/design/liquers-web-store/phase2-architecture.md` (design history, not current state).

**Capability links:** `specs/README.md` gains the design during implementation; Phase 5 points it at
the rustdoc and the gap-analysis item.

**Phase 5 evidence capture:** whether `key.as_absolute()?` at ~60 sites reads as intended or as noise
(the input to `STORE-ABSOLUTE-KEY-NOT-TYPE-ENFORCED`); any store method found reachable with a
relative key that Phase 2 missed; the real diff size versus the 43-call-site estimate; whether a
relative `recipes.yaml` `cwd` now fails somewhere confusing; and the `Query::absolute` rename issue
that Phase 2 deferred to Phase 5 filing.

## Phase 5 Entry Criteria

- [ ] Steps 1–11 complete; `cargo test -p liquers-lib --lib --tests` and the wasm loop green
- [ ] The mutation check has been run and restored: removing a guard turns `keyabs08` red
- [ ] All user comments answered — the two currently open are the issue's priority (**resolved by
      git history**: keep P0, delete the stale paragraph in Step 11) and confirming the command
      namespaces Phase 2 checked
- [ ] All review comments from Phases 2–4 answered or incorporated
- [ ] Documentation verified against implemented behaviour, not against this plan
- [ ] Phase 5 lands in the same PR (this is a P0 fix; a follow-up documentation PR is not appropriate)
- [ ] Evidence from the Documentation Updates section collected while implementing, not reconstructed
- [ ] `STORE-FILESTORE-PATH-TRAVERSAL` closed with a resolution note; the `Query::absolute` rename
      issue filed

## Review Outcomes

Four conformity passes and one holistic pass were run. This host does not launch parallel review
agents, so they were performed sequentially and recorded unchanged, per the skill's
host-compatibility rule.

| Pass | Result |
|---|---|
| Phase 1 conformity | No findings. Every Phase 1 interaction has a step: query system (2), store system (3–6), error system (1, 7–9), web/API (7), documentation intent (3, 11). |
| Phase 2 conformity | No findings. Every architectural decision has a step; call-site counts match the corrected Phase 2 table (15/13/15); the "no overridable `check_key`" and "no `store_name` parameter" decisions are carried into Steps 1 and 3. |
| Phase 3 conformity | No findings. All 16 tests are assigned: `keyabs01`–`06` → Step 2, `07`/`10`/`11` → Step 5, `08`/`09` → Step 4, `12`–`14` → Step 10, `15` → Step 7, `16` → Step 6, `STORE05` → Steps 9 and 11. Pitfall 1's `makedir` requirement and `keyabs11`'s direct-store requirement are written into the steps that implement them, not left in Phase 3. |
| Codebase compatibility | One finding, fixed — see below. |
| Holistic (all phases) | One finding, fixed — see below. |

| # | Finding | Resolution |
|---|---|---|
| 4.1 | The plan did not say that Step 1 breaks `liquers-axum`, `liquers-py` and `liquers-web` until Steps 7–9, because `ErrorType` has no `_ =>` arms. An implementer hitting a red `cargo check --workspace` at Step 2 would reasonably conclude they had broken something and might add a default arm — silently defeating the convention that makes the variant safe. | Added the **Build-order warning** before Step 1, per-crate validation commands, and the explicit instruction not to add a default arm. Noted that `liquers-web` is outside `default-members` so native builds are unaffected. |
| 4.2 | The manual-validation command would read as a failure: `liquers-validate` on `-R/../../etc/passwd` still succeeds after the fix, because the language is deliberately unchanged. | Expected output states that a clean validate result is correct, not a regression. |

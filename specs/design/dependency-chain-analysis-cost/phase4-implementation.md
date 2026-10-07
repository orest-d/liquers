# Phase 4: Implementation Plan - Direct dependency records and linear dependency analysis

## Overview

**Feature:** Direct dependency records and linear dependency analysis (fixes
`EVALUATING-A-LONG-DEPENDENCY-CHAIN-GETS-SUPER-LINEARLY-SLOW`).

**Architecture:** The three changes from Phase 2:
- **Planner:** one memoized walk per plan analysis, which records direct dependencies only and
  applies a transitive summary (`plan.rs`).
- **Asset manager:** a walk over stored records (`AssetManager::stored_dependency_state`), used by
  the `OnLoad` check and by the audits (`assets.rs`).
- **Recipe provider:** a recipe cache checked against the stored bytes (`recipes.rs`).

**Estimated complexity:** Medium. **Estimated time:** 1–1.5 days.

**Prerequisites:**
- Phases 1–3 approved. Phase 2 Decisions 1–3 are recorded: provider field, 200-link bound
  measured, status case accepted.
- No new crates or dependencies (`scc` is already used by `liquers-core`).

**Commit discipline:** one commit per step, so each step can be reverted on its own. The default
test loop is `cargo test -p liquers-core --lib --tests` (core only, which fits the disk). `cargo
clean` before Step 7's cross-crate runs if `target/` is large.

## Implementation Steps

### Step 1: Benchmark first (baseline)

**File:** `liquers-core/tests/dependency_chain_scaling.rs` (new).

**Action:**
- I6 `chain_evaluation_scales`, `#[ignore]`. Chain sizes come from the `CHAIN_SIZES` environment
  variable, default `10,20,40`. It wraps the provider in a counting `AsyncRecipeProvider`, prints
  time and lookups per size with `eprintln!`, and asserts only that the chain evaluates. The
  bounds are added in Step 6.
- Start from the scratch benchmark (scratchpad `zz_scratch_chain_scaling.rs`), minus the backtrace
  tracing.

**Validation:**
```bash
CHAIN_SIZES=10,20,40 cargo test -p liquers-core --test dependency_chain_scaling -- --ignored --nocapture
# Expected at HEAD: ~0.7 s / ~7 s / ~90 s; lookups 3·(i+1)² per link. Record in the PR.
```

**Rollback:** delete the file.

**Agent Specification:** Model haiku. Skills: liquers-unittest. Knowledge: the scratch benchmark,
Phase 3 I6, CLAUDE.md "Diagnostic Output". Rationale: a mechanical port.

---

### Step 2: Planner — `DependencyWalk` and `analyze_plan_dependencies`

**Files:** `liquers-core/src/plan.rs`, `liquers-core/src/interpreter.rs`,
`liquers-core/src/recipes.rs` (`create_plan_with_init_metadata` only).

**Action:**
- **New types.** Add `DependencySummary` and `DependencyWalk<E>` (Phase 2 "Data Structures").
  `on_path: HashSet<Key>`; `done: HashMap<(Key, Option<Key>), Option<DependencySummary>>`.
- **`walk_plan`.** The step match of today's `find_dependencies`, with every `Step` variant matched
  explicitly and no `_ =>`. In the `GetAsset*` arm:
  - resolve the key exactly as today;
  - push the `StateArgument` dependency, and the `Recipe` dependency when `recipe_opt` finds a
    recipe;
  - call `summarize_key`, and merge its summary instead of extending the dependency list.

  `Evaluate`, `Step::Plan`, `Action`, `GetAssetDirectory` and `GetAssetRecipe` keep today's bodies:
  the walk recurses through anonymous steps, their dependencies pass through, and `Evaluate`
  children are relabelled.
- **`summarize_key`.**
  1. If the key is in `on_path`, return today's "Circular dependency detected…" error with the key
     attached.
  2. If `done` holds a result for this key and the caller's CWD, return it.
  3. Otherwise: `recipe_opt`, then `to_plan_for_key`, then `walk_plan` on the recipe's plan with a
     clone of the caller's cursor. Merge the summaries per Phase 2's rules: the volatile key that
     sorts first wins, and expiries combine with `|`.
  4. Store the result in `done`, and pop `on_path` on **every** exit path, including errors.
- **`analyze_plan_dependencies`.** It replaces `has_volatile_dependencies` and
  `has_expirable_dependencies` and keeps their info, warning and volatile messages word for word.
  - It sorts the direct list as `find_dependencies` sorts today.
  - It applies the `changed && plan.expires.is_volatile()` gate.
  - On error it calls `dependency_check_error` and returns the error, applying nothing.
- **`find_dependencies(envref, plan, cursor)`.** A wrapper with the `stack` parameter removed.
- **Delete** `has_volatile_dependencies`, `has_expirable_dependencies` and
  `has_expirable_dependencies_impl`.
- **Call sites:**
  - `interpreter.rs:65-66` and `:176-179` → `analyze_plan_dependencies(…, initial_cwd).await?`;
  - `recipes.rs:632-635` → `let _ = analyze_plan_dependencies(envref, &mut plan, None).await;`.
    The `if plan.error.is_none()` gate goes away, because on error the merged pass applies
    nothing.
  - Update the `use` lists at `interpreter.rs:15` **and** `recipes.rs:43-44`.
- **`declared_volatile`.** `K` itself when `recipe(K).volatile`; otherwise the minimum, by
  `DependencyKey` string, over the summaries read.
- **Tests.**
  - Changed tests: R1–R4 and R6. The ten `find_dependencies` call sites: four in `src`, which
    become walk internals, and six in tests.
  - New unit tests U1–U11, U8b and U8c. U5, U6, U11 use `CountingRecipeProvider`, extended with
    a recipe without `cwd` for U11.

**Code changes (signatures, as Phase 2):**
```rust
impl<E: Environment> DependencyWalk<E> {
    fn new(envref: EnvRef<E>) -> Self;
    fn walk_plan<'a>(&'a mut self, plan: &'a Plan, cursor: &'a mut CwdCursor)
        -> crate::maybe_send::BoxFuture<'a, Result<(Vec<PlanDependency>, DependencySummary), Error>>;
    fn summarize_key<'a>(&'a mut self, key: &'a Key, cursor: &'a CwdCursor)
        -> crate::maybe_send::BoxFuture<'a, Result<Option<DependencySummary>, Error>>;
}
pub(crate) async fn analyze_plan_dependencies<E: Environment>(
    envref: EnvRef<E>, plan: &mut Plan, initial_cwd: Option<Key>) -> Result<(), Error>;
pub(crate) fn find_dependencies<'a, E: Environment>(
    envref: EnvRef<E>, plan: &'a Plan, cursor: &'a mut CwdCursor,
) -> crate::maybe_send::BoxFuture<'a, Result<Vec<PlanDependency>, Error>>;
```

**Validation:**
```bash
cargo test -p liquers-core --lib plan::
cargo test -p liquers-core --lib --tests
CHAIN_SIZES=10,20,40 cargo test -p liquers-core --test dependency_chain_scaling -- --ignored --nocapture
# Expected: lookups (i+1)-ish per analysis (3·(i+1) per link); 40 links in a few s (still re-parsing recipes.yaml)
```
Every existing test passes except the ones the design changes (R1–R4 and R6). The R6 call sites
are at `plan.rs:4399, 4421, 4430, 4473, 4474`. The prototype showed that
records-only changes break nothing else.

**Rollback:** `git revert` this step's commit.

**Agent Specification:** Model sonnet. Skills: rust-best-practices, liquers-unittest. Knowledge:
- Phase 2 sections "`plan.rs`" and "Semantics preserved exactly";
- today's `plan.rs:2589-2897`, `recipes.rs:280-361` and `recipes.rs:620-640`;
- the scratchpad prototype diff `prototype-direct-records.diff`.

Rationale: the semantics must be preserved exactly. The pop-on-every-exit and sorted-order details
need care.

---

### Step 3: Recipe cache in `DefaultRecipeProvider`

**Files:** `liquers-core/src/recipes.rs`, plus mechanical edits at 115 sites (108 in
`liquers-core`, 6 in `liquers-axum/tests`, `liquers-records/src/provider.rs:205`).

**Action:**
- **Test hook.** A `#[cfg(test)] parses: AtomicUsize` field, incremented on each YAML parse, for
  U12 and U12b. Keep it next to `cache`. This is a test-only amendment to Phase 2's struct.
  `CachedRecipes` derives `Debug`. `scc::HashMap` 3.8.8 implements `Debug` and `Default`, as in
  `ManifestRecipeProvider`.
- **The struct.** `pub struct DefaultRecipeProvider { cache: scc::HashMap<Key, Arc<CachedRecipes>> }`,
  with `#[derive(Default)]` (and `Debug` if `scc::HashMap` allows), plus `pub fn new() -> Self`.
  Mirror `ManifestRecipeProvider::new`.
- **A private `cached_recipes(&self, dir, envref)`**, which returns
  `Result<Option<Arc<CachedRecipes>>, Error>`:
  1. `get_bytes(dir/recipes.yaml)`. An error keeps today's mapping to an empty list.
  2. On a hit, compare the bytes and return the cached entry.
  3. On a miss, parse, `set_cwd`, build `by_name` with `entry().or_insert` so the first duplicate
     wins, and insert.
  4. A parse error is returned and not cached.
- **The trait methods.** `get_recipes` returns `(*cached).list.clone()`, because its public
  signature returns an owned `RecipeList`. `recipe` and `recipe_opt` index `by_name`.
  `has_recipes` is unchanged.
- **The construction sites.** 115 value uses, among 137 lines that mention the name. A blind
  `sed` breaks the definitions and imports, so:
  1. Hand-edit `recipes.rs:684-712` (the struct and its `impl`s) and `recipes.rs:1699`.
  2. Run a `perl -pi` pass over the files from `rg -l '\bDefaultRecipeProvider\b' --type rust`.
     It **skips** lines matching `^\s*(//|use\b|pub struct|impl\b)` and lines containing `{` as
     part of a `use …::{…}` list, and rewrites `\bDefaultRecipeProvider\b(?=\s*($|[),;.]))` to
     `DefaultRecipeProvider::new()`. The `$` alternative catches end-of-line uses such as
     `liquers-records/src/provider.rs:205`.
  3. Review `git diff` by hand: doc links such as `recipes.rs:880, 961` must stay unchanged, and
     the braced `use` lists in `liquers-records/src/provider.rs:21` and the three axum test files
     must not change.
  4. Check every affected crate (validation below).
- **Tests:** U12, U12b, U12c, U13, U14.

**Validation:**
```bash
cargo check -p liquers-core --all-targets
cargo check -p liquers-records --all-features --all-targets
cargo check -p liquers-axum --tests
cargo test -p liquers-core --lib recipes::
cargo test -p liquers-core --lib --tests
CHAIN_SIZES=10,20,40,200 cargo test -p liquers-core --test dependency_chain_scaling -- --ignored --nocapture
# Expected: 40 links ≲ 0.5 s; 200 links ≈ 4.5–5.5 s (Phase 2 prediction)
```

**Rollback:** `git revert` the commit. The call-site edits are in the same commit.

**Agent Specification:** Model sonnet for the cache, haiku for the call-site sweep. Skills:
rust-best-practices. Knowledge:
- Phase 2 "`recipes.rs`: the recipe cache";
- `liquers-records/src/provider.rs:40-175` (the pattern to mirror);
- `recipes.rs:684-830`.

Rationale: the cache is small but must not change `get_recipes`'s semantics. The sweep is
mechanical.

---

### Step 4: `AssetManager::stored_dependency_state`

**File:** `liquers-core/src/assets.rs` (trait `AssetManager`, near `audit_gaps` and
`dependency_version`), plus a re-export of `StoredDependencyState` where `AuditReport` is exported.

**Action:**
- **The enum.** `#[non_exhaustive] pub enum StoredDependencyState { Known(Version),
  Confirmed(Version), Stale { version: Version, dependency: DependencyKey }, Unresolvable }`, with
  `Debug, Clone, PartialEq, Eq`.
- **The default trait method.**
  ```rust
  async fn stored_dependency_state(&self, dep_key: &DependencyKey) -> StoredDependencyState;
  ```
  It is **infallible**, an amendment to Phase 2's `Result`: every store error already maps to
  `Unresolvable`, and `audit_gaps` keeps propagating `dependency_version` errors on today's path.
  It delegates to a private recursive helper that returns
  `crate::maybe_send::BoxFuture<'a, StoredDependencyState>` and carries a `HashSet` of on-path keys
  and a `HashMap` memo for the call. Per key:
  1. If `dependency_manager().get_version(key)` has a version, return `Known(v)`.
     - **Why `Known` needs no recursion.** A key holds a version in the manager only if it was
       computed in this process, or loaded through `try_fast_track` under the same policy, whose
       checks passed (under `OnLoad`, via this walk). Every later upstream change cascades
       through the edges registered then, and the cascade removes its version. A `Known` key is
       therefore as fresh as the policy guarantees.
  2. If the key is not store-resolvable, return `Unresolvable`. Callers treat a non-resolvable
     record as compatible, as today.
  3. If the key is on the current path, return `Unresolvable`.
  4. If it is a **directory key** (`-R-dir/`), its version comes from `dependency_version` (the
     listing version). It has no records and no status. Return `Confirmed(v)` after
     `observe_version`, as today's `OnLoad` does; an error gives `Unresolvable`.
  5. For a **pure key**, `store.get_metadata(key)`. Absent or an error gives `Unresolvable`. No
     `version()` gives `Unresolvable`. Status is tested as `dependency_blocks_fast_track` does: a
     stored `Status::Recipe` does **not** block (the version was kept on remove,
     `assets.rs:1182`); otherwise a status that `status_permits_reuse` rejects gives
     `Stale { version, dependency: key }`.
  6. For each record, compare per Phase 2:
     - Against `Known(v)`, use `v.matches(&record.version)`.
     - Against `Confirmed(v)`, use equality, with a recorded `unknown()` compatible.
     - A child `Stale { dependency, .. }` makes this key `Stale { version: own, dependency }`, and
       nothing is registered for this key.
     - `Unresolvable` refuses only when the record is store-resolvable and its version is not
       unknown, which is today's `OnLoad` rule.
  7. If everything holds: `observe_version(key, v)`, then
     `load_from_records(key, records)`. Pass its `ExpiredDependents` to
     `expire_dependencies_result(…, ExpiryCause::Updated { version: v })`, as `assets.rs:1407-1416`
     does. Return `Confirmed(v)`.
- **The predicate.** Make `AssetData::status_permits_reuse` callable from the trait, by moving it
  to a free `pub(crate) fn` in `assets.rs` (it uses neither `self` nor `E`) and updating its
  callers at `assets.rs:1176, 1183`. The trait reaches the store through
  `self.get_envref().get_async_store()`, and the manager through `self.dependency_manager()`.
- **Export.** `StoredDependencyState` is public through `pub mod assets`, like `AuditReport`. No
  re-export.
- **Tests:** U15–U25. Two test-only `AsyncStore` wrappers delegating to `AsyncMemoryStore` go in
  `assets.rs` `mod tests`: `CountingMetadataStore` (U16) and `FailingMetadataStore` (U24).

**Validation:**
```bash
cargo test -p liquers-core --lib assets::
rustup target add wasm32-unknown-unknown   # not installed in the cloud session by default
cargo check -p liquers-core --target wasm32-unknown-unknown   # ?Send BoxFuture (as check-build-matrix.sh runs it)
```

**Rollback:** `git revert`. Nothing calls the method yet.

**Agent Specification:** Model sonnet. Skills: rust-best-practices, liquers-unittest. Knowledge:
- Phase 2 "`assets.rs`: the stored-records walk";
- `assets.rs:1162-1420` (today's per-record rules);
- `assets.rs:5619-5728` (audit and `dependency_version`);
- `dependencies.rs:261, 453, 478, 1069`.

Rationale: new logic built from existing rules. The comparison rules must not drift.

---

### Step 5: Wire the walk into `try_fast_track` (`OnLoad`) and `audit_gaps`

**File:** `liquers-core/src/assets.rs`.

**Action:**
- **`try_fast_track`.** Replace the body of the `else if … OnLoad …` branch (`assets.rs:1334-1366`)
  with `manager.stored_dependency_state(&dep_record.key)`. **Keep the branch's guard**
  (`OnLoad && dep_record.key.is_store_resolvable() && !dep_record.version.is_unknown()`), so
  command records are never refused. The branch is reached only when the manager has no version,
  so `Known` arrives only if a concurrent registration happened in between. It is compared with
  `matches`. Match explicitly:
  - `Known(v)` with `v.matches(&record.version)`, or `Confirmed(v)` with `v == record.version`:
    continue.
  - Otherwise: log today's "stale (on_load)" line, `clear_fast_track_payload()`, `return
    Ok(false)`.
  - Remove the now-redundant `observe_version` call.

  The manager-known branch, the unconditional `dependency_blocks_fast_track` check and the
  `load_from_records` registration are **unchanged**; the last one is the minimum rule. The order
  of the `OnLoad` refusal and the status check does not matter: both refuse.
- **`audit_gaps`.** For a store-resolvable gap, first call `stored_dependency_state(gap)`:
  - `Stale { dependency, .. }`: `dependency_manager().expire(gap)`, then
    `expire_dependencies_result(expired, ExpiryCause::StaleDependency { dependency })`, and push
    the expired keys into `report.expired` (`Expire` mode). In `ReportOnly` mode, push a finding
    for each dependent edge, as `stale_edges` would.
    - **What `expire(gap)` does here** (verified at `dependencies.rs:800-955`). For a gap with no
      registered version, the skip-cascade check fires only for a `Version(0)` entry, so the
      cascade runs. The gap itself appears in `keys` with `via = gap`; filter it out of
      `report.expired`, since it has no loaded asset. Its dependents expire.
    - If implementation finds otherwise, stop, record the difference in the PR, and ask before
      changing the approach.
    - This is new behaviour: today a gap whose stored version matches never cascades. I4 is its
      test.
  - `Known`, `Confirmed` or `Unresolvable`: today's path, `dependency_version` then
    `audit_version` or `stale_edges`.
- **Tests:** integration I1–I4, I3b, I4b and I4c in `tests/dependency_audit_integration.rs`.
  - Add `struct MakeText { text: &'static str, impl_version: u128 }`.
  - Add `register_counting_commands_with(cr, calls, make_text)`, which sets
    `.impl_version = Version::new(impl_version)` on the `make_text` registration (the return value
    of `register_command`). `register_counting_commands_in` delegates to it with today's
    `"generated"` and no version.
  - Add `second_process_with_commands(snapshot, changed, calls, policy, make_text)`, to which
    `second_process_with` delegates.
  - The existing callers are unchanged.
  - A failing-store wrapper for I4c goes in `liquers-core/tests/fixtures/mod.rs`. It cannot be
    shared with U24's, because `cfg(test)` code in `src/` is invisible to `tests/`.
  - I3 and I3b build their store with `l0` stored `Expired` and `l1` `Ready` by replaying the
    first process's snapshot and rewriting `l0`'s metadata status (as `second_process_with`
    rewrites versions). Add a `changed_status` argument to `second_process_with_commands`.
  - `register_counting_commands_with` is the "extension" of `register_counting_commands_in` that
    Phase 3 describes: a new function, with the old one delegating to it.

**Validation:**
```bash
cargo test -p liquers-core --test dependency_audit_integration
cargo test -p liquers-core --lib --tests
```

**Rollback:** `git revert`.

**Agent Specification:** Model sonnet. Skills: rust-best-practices, liquers-unittest. Knowledge:
- Phase 2 "Corrections" and "Call-site changes";
- Phase 3 E2, I1–I4;
- the scratchpad probe `zz_scratch_restart_upstream.rs`, which is I1/I2's skeleton.

Rationale: behaviour on the restart path; it has to match Phase 1 Decision 1 exactly.

---

### Step 6: Provenance, smoke, and the bound

**Files:** `tests/dependency_audit_integration.rs` (I5), `tests/dependency_chain_scaling.rs`
(I7 and the bounds in I6), and a `liquers-axum` test for the audit endpoint (I8).

**Action:**
- **I5.** An evaluated 10-link chain; expire `l0` through the manager; the last link's `via` is
  `l8`.
- **I7.** A 20-link chain in under 3 s, not ignored.
- **I6.** Add the assertions 40 < 1 s and 200 < 5 s. I6 **fails** unless both 40 and 200 are in
  `CHAIN_SIZES`, and its default becomes `10,20,40,200`. Each size gets a fresh environment and
  provider, so sizes share no warm state. Then run it. If 200 links measures above
  5 s, set the bound to 8 s per Phase 2 Decision 2 and record the measurement in the PR. This run
  is the acceptance check: I6 is `#[ignore]`, so default runs do not check it.
- **I8 (new, carried from Phase 2 Integration Points).** In `liquers-axum/tests` (the file that
  exercises `key_handlers.rs:484-496`; find it with `rg trigger_dependency_audit liquers-axum`),
  set up a stale gap like I4, call the audit endpoint, and assert that the response lists the
  dependent as expired with `StaleDependency`.
- **`chain_env(n)`.** A local helper in `dependency_chain_scaling.rs`, unrelated to the `chain_env`
  in `keyed_version_cascade.rs`: a `SimpleEnvironment<Value>` over `AsyncMemoryStore` with
  `make_text`, `upper` and `DefaultRecipeProvider::new()`. I5 uses its own copy in
  `dependency_audit_integration.rs`.
- **Optional (R5).** Leave `cascade_over_100_link_chain` as it is: its hand-written chain still
  tests what it was written for.

**Validation:**
```bash
cargo test -p liquers-core --test dependency_audit_integration
cargo test -p liquers-core --test dependency_chain_scaling
cargo test -p liquers-core --test dependency_chain_scaling -- --ignored --nocapture   # acceptance (I6)
cargo test -p liquers-axum --tests
```

**Agent Specification:** Model haiku. Skills: liquers-unittest. Knowledge: Phase 3 I5–I7 and the
Step 1 file.

---

### Step 7: Full validation across affected crates

**Action:** run the commands under "Manual Validation". Fix any fallout within this design's
scope. File any unrelated failure as an issue (CLAUDE.md).

**Agent Specification:** Model sonnet. Skills: rust-best-practices.

## Testing Plan

### Unit Tests
- **After Step 2:** `cargo test -p liquers-core --lib plan::` (U1–U11, U8b, U8c, R1–R4, R6).
- **After Step 3:** `cargo test -p liquers-core --lib recipes::` (U12–U14, U12b, U12c).
- **After Step 4:** `cargo test -p liquers-core --lib assets::` (U15–U25).
- **After every step:** `cargo test -p liquers-core --lib --tests`. Only the design-changed tests
  may change.

### Integration Tests
- **After Step 5:** `cargo test -p liquers-core --test dependency_audit_integration` (I1–I4, I3b,
  I4b, I4c).
- **After Step 6:** I5 and I7, plus the ignored I6 with `--ignored --nocapture`.

### Manual Validation
```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests
cargo test -p liquers-records --all-features --lib --tests
cargo test -p liquers-axum --tests
cargo test -p liquers-lib --lib --tests
cargo test -p liquers-core --test dependency_chain_scaling -- --ignored --nocapture   # acceptance (I6), last
rustup target add wasm32-unknown-unknown && cargo check -p liquers-core --target wasm32-unknown-unknown
cargo run -p liquers-core --features cli --bin liquers-validate -- --command make_text --command upper -- '-R/data/l0.txt/-/upper/l1.txt'
```
`scripts/check-build-matrix.sh` is **not** required: no `#[cfg(feature)]`, optional dependency or
`ExtValue` match changes.

## Task Splitting (Agent Assignments)

| Step | Model | Skills | Parallel with |
|---|---|---|---|
| 1 Benchmark | haiku | liquers-unittest | — |
| 2 Planner walk | sonnet | rust-best-practices, liquers-unittest | 3 (different files except `recipes.rs:632`, which is a one-line conflict; do 2 first) |
| 3 Recipe cache | sonnet + haiku sweep | rust-best-practices | after 2 |
| 4 Stored walk | sonnet | rust-best-practices, liquers-unittest | 2, 3 (separate file) |
| 5 Wiring | sonnet | rust-best-practices, liquers-unittest | after 4 |
| 6 Tests and bound | haiku | liquers-unittest | after 3, 5 |
| 7 Validation | sonnet | rust-best-practices | last |

## Rollback Plan

### Per-Step Rollback
Each step is one commit, so `git revert <sha>`. Steps 2, 3 and 4 are independent. Step 5 depends
on 4, and Step 6's bounds depend on 2 and 3.

### Full Feature Rollback
Revert Steps 6 down to 2. Step 1's benchmark can stay, since it is harmless and `#[ignore]`d.

### Partial Completion
- **Steps 2 and 3 without 4 and 5.** Do not ship: records become direct-only, but `OnLoad` and the
  audits lose the deep check, which I2–I4 catch. Ship Steps 2–5 together, or none.
- **Step 3 alone** (the cache) is safe to ship on its own.

## Documentation Updates

### New Reference and Guide Documents
None.

### Existing Documents and `affects_docs`
In Phase 5:
- **`specs/reference/DEPENDENCIES_STATUS.md`** gets a new section, "What a dependency record
  holds", following the Phase 2 Documentation Architecture, plus a History row and `reviewed:`.
- **`specs/reference/ASSETS.md`**: review and update the fast-track and audit-policy text, plus a
  History row and `reviewed:`.
- `affects_docs: [DEPENDENCIES_STATUS.md, ASSETS.md]`.

### Design, Capability, and Cross-Links
- `specs/README.md` design status.
- The issue's resolution, with the numbers, and `status: closed`.
- A follow-up issue, `DEPENDENCY-ANALYSIS-REWALKS-UPSTREAM-PER-EVALUATION` (P3, the optional
  summary cache).
- The index, via `scripts/docs_index.py`.

### Phase 5 Evidence Capture
- The benchmark table before and after (Steps 1, 2, 3, 6) and the lookups per link.
- Any deviation from Phase 2, with its reason.
- Whether the 200-link bound needed relaxing.

### CLAUDE.md / PROJECT_OVERVIEW.md
No change expected: no new pattern, and the core concepts are unchanged. Re-check in Phase 5.

## Phase 5 Entry Criteria
Steps 1–7 merged into the PR branch, all validation green, and review comments resolved.

## Execution Options
After approval: execute now (Steps 1–7 in order, one commit each), create a task list, revise, or
exit.

## Critical Review Checklist
- [x] Every step has files, signatures, validation, rollback and an agent specification
- [x] Unit, integration and manual commands are listed
- [x] Documentation updates are planned for Phase 5
- [x] The partial-completion hazard (Steps 2–5 must ship together) is explicit

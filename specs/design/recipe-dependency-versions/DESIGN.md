---
id: RECIPE-DEPENDENCY-VERSIONS
kind: design
title: Recipe dependency keys carry a content version, so editing a recipe expires what was computed from it
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/assets, core/plan]
issues: [RECIPE-DEPENDENCY-KEY-IS-NEVER-VERSIONED]
created: 2026-10-26
---
# Recipe dependency keys carry a content version, so editing a recipe expires what was computed from it

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented.

## Phase 1: High-Level Design

### Purpose

A keyed asset is the result of its recipe. When the recipe changes, the asset — and everything
built on it — is out of date, exactly as when an input changes. Today nothing connects the two:
recipe dependency keys (`-R-recipe/<key>`) never get a version, and an asset does not depend on its
own recipe at all, so an edited `recipes.yaml` leaves every computed value `Ready` in memory and in
the store. Recipes are the user-facing way to define data products (`VISION.md`); editing one must
invalidate its products.

### Problem Example

Verified by reading HEAD (`cd6d5da`): `DependencyKey::from_recipe_key` is used only by the plan walk
(`liquers-core/src/plan.rs`: `GetAssetRecipe` steps, and reads served by a recipe), its records get
`Version::unknown()` from the version map (`interpreter.rs`), nothing registers a version for a
`-R-recipe/` key, and a keyed asset records no dependency on its own recipe
(`walk_records_direct_dependencies_only` shows the exact list).

`data/recipes.yaml` declares `r.txt` = `make_text/upper/r.txt` (validated:
`liquers-validate --command make_text --command upper --command lower -- 'make_text/upper/r.txt'
'make_text/lower/r.txt'`: both `Ok`).

1. `get(data/r.txt)` → `GENERATED`, `Ready`, stored;
2. `set_binary(data/recipes.yaml, …)` now declaring `r.txt` = `make_text/lower/r.txt`; the write
   publishes the version of `data/recipes.yaml` and refreshes the provider's cache
   (`refresh_listing_version` → `directory_changed`);
3. `get(data/r.txt)`.

Today: `GENERATED`, from memory; after a restart, fast-tracked from the store. Should be: step 2
expires `data/r.txt` (`Cascaded { Updated }` rooted at `-R-recipe/data/r.txt`) and step 3 returns
`generated`. A key that read `data/r.txt` expires with it.

### Scope and Acceptance Criteria

- **AC-1** Editing a key's recipe expires the key
  WHEN a recipe-defining file is written through the asset manager and the recipe of an evaluated key changes
  THEN the key is `Expired` (reason `Cascaded { Updated }`, root `-R-recipe/<key>`), in memory and in the store, and the next `get` evaluates the new recipe
- **AC-2** Dependents follow
  WHEN a key that read the edited key is `Ready`
  THEN it expires in the same cascade
- **AC-3** An unchanged recipe expires nothing
  WHEN the recipe file is rewritten with the same recipe for the key (or only other keys' recipes change)
  THEN the key stays `Ready`
- **AC-4** A recipe reached through a served read
  WHEN a plan reads `-R/<k>` served by a recipe and `<k>`'s recipe changes
  THEN the record `-R-recipe/<k>` carries a concrete version and the reader expires
- **AC-5** Restart
  WHEN the recipe was edited while no process ran, and the audit policy is `on_load`
  THEN the stored copy is not fast-tracked: its own-recipe record no longer matches, and the recipe is evaluated
- **AC-6** A removed recipe
  WHEN the recipe of an evaluated key is removed from the file
  THEN the key is expired as for an edit

Non-goals: recipe files edited outside Liquers while a process runs (no event; caught on the next
`on_load` check or audit, like any external change); reusing analysis summaries
(`DEPENDENCY-ANALYSIS-SUMMARIES-NOT-REUSED-ACROSS-EVALUATIONS`, which this unblocks); recipe
providers that do not read from the store (they get versions at resolution, but no edit event).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — rule 4: every keyed asset's stored metadata gains a
  dependency record, and `DependencyKey::is_store_resolvable` (`pub`) answers `true` for recipe
  keys; rule 5: open decisions below.
- **Leading issue:** **Open design question - which recipe fields make a version.** Recommended:
  every declared field (query, title, description, arguments, links, cwd, volatile, expires,
  `stored`, `cached`), excluding the derived `has_circular_dependencies` / `circular_dependency_key`.
  Consequence: editing only a title recomputes the value; a narrower set (query, arguments, links,
  cwd, volatile, expires) avoids that but leaves a recipe-declared title stale on a stored value.
- **Explanation:** the dependency graph already does everything once a version exists; the work is
  computing a stable version, registering it where recipes are resolved, giving each keyed asset an
  edge from its own recipe, and re-resolving the recipes of a directory when the asset manager
  writes into it.
- **Open questions:**
  1. **Open design question - version fields** (above).
  2. **Proposed resolution - a removed recipe is a change.** Its version is the hash of an empty
     recipe description, so dependents expire (AC-6). The alternative, ignoring removals, keeps
     serving a value no definition produces.
  3. **Proposed resolution - every keyed asset records its own recipe.** One record
     (`-R-recipe/<key>`) per keyed asset, in memory and in stored metadata.
  4. **Proposed resolution - cost of the refresh.** A write into a directory re-resolves the
     recipe keys the version map holds under that directory (a scan of the map's recipe keys). Keep
     the scan; add an index only if `dependency_chain_scaling` regresses by more than 10%.

### Design Dependencies

- `dependency-chain-analysis-cost`, `plan-policy` (complete, E4) — the plan walk and its records.
- `DEPENDENCY-ANALYSIS-SUMMARIES-NOT-REUSED-ACROSS-EVALUATIONS` (issue, no design) —
  **required-by**: its invalidation needs recipe versions.
- `root-listing-dependency-key-spelling`, `recipe-folder-names-listing-contract` — **overlaps**
  (weak): both edit `refresh_listing_version`; this design adds one call at its first level. Textual
  rebase only.
- `runtime-dependency-record-superseded-check` — **overlaps** (weak): a recipe edited during a run
  is caught by its completion check once both land.

### Consolidated Findings

- Version stability: `Recipe` holds `HashMap`s (`arguments`, `links`), so the version is computed
  from an explicitly ordered form (sorted maps), never from `serde_json::to_vec(&recipe)` directly.
- Registration points: (a) `AssetRef::evaluate_recipe`, owner branch, where the key's recipe is in
  hand (`recipe_opt`), with `register_version` (a first registration expires only contradicted
  stored dependents, as for any key); (b) the plan walk's `visit`, for served reads; (c) the
  directory refresh after a write. All three use one crate-private helper.
- Order in the refresh: `directory_changed` (cache drop) runs before the recipes are re-resolved, so
  the new file is read. The refresh never takes `key_mutation_lock`, so it is safe from `remove`.
- Restart: the `-R-recipe/` arm of `dependency_version` and `stored_leaf` mirrors the `-R-dir/` arm
  (a leaf whose version is computed, never read from store metadata).

## Phase 2: Architecture

### Solution

1. `liquers-core/src/recipes.rs`: `pub(crate) fn recipe_version(recipe: Option<&Recipe>) -> Version`
   — `Version::from_content` over a canonical JSON object of the versioned fields with
   `arguments` and `links` collected into `BTreeMap`s; `None` hashes the empty object (open
   question 2).
2. `liquers-core/src/assets.rs`: a crate-private helper

   ```rust
   /// Register the version of `key`'s recipe and expire what recorded a different one.
   pub(crate) async fn register_recipe_version<E: Environment, M: AssetManager<E> + ?Sized>(
       manager: &M, key: &Key, recipe: Option<&Recipe>) -> Version;
   ```

   It calls `dependency_manager().register_version(&DependencyKey::from_recipe_key(key), v)` and
   `expire_dependencies_result(expired, ExpiryCause::Updated { version: v })`.
3. `AssetRef::evaluate_recipe`, owner branch, after `recipe_opt` resolves the key's recipe: call the
   helper, and once the run's `Context` exists add `DependencyRecord::new(from_recipe_key(&key), v)`
   (`Context::add_dependency`), so `track_keyed_asset` → `load_from_records` adds the edge
   `-R-recipe/<key> → <key>` and the record is stored.
4. `liquers-core/src/plan.rs`, `DependencyWalk::visit`: after `recipe_opt` returns a recipe, call
   the helper for `read.key` (the walk holds `envref`), so the interpreter's record for
   `-R-recipe/<k>` gets a concrete version (AC-4).
5. `AssetManager::refresh_listing_version` (default method): at the first level only (the written
   key's parent), after `directory_changed`, call a new private helper
   `refresh_recipe_versions(manager, dir)`: for each key in
   `dependency_manager().recipe_keys_under(dir)` re-resolve `recipe_opt` and call the helper.
6. `liquers-core/src/dependencies.rs`: `pub(crate) async fn recipe_keys_under(&self, dir: &Key) ->
   Vec<Key>` — scans `versions` for `is_recipe_key()` entries whose key starts with `dir`.
7. Restart: `DependencyKey::is_store_resolvable` (`metadata.rs`) also answers `is_recipe_key()`;
   `AssetManager::dependency_version` gains a `-R-recipe/` arm (resolve, `recipe_version`);
   `stored_leaf` treats a recipe key like a directory key (computed leaf, `observe_version`).

Rejected alternatives:

- **Depend on the recipe file** (`-R/data/recipes.yaml`) instead: provider-specific (manifests,
  template providers), and one edit would expire every key of the folder.
- **Version from the provider** (a new `AsyncRecipeProvider::recipe_version`): a `pub` trait change
  for something the core can compute from the resolved recipe.
- **Compare recipes on every `get`:** a provider lookup on the hot path of every cached read.

### Changes

- `recipes.rs`: `recipe_version`. `assets.rs`: `register_recipe_version`, `refresh_recipe_versions`,
  `evaluate_recipe`, `refresh_listing_version`, `dependency_version`, `stored_leaf`.
  `dependencies.rs`: `recipe_keys_under`. `plan.rs`: `visit`. `metadata.rs`:
  `is_store_resolvable` and its doc.
- Tests whose expected dependency lists change: `plan.rs` `walk_records_direct_dependencies_only`
  (unchanged: the walk adds versions, not keys) and any test asserting a keyed asset's full record
  list (gains `-R-recipe/<key>`).
- `specs/reference/DEPENDENCIES_STATUS.md` (recipe keys are versioned; own-recipe edge),
  `RECIPES_AND_PLANS.md` (editing a recipe expires its products), `ASSETS.md` §Why an asset is
  `Expired` (`Updated` rooted at a recipe key). History rows, `reviewed:`.
- No command, registry, serialized format (`DependencyRecord` unchanged) or binding change.

### Risks

| Risk | Handling | Certainty |
|---|---|---|
| Unstable version (map order) expires everything on every refresh | canonical sorted form; unit test hashing two equal recipes built in different insertion orders | high |
| A refresh expires keys whose recipe did not change | `register_version` cascades only on a different version (AC-3) | high |
| Scan cost per write | open question 4; benchmark before/after | medium |
| Tests that relied on the stale behaviour after a `recipes.yaml` write | run core and lib suites; adjust them | medium |
| A provider whose `recipe_opt` errors during refresh | error → skip that key (`eprintln!`), the write stands | high |

Recovery: reverting step 5 (the refresh) removes the cascade on edits, keeping versions.

## Phase 3: Examples and Tests

### Examples

The Problem Example, with `make_text` (returns `generated`), `upper` and `lower` registered the
way `env_over` / `env_with_at` register them in `asset_manager_remove_expire_describe.rs`.

### Tests

New file `liquers-core/tests/recipe_versions.rs` (both built-in managers via a local
`on_both_managers!` macro, as in `runtime_volatility.rs`):

- `editing_a_recipe_expires_the_key` — the Problem Example: after the rewrite, `data/r.txt` is
  `Expired` with root `-R-recipe/data/r.txt`; `get` returns `generated`. Covers AC-1.
- `editing_a_recipe_expires_readers` — `data/s.txt` = `-R/data/r.txt/-/upper/s.txt`; it expires too.
  Covers AC-2, AC-4.
- `rewriting_an_unchanged_recipe_keeps_the_key` — rewrite with `r.txt` unchanged and another recipe
  added; `data/r.txt` stays `Ready`. Covers AC-3.
- `removed_recipe_expires_the_key` — rewrite without `r.txt`. Covers AC-6.
- `edited_recipe_is_not_fast_tracked_after_restart` — evaluate, edit the stored `recipes.yaml`
  directly in the store, build a fresh environment with `DependencyAuditPolicy::OnLoad` over it;
  `get(data/r.txt)` evaluates the new recipe. Covers AC-5.

Unit tests: `recipes::tests::recipe_version_ignores_map_order` and
`recipe_version_changes_with_query`; `dependencies::tests::recipe_keys_under_selects_subtree`;
`metadata::tests::dependency_key_is_store_resolvable` extended with a recipe key. Cover AC-1, AC-3.

Queries validated: `make_text/upper/r.txt`, `make_text/lower/r.txt`, `-R/data/r.txt/-/upper/s.txt`
(with `--command make_text --command upper --command lower`).

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test recipe_versions`, then
`cargo test -p liquers-core --lib --tests`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core/tests/recipe_versions.rs` — the five tests; AC-1, AC-2, AC-4, AC-5, AC-6 fail at HEAD — `cargo test -p liquers-core --test recipe_versions`
- [ ] 2. `recipes.rs` `recipe_version` + unit tests — `cargo test -p liquers-core --lib recipe_version`
- [ ] 3. `assets.rs` `register_recipe_version`; `evaluate_recipe` registers and records the own
  recipe — `cargo test -p liquers-core --test recipe_versions editing_a_recipe` (still fails until step 5)
- [ ] 4. `plan.rs` `visit` registers served recipes — `cargo test -p liquers-core --lib plan::tests`
- [ ] 5. `dependencies.rs` `recipe_keys_under`; `assets.rs` `refresh_recipe_versions` called from
  `refresh_listing_version` — AC-1..AC-4, AC-6 pass. Rollback: revert this step.
- [ ] 6. `metadata.rs` `is_store_resolvable`; `assets.rs` `dependency_version`, `stored_leaf` — AC-5
  passes — `cargo test -p liquers-core --test recipe_versions --test dependency_audit_integration`
- [ ] 7. Benchmark: the ignored `chain_evaluation_scales` / `dependency_chain_scaling` before and
  after; a >10% regression returns open question 4 to the maintainer.
- [ ] 8. `DEPENDENCIES_STATUS.md`, `RECIPES_AND_PLANS.md`, `ASSETS.md` (History, `reviewed:`);
  issue closed — `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests`;
`cargo test -p liquers-records --all-features --lib --tests` (manifest provider); diff review.

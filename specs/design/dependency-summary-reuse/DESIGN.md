---
id: DEPENDENCY-SUMMARY-REUSE
kind: design
title: Dependency analysis reuses the summary of a keyed asset this process has already planned
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/plan, core/assets]
issues: [DEPENDENCY-ANALYSIS-SUMMARIES-NOT-REUSED-ACROSS-EVALUATIONS]
created: 2026-10-26
---
# Dependency analysis reuses the summary of a keyed asset this process has already planned

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented.

## Phase 1: High-Level Design

### Purpose

Every evaluation runs one `analyze_plan_dependencies` walk over its whole upstream recipe graph,
starting from an empty memo. Evaluating a chain of *n* links one at a time therefore costs O(n²)
recipe visits. Wide fan-in over a shared upstream re-walks it for every dependent. When a key has
already been finalized as an asset in this process, its summary (declared volatility, combined
expiry) is known, and the dependency graph that would invalidate it is registered. Reusing it makes
the work per evaluation proportional to the direct dependencies.

### Problem Example

`liquers-core/tests/dependency_chain_scaling.rs` `chain_env(20)`: `data/recipes.yaml` with
`l0.txt = make_text/l0.txt` and `l{i}.txt = -R/data/l{i-1}.txt/-/upper/l{i}.txt`.
`evaluate_chain(20)` evaluates `-R/data/l0.txt` … `-R/data/l20.txt` in order (each query validated:
`GetAsset[data, l3.txt]`).

- Today: about 691 recipe lookups (`chain_20_links_smoke`). After
  `payload-requirement-skips-dependency-analysis`, about 271: one walk per evaluation, but evaluating
  `l20` still walks `l19 … l0`, although each of them was finalized moments before.
- Should be: evaluating `l20` looks up `l19`, finds its summary from `l19`'s own finalization, and
  stops. The total is about 3 lookups per link, roughly 65 for 20 links.

### Scope and Acceptance Criteria

- **AC-1** One-at-a-time chains are linear
  WHEN `evaluate_chain(20)` runs
  THEN the recipe lookups are below 120, and `evaluate_chain(200)` stays within `BOUNDS`
- **AC-2** Fan-in is linear
  WHEN after the chain, 50 keyed recipes each reading `-R/data/l20.txt` are evaluated
  THEN the lookups added are below 50 × 4
- **AC-3** The results are those of a full walk
  WHEN `l0`'s recipe declares `expires: in 5 min` and the chain is evaluated one link at a time
  THEN every `l{i}` asset expires `in 5 min` and none is volatile, exactly as with the cache disabled
- **AC-4** A volatile upstream is never short-cut
  WHEN `l0`'s recipe is `volatile: true`
  THEN every `l{i}` plan is volatile, as today (volatile summaries are never cached)
- **AC-5** Invalidation follows the dependency graph
  WHEN `l10` is expired (`AssetManager::expire`) or its key is written, and `l20` is then evaluated again
  THEN the cached summaries of `l10 … l20` are gone, the next walk descends through them again, and
  the result matches AC-3
- **AC-6** Priming is unaffected
  WHEN the inline manager primes a cold chain (`upstream_graph`)
  THEN it records every upstream node as today: the recorder never uses the cache

Non-goals: a memo shared by the analyses of one evaluation session (the issue's first option; it
does not help one-at-a-time evaluation); summaries of keys that were never finalized in this process
(no invalidation edges exist for them); persisting summaries across restarts; versioning recipe
keys (`RECIPE-DEPENDENCY-KEY-IS-NEVER-VERSIONED`).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — a performance feature, not a bug fix (rule 2), and two open
  decisions (rule 5).
- **Leading issue:** Proposed resolution - where summaries are kept.
- **Explanation:** under the recommended answers the design is complete and local to `liquers-core`.
  It needs no public API, because the dependency manager is crate-internal and already the owner of
  the invalidation graph. The decisions are visible because they define when an analysis is allowed
  to trust earlier work.
- **Open questions:**
  1. **Proposed resolution - where summaries are kept.** (a) **Recommended:** a crate-private map
     in `DependencyManager`, filled when a keyed asset's plan is finalized, and cleared by the same
     cascade that expires the asset. (b) Read volatility and expiry from the live `Ready` asset's
     metadata. This needs a new `AssetManager` trait method to peek without creating or loading, and
     it has no answer for `Override` and `Source` values. (c) A memo per evaluation session. It
     helps only a cold deep request.
  2. **Proposed resolution - recipe edits.** Until `RECIPE-DEPENDENCY-KEY-IS-NEVER-VERSIONED` is
     fixed, editing a recipe does not expire the assets built from it, and under (a) it does not
     drop their cached summaries either. **Recommended:** accept this. The cached summary then
     describes the value actually served, which is equally stale. Once recipe keys get versions,
     the existing cascade (edges to `-R-recipe/<key>` are already recorded) removes both with no
     change here. The alternative is to make that issue a hard prerequisite.

### Design Dependencies

- `payload-requirement-skips-dependency-analysis` — **requires** (order only): removes the two
  wasted walks per scheduled dependency. AC-1's threshold assumes it. The two designs edit different
  functions (`RequiresPayload for Query`, `Context::schedule_dependency_asset_with_key` there;
  `DependencyWalk::visit`, `finalize_plan_expanded`, `DependencyManager` here).
- `cwd-less-recipe-analysed-at-root` — **overlaps**: changes how `visit` resolves a recipe without
  its own `cwd`. This design caches only recipes that set their own `cwd`. Once that design lands,
  every recipe's summary is caller-independent and the restriction can be lifted (a follow-up, not a
  prerequisite).
- `inherited-volatility-bounds-predecessor-cut` — **overlaps**: reads the per-read contributions in
  `run`. Cached summaries are non-volatile, so they never set its position.
- `recipe-preview-shares-plan-analysis` — **overlaps**: a preview reads the cache, never fills it.
- `RECIPE-DEPENDENCY-KEY-IS-NEVER-VERSIONED` (issue, no design) — related through decision 2.

### Consolidated Findings

- A summary is safe to reuse exactly as long as nothing it was computed from has changed. For a key
  finalized as an owned asset, `finalize_plan_expanded` has just registered the edges to its direct
  dependencies (`register_plan_dependencies`), and those dependencies registered theirs. So every
  change that the dependency graph can see reaches the key through `expire_from_frontier`, and that
  is where the cache entry is removed.
- What to cache is already on the plan after analysis: a non-volatile plan's `expires`. That equals
  the walk's summary of the key (`recipe.expires | recipe_plan.expires | upstream`, because `to_plan`
  folds the recipe's own `expires`). Volatile plans are not cached, so `declared_volatile` is always
  `None` and `self_volatile` always `false` in a cached summary.
- Caller independence: the walk memoizes per `(key, caller CWD)` because a recipe without its own
  `cwd` resolves against its caller. Only recipes with their own `cwd` are cached. `Recipe::to_plan`
  records that as `plan.prologue_steps > 0`. `DefaultRecipeProvider` sets the `cwd` of every recipe
  from `recipes.yaml` (`RecipeList::set_cwd`), so this covers the normal case.
- `upstream_graph` (the inline manager's priming) must see every node and bypasses the cache.
- A successful keyed evaluation registers its own version (`track_asset` → `track_keyed_asset` →
  `register_version`), so the summary is staged at finalization and promoted after that
  registration; otherwise the evaluation would erase its own entry (Codex review, PR #183).
- The map grows and shrinks like the dependency manager's `versions` map; no new bound is needed.

## Phase 2: Architecture

### Solution

1. `liquers-core/src/dependencies.rs` `DependencyManager` gains a private field
   `summaries: scc::HashMap<DependencyKey, crate::plan::DependencySummary>` (initialised in `new`) and:

   ```rust
   pub(crate) async fn cached_summary(&self, key: &DependencyKey) -> Option<DependencySummary>;
   pub(crate) async fn cache_summary(&self, key: DependencyKey, summary: DependencySummary);
   pub(crate) async fn stage_summary(&self, key: DependencyKey, summary: DependencySummary);
   ```

   A summary is **staged** at finalization and **promoted** to `summaries` only when the evaluation
   that computed it is tracked: `track_keyed_asset` moves the staged entry into `summaries` *after*
   its own `register_version` call. (Codex review on PR #183: `track_asset` → `track_keyed_asset` →
   `register_version` runs on every successful keyed evaluation, so a summary cached at
   finalization would be removed by the same evaluation's version registration.) A private
   `staged: scc::HashMap<DependencyKey, DependencySummary>` holds the staged entries.

   Removal, of both the cached and the staged entry: in `expire_from_frontier` for the root and
   every expired dependent (a dependency that changed while the key was evaluating discards the
   staged summary, so it is never promoted); in `remove`; in `register_written_version` for the key
   itself; and in `register_version` for the key itself **except** when called from
   `track_keyed_asset`, which forgets the old cached entry and then promotes the staged one. One
   private `forget_summary(&self, key)`.
2. `liquers-core/src/plan.rs`: `DependencySummary` gets a `pub(crate) fn from_finalized(expires:
   Expires) -> Self` (non-volatile, no declared key). `DependencyWalk` gains `use_cache: bool`,
   `true` in `analyze_plan_dependencies` and `false` when `recorder` is set (`upstream_graph`). In
   `visit`, after the cycle, directory and memo checks and before `recipe_opt`: if `use_cache` and
   `read.kind != ReadKind::Directory` and the dependency manager has a summary for
   `DependencyKey::from(&read.key)`, insert it into `done` and return `Visit::Done(Some(summary))`.
   The root's `served` flag is then `true` for a `State` read, which records the same `Recipe`
   dependency as a full visit.
3. `liquers-core/src/interpreter.rs` `finalize_plan_expanded`, inside the existing
   `if !plan.is_volatile { … if let Some(key) = context.owner_key().await? { … } }` block, after
   `register_plan_dependencies`: if `plan.prologue_steps > 0`, call
   `manager.dependency_manager().stage_summary(DependencyKey::from(&key),
   DependencySummary::from_finalized(plan.expires.clone()))`. The summary becomes visible to later
   walks once the asset is tracked (item 1).

Sync/async: `scc` async accessors, as the other maps. Errors: none new; the cache is an
optimisation, and a miss falls back to today's walk.

Rejected: options (b) and (c) of decision 1; caching every summary the walk computes (keys never
finalized have no invalidation edges).

### Changes

- `liquers-core/src/dependencies.rs`: field, three crate methods, four removal points, unit tests.
- `liquers-core/src/plan.rs`: `DependencySummary::from_finalized`, `DependencyWalk::use_cache`,
  `visit`, `upstream_graph` / `key_upstream_graph` (cache off).
- `liquers-core/src/interpreter.rs`: `finalize_plan_expanded`.
- `liquers-core/tests/dependency_chain_scaling.rs`: tests.
- Documents: `specs/reference/RECIPES_AND_PLANS.md` §Finalization (the summary of a finalized keyed
  asset is reused until it expires); `specs/reference/DEPENDENCIES_STATUS.md` (the dependency
  manager also holds analysis summaries). History rows, `reviewed:` bumps.

No `pub` API, command, registry or format change.

### Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `dependencies.rs`, `plan.rs`, `interpreter.rs`, one test file, two documents |
| Soundness | a stale summary needs a change the graph cannot see: a recipe edit (decision 2), or an edge not yet registered. Edges are registered in the same block, before the entry is written, so the second does not occur |
| Existing tests | walk tests in `plan.rs` use a fresh environment per test, so they see no cache; `dependency_chain_scaling`, `deep_chain_evaluation`, `keyed_version_cascade` exercise the paths |
| Concurrency | two evaluations finalizing the same key write equal summaries; a cascade racing a write can leave an entry for a key just expired, which is the same window `register_plan_dependencies` already has (the next finalization overwrites it) |
| Performance | one map lookup per read in the walk; the walk stops at the first finalized key |
| Recovery | set `use_cache` to `false` in `analyze_plan_dependencies` (one line) or revert |
| Certainty | medium-high: invalidation coverage is checked by AC-5 |

## Phase 3: Examples and Tests

### Examples

The Problem Example. After the change, the walk for `l20` visits `l19` only, and the cached summary of
`l19` (expiry `in 5 min` in AC-3) is merged into `l20`'s plan exactly as the full walk would.

### Tests

In `liquers-core/tests/dependency_chain_scaling.rs` (existing `chain_env`, `evaluate_chain`):

- `chain_one_at_a_time_walks_only_direct_dependencies` (AC-1): `evaluate_chain(20)` lookups `< 120`.
- `fan_in_over_a_finalized_chain_is_linear` (AC-2): after the chain, add 50 recipes
  `f{j}.txt = -R/data/l20.txt/-/upper/f{j}.txt` to `recipes.yaml` (written through `set_binary` so
  the provider refreshes), evaluate them, and check that the added lookups are `< 200`.
- `cached_summaries_match_a_full_walk` (AC-3, AC-4): two environments, one with the cache disabled
  through a `#[cfg(test)]` switch on the walk, the chain with `l0` expiring `in 5 min` and, separately,
  volatile. Compare `expires()` and volatility of every `l{i}` asset.
- `expiring_a_link_drops_downstream_summaries` (AC-5): evaluate the chain, `expire` `data/l10.txt`,
  reset the counter, evaluate `-R/data/l20.txt` again; the lookups are at least 10 (the walk
  descends again), and the expiry matches AC-3.
- Existing `chain_20_links_smoke`, the `BOUNDS` test, and `liquers-core/tests/deep_chain_evaluation.rs`
  (AC-6, inline priming).

Unit, `dependencies.rs` `mod tests`: `summary_is_forgotten_with_its_key_and_dependents` (AC-5): cache
summaries for `a` and `b`, add edge `b → a`, `register_version(a, new)`; both entries are gone.
`staged_summary_survives_own_tracking` (AC-1): stage a summary for `a`, run `track_keyed_asset` for
`a` (which registers its version); `cached_summary(a)` is `Some`. `staged_summary_dropped_by_upstream_change`
(AC-5): stage for `b` with edge `b → a`, `register_version(a, new)`, then track `b`; `cached_summary(b)`
is `None`.

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test dependency_chain_scaling --test
deep_chain_evaluation`, `cargo test -p liquers-core --lib dependencies`.

## Phase 4: Implementation Plan

### Steps

Precondition (order only): `payload-requirement-skips-dependency-analysis` has landed, so AC-1's
threshold measures this change.

- [ ] 1. `dependencies.rs` — `summaries`, `staged`, `cached_summary`, `cache_summary`, `stage_summary`, promotion in `track_keyed_asset` after its `register_version`, `forget_summary` at the removal points; unit tests `summary_is_forgotten_with_its_key_and_dependents`, `staged_summary_survives_own_tracking`, `staged_summary_dropped_by_upstream_change` — `cargo test -p liquers-core --lib dependencies`. Rollback: revert.
- [ ] 2. `plan.rs` — `DependencySummary::from_finalized`, `DependencyWalk::use_cache` (off for `upstream_graph`, `key_upstream_graph`), cache consult in `visit`, test-only switch — depends on 1 — `cargo test -p liquers-core --lib plan::tests`. Rollback: revert.
- [ ] 3. `interpreter.rs` `finalize_plan_expanded` — `stage_summary` after `register_plan_dependencies` when `prologue_steps > 0` — depends on 2 — the four new tests in `dependency_chain_scaling.rs` pass. Rollback: revert 1-3 together.
- [ ] 4. Documents — `RECIPES_AND_PLANS.md`, `DEPENDENCIES_STATUS.md` (History rows, `reviewed:`) — `python3 scripts/docs_index.py --check`
- [ ] 5. Close `DEPENDENCY-ANALYSIS-SUMMARIES-NOT-REUSED-ACROSS-EVALUATIONS` with a resolution note (and, if decision 2 was "accept", a line in `RECIPE-DEPENDENCY-KEY-IS-NEVER-VERSIONED` saying the fix also refreshes cached summaries); this design to `complete` — `python3 scripts/docs_index.py` then `--check`

### Validation

`cargo fmt --check -p liquers-core`; `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests`;
`cargo test -p liquers-lib --lib --tests`; `CHAIN_SIZES=200 cargo test -p liquers-core --test
dependency_chain_scaling -- --nocapture` for the timing record. Diff review: three source files, one
test file, two documents.

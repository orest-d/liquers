---
id: KEYED-RUN-REPLACES-DEPENDENCY-EDGES
kind: design
title: A keyed run replaces its incoming dependency edges, and expression attribution lives as long as the expression's value
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/assets]
issues: [EVALUATION-KEEPS-DROPPED-DEPENDENCY-EDGES, EXPRESSION-ATTRIBUTION-MAPS-ARE-NEVER-CLEANED]
merged: 2026-10-20
created: 2026-10-20
---
# A keyed run replaces its incoming dependency edges, and expression attribution lives as long as the expression's value

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20. Phases 1-4 are written but not
approved, and nothing is implemented. Two sources share this design because they strongly overlap
(`overlap.md` T2 and T3, recorded under Design Dependencies).

## Phase 1: High-Level Design

### Purpose

On the evaluation path the graph of `DependencyManager` (`liquers-core/src/dependencies.rs`) only
grows. (1) A keyed run adds edges at schedule time (`register_scheduled_dependency`) and at
completion (`track_keyed_asset` → `load_from_records`); nothing removes the previous run's edges.
The write path was fixed by `replacing-write-drops-stale-dependency-edges` (complete, #159), which
left this path out. (2) The expression maps `expression_dependents`, `expression_keyed_deps` and
`expression_expr_deps` are never cleaned (`remove_expression` is `#[cfg(test)]`): memory grows per
distinct expression, and a keyed asset stays attributed to `q` after it stopped reading it.

Verified at HEAD (18732dd): only `track_replacing_asset` and `set_volatile` call
`remove_dependent_edges`; `remove_expression` has one test caller.

**Finding.** `dependency-scheduling` planned to drop all three entries at the expression's terminal
status. That breaks a *cached* expression: a registered `Ready` `q` is reused, a later keyed
requester gets its edges `x → k` from `expression_keyed_deps[q]` (`propagate_attribution`), and
expiry does not cascade through an expression, so those edges are the only route from `x` to `k`.

### Problem Example

Command `pick` reads `-R/data/c.txt` and then evaluates the key whose name it contains. Recipe
`data/r.txt` = `-R/data/c.txt/-/pick` (validated: `liquers-validate --command pick --
'-R/data/c.txt/-/pick'`, status `Ok`).

```text
c.txt = "a"   get r.txt      run 1 reads c, a        edges c→r, a→r
write c = "b"                r expires; c's edge map is dropped by the cascade; a→r survives
get r.txt                    run 2 reads c, b        edges c→r, b→r (a→r still there)
write a = "new"              today: r expires (Cascaded, root -R/data/a.txt)   should: r stays Ready
```

Expression side: keyed `k` evaluates the non-keyed `q` = `-R/data/x.txt/-/upper`. After `q`
expires and `k` re-runs without it, `expression_dependents[q]` still holds `k` and
`expression_keyed_deps[q]` holds `x` for the life of the process; a later run of `q` adds a
spurious `y → k` edge for each new read `y`.

### Scope and Acceptance Criteria

- **AC-1** A dependency the new run did not read no longer expires the key
  WHEN a keyed asset is re-evaluated and its new run does not request a dependency `a` its previous run requested
  THEN a later write of `a` does not expire the key
- **AC-2** A dependency the new run read still expires it
  WHEN the new run requests `b`, directly or through a non-keyed expression
  THEN a later write of `b` expires the key (`Cascaded`, root `b`)
- **AC-3** The key's own dependents are untouched
  WHEN a keyed run starts
  THEN the edges from the key to its dependents are unchanged, and they still expire when the new value is published
- **AC-4** A re-run leaves the expressions it no longer reads
  WHEN a keyed asset re-runs
  THEN it is no longer in any `expression_dependents` set until its new run requests that expression again
- **AC-5** An expression that will not be served again releases its attribution
  WHEN a non-keyed asset's run ends `Error`, `Cancelled`, `Volatile` or `Expired`, or ends in any status while the asset is not registered for reuse (`cached: false`), or a `Ready` non-keyed asset is later expired
  THEN its `expression_keyed_deps` and `expression_expr_deps` entries are gone, and its `expression_dependents` entry is gone once empty
- **AC-6** A cached expression keeps serving late joiners
  WHEN a registered non-keyed asset `q` is `Ready` and a keyed asset first requests it after `q` finished
  THEN a later write of a key `q` read expires that keyed asset, as today
- **AC-7** Cycle detection is unchanged
  WHEN the existing schedule-time cycle tests run (`dependencies::tests::scheduled_*`)
  THEN they pass unchanged

Non-goals: dropping a keyed asset from attribution sets when it is merely expired or removed (it
leaves on its next run or write); a reverse edge index (see Open questions); the loss of
expression-mediated edges after a reload (`EXPRESSION-DEPENDENCY-RECORD-IS-LOST-ON-RELOAD`, filed
with this design); `set_binary` (`set-binary-replaces-dependency-edges`).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. Rule 5: two observable choices are open (below). Rule 3 would
  also fail if the reverse-index alternative were chosen.
- **Leading issue:** **Proposed resolution - when a keyed asset's old edges go:** at the start of
  its run (recommended), not at its completion.
- **Explanation:** The run start is the one point where the old edges are certainly obsolete and
  the new ones are not yet registered, so clearing there needs no record of which edges this run
  added. The expression rule follows the expression's value, not its status, which keeps AC-6.
- **Open questions:**
  1. **Proposed resolution - clear at run start.** A write between the clear and the run's request
     expires nothing, which is harmless: the run has not read it yet. Alternative: replace at
     completion, which needs a per-run set of added edges (a new structure).
  2. **Proposed resolution - accept over-expiry for a lingering attribution.** A keyed asset that
     is expired but never re-run stays in `expression_dependents[q]`, so a later run of `q` can
     still add an edge to it. Removing it from the set at `q`'s expiry instead would race with a
     requester that joins the set in the same instant, and the race would under-expire (a stale
     value). Over-expiry is the safe direction.
  3. **Open design question - the cost of the clear.** `remove_dependent_edges` scans every entry
     of `keyed_dependents` and `expression_dependents`, so a run costs O(graph). Recommended: keep
     the scan, and add a reverse index (`dependent → its dependencies`) only if the
     `dependency_chain_scaling` benchmark regresses by more than 10%. A reverse index has to stay
     consistent in every expiry path, so it is not added speculatively.

### Design Dependencies

- `replacing-write-drops-stale-dependency-edges`, `dependency-scheduling` (complete, E4): this
  design extends the first's write rule to runs and corrects the second's cleanup rule.
- `set-binary-replaces-dependency-edges`, `runtime-dependency-record-superseded-check`:
  **overlaps** (weak: the same invariant on another path; the other end of a run). No ordering.
- `EXPRESSION-DEPENDENCY-RECORD-IS-LOST-ON-RELOAD` (filed with this design): **overlaps**. A run
  cleared and then cancelled reloads its stored copy's edges from records, as after a restart; the
  limitation exists today and is not widened.

Merge record (`overlap.md`): T2 (the expression cleanup relies on AC-4 to bound
`expression_dependents`) and T3 (`remove_dependent_edges` / `remove_expression`). No runner-up.

### Consolidated Findings

- Clear before the run's context exists, so every registration of the new run lands after it.
  `remove_dependent_edges` already strips the key from `expression_dependents` (AC-4).
- Never drop `expression_keyed_deps[q]` while `q` is a `Ready`, registered value (AC-6).
- Clearing `expression_keyed_deps` / `expression_expr_deps` affects only *late joiners*, never an
  edge already added, so a concurrent run of the same expression is not harmed.
- Step 5 measures the O(graph) scan; a regression returns open question 3 to the maintainer.

## Phase 2: Architecture

### Solution

**Keyed run start.** In `AssetRef::evaluate_recipe` (`liquers-core/src/assets.rs`), in the branch
"This asset is the registered owner, or nothing is registered" (after the recipe is resolved and
before `self.create_context()`), when `self.bound_owner_key()` is `Some(key)` and the asset is not
volatile, call
`dependency_manager().remove_dependent_edges(&DependencyKey::from(&key))`. `bound_owner_key` is
already the graph-node test that `track_asset` and the stale-dependency path use. A delegating
asset gets `None` and clears nothing. An unregistered `cached: false` owner is still its key's node
and is cleared. No `data` lock is held across the call.

**Expression release.** Replace `#[cfg(test)] remove_expression` with

```rust
/// Drop what an expression run left for late joiners; the attribution set goes only once empty.
pub(crate) async fn release_expression(&self, expr: &DependencyKey);
```

It removes `expression_keyed_deps[expr]` and `expression_expr_deps[expr]`, and removes
`expression_dependents[expr]` only when the set is empty (`remove_if_async`). It is called:

1. at the end of a run, in `AssetRef::finish_run_with_result`, for an asset with no bound owner key
   and a query, when the final status is not `Ready`, or the metadata says `cached: Some(false)`
   (set by `mark_unregistered`). A `Volatile` final status is "not `Ready`".
2. in `AssetRef::expire_without_cascade`, for an asset with no bound owner key and a query whose
   status was `Ready` before the transition.

The key is `DependencyKey::from(&query)`, as at scheduling. `match` on `Status` stays explicit.

Rejected: replace at completion (expression-mediated edges are not in the records, so live edges
would go); drop all three entries at terminal status (breaks AC-6); key the maps by asset id
(rewrites the scheduling code for a P3 issue).

Known-issue preflight: nothing open blocks this; `root-listing-dependency-key-spelling` edits
`load_from_records` only.

### Changes

- `liquers-core/src/dependencies.rs`: `release_expression` (replacing `remove_expression`); doc
  comments of the three expression maps (their lifetime) and of `remove_dependent_edges` (its new
  caller); a `#[cfg(test)] fn expression_entries(&self, expr) -> (usize, usize, usize)` for tests.
- `liquers-core/src/assets.rs`: `evaluate_recipe` (run-start clear), `finish_run_with_result` and
  `expire_without_cascade` (release).
- `specs/reference/DEPENDENCIES_STATUS.md`: "A run replaces its key's incoming edges" next to the
  write rule, plus the expression lifetime rule. History row and `reviewed:`.
- No `pub` item, command, registry or serialized format changes. Async, executor-agnostic.

### Risks

| Risk | Handling | Certainty |
|---|---|---|
| An edge the old run had and the new run re-requests is missing for an instant | the new request re-adds it before reading. AC-2 | high |
| O(graph) scan per keyed run | Open question 3. Run the ignored `chain_evaluation_scales` before and after | medium |
| Late joiner on a cached expression loses edges | release only off `Ready` or for unregistered assets. AC-6 | high |
| The cancelled-replacement rollback | it runs after a request, the clear before the context exists | high |

Recovery: the two changes are independent call sites. Reverting either restores today's behaviour.

## Phase 3: Examples and Tests

### Examples

The Problem Example is `rerun_drops_an_unread_dependency_edge`: both built-in environments,
`AsyncMemoryStore`, `DefaultRecipeProvider`, `data/recipes.yaml` declaring `r.txt`; `pick` is an
async command returning the value of `context.evaluate(format!("-R/data/{input}"))`.

### Tests

Integration (`liquers-core/tests/dependency_manager_integration.rs`, bodies in
`tests/common/manager_scenarios.rs`, run on both built-in managers as `replacing_install_*` is):

- `rerun_drops_an_unread_dependency_edge`: the Problem Example. After the final write, `r.txt` is
  `Ready`. Covers AC-1.
- `rerun_keeps_the_edge_of_what_it_read`: as above, then `set_binary(b)`. `r.txt` is `Expired` with
  `Cascaded { root: -R/data/b.txt }`. Covers AC-2.
- `rerun_keeps_the_keys_own_dependents`: `s.txt` = `-R/data/r.txt/-/upper`. Re-run `r`, publish a
  new value of it: `s.txt` expires. Covers AC-3.
- `cached_expression_attributes_a_late_joiner`: `k1` then `k2` read the registered `Ready`
  `q = -R/data/x.txt/-/upper`; a write of `x` expires `k2`. Covers AC-6 (passes at HEAD; a guard).

Unit (`liquers-core/src/dependencies.rs` `tests`):

- `release_expression_keeps_a_nonempty_attribution_set` and
  `release_expression_drops_an_empty_attribution_set`. Cover AC-5.
- `remove_expression_clears_transient_attribution`: renamed `release_expression_clears_late_joiner_state`
  and adapted (the cycle-free re-registration still holds). Covers AC-5 and AC-7.
- Existing `scheduled_*` tests unchanged. Covers AC-7.

Unit (`liquers-core/src/assets.rs` `tests`, which reach `expression_entries`):

- `keyed_run_start_leaves_expression_sets`: re-run without requesting `q`, the keyed asset is not
  in `expression_dependents[q]`. Covers AC-4.
- `failed_expression_run_releases_its_entries`, `unregistered_expression_run_releases_its_entries`,
  `expiring_a_ready_expression_releases_its_entries`, `ready_registered_expression_keeps_its_entries`.
  Cover AC-5 and AC-6.

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib dependencies::tests`, `--lib assets::tests`,
`--test dependency_manager_integration`, then `cargo test -p liquers-core --lib --tests`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. Phase 3 tests. At HEAD the AC-1, AC-4 and AC-5 tests fail, the rest pass. Proof: the
  commands above.
- [ ] 2. `dependencies.rs`: `release_expression`, `expression_entries` (test-only), map doc comments.
  Proof: `cargo test -p liquers-core --lib dependencies::tests`.
- [ ] 3. `assets.rs` `evaluate_recipe`: the run-start `remove_dependent_edges` call (depends on 1).
  Proof: `--test dependency_manager_integration`, and `replaced_run_cannot_restore_a_cleared_edge` still
  passes. Rollback: remove the call.
- [ ] 4. `assets.rs` `finish_run_with_result`, `expire_without_cascade`: `release_expression`
  (depends on 2). Proof: `--lib assets::tests`.
- [ ] 5. Measure: `cargo test -p liquers-core --test dependency_chain_scaling -- --ignored --nocapture`
  before step 3 and after step 4. Record both in the PR. More than 10% slower: stop and take open
  question 3 back to the maintainer.
- [ ] 6. `DEPENDENCIES_STATUS.md` (rules, History, `reviewed:`); close both sources with resolution
  notes. Proof: `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-lib --lib --tests`, the
build-matrix wasm rows, and a diff review for scope and `println!`.

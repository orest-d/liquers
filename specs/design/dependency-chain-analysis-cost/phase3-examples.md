# Phase 3: Examples & Use-cases - Direct dependency records and linear dependency analysis

## High-Level Introduction

The examples follow the Phase 1 purpose in order:
1. **Cost.** A chain of keyed recipes evaluates in roughly quadratic time with a small constant.
2. **Provenance.** Each link records only what it reads, so provenance (`via`) names the real
   predecessor.
3. **Restart.** Transitive freshness after a restart is the dependency manager's job, decided by
   the audit policy.

The pitfalls example covers the behaviours a user can now observe differently. The test plan pins
all of it, including the semantics Phase 2 promises to preserve.

## Example Type

**Conceptual examples with concrete test specifications** (pre-approved by the maintainer). The
feature is internal machinery with no new API for users. The tests below are the executable form,
and Phase 4 writes them.

## Overview Table

| # | Kind | Name | Demonstrates / checks |
|---|---|---|---|
| E1 | example | Chain of keyed recipes | Cost, direct records, `via` = predecessor |
| E2 | example | Restart after an upstream command changes | `Explicit` vs `OnLoad`, audit, the manager's walk |
| E3 | example | Pitfalls | Stale-until-touched under `Explicit`, a recipe without `cwd`, editing `recipes.yaml` |
| U1 | unit, `plan.rs` | `walk_records_direct_dependencies_only` | Link *i*'s list is {`l{i-1}`, its recipe key, `upper` metadata + impl}, nothing upstream |
| U2 | unit, `plan.rs` | `walk_passes_through_evaluate_and_nested_plan` | Anonymous `Evaluate` / `Step::Plan` children appear, relabelled as today |
| U3 | unit, `plan.rs` | `walk_detects_recipe_cycle` | `a → b → a` gives "Circular dependency detected", `with_key`. No such test exists today |
| U4 | unit, `plan.rs` | `walk_detects_self_cycle` | `a → a` |
| U5 | unit, `plan.rs` | `walk_visits_each_recipe_once_on_diamond` | `d → {b, c} → a`: `recipe_opt` called once per key (`CountingRecipeProvider`) |
| U6 | unit, `plan.rs` | `walk_lookups_linear_in_chain_length` | 30-link chain via `CountingRecipeProvider`: exactly 31 lookups per analysis |
| U7 | unit, `plan.rs` | `diamond_expiry_matches_today` | Combined expiry over a diamond equals the old recursive result (`COMBINED-EXPIRES`) |
| U8 | unit, `plan.rs` | `declared_volatile_upstream_marks_plan_volatile` | A volatile recipe two levels up: plan volatile; message names the same key as today |
| U9 | unit, `plan.rs` | `immediate_expiry_upstream_marks_plan_volatile` | The `changed && is_volatile()` gate |
| U10 | unit, `plan.rs` | `walk_error_applies_nothing` | On a cycle, `plan.error` is set; dependencies, volatility and expiry are untouched |
| U11 | unit, `plan.rs` | `memo_respects_caller_cwd_for_recipe_without_cwd` | Same key, two caller cursors, recipe without `cwd`: two results |
| U12 | unit, `recipes.rs` | `recipe_cache_hits_without_reparse` | Two lookups, one parse (parse counter via a test-only hook, or timing-free by asserting the `Arc` is shared) |
| U13 | unit, `recipes.rs` | `recipe_cache_sees_out_of_band_edit` | Rewrite `recipes.yaml` directly in the store (no manager): the next lookup sees the new recipe |
| U14 | unit, `recipes.rs` | `recipe_cache_first_duplicate_wins` | Two recipes with the same filename: `recipe_opt` returns the first, as `RecipeList::get` |
| U15 | unit, `assets.rs` | `stored_state_known_short_circuits` | Manager holds a version: `Known`, no store read |
| U16 | unit, `assets.rs` | `stored_state_confirms_and_registers` | 3-link stored chain, nothing known: `Confirmed`; afterwards the manager knows all three versions and edges |
| U17 | unit, `assets.rs` | `stored_state_detects_deep_command_change` | Stored `l0` recorded `make_text` impl v1, manager has v2: `l2` gives `Stale { dependency: command_impl---make_text }` |
| U18 | unit, `assets.rs` | `stored_state_detects_deep_expired_status` | Stored `l0` `Expired`, `l1` `Ready`: `l2` gives `Stale { dependency: l0 }` |
| U19 | unit, `assets.rs` | `stored_state_record_cycle_is_unresolvable` | Hand-written stored records `a ↔ b`: `Unresolvable`, terminates |
| U20 | unit, `assets.rs` | `stored_state_absent_is_unresolvable` | Recorded key not in the store |
| I1 | integration, `dependency_audit_integration.rs` | `restart_upstream_command_change_explicit_serves_then_cascades` | E2 under `Explicit` |
| I2 | integration, same | `restart_upstream_command_change_on_load_refuses` | E2 under `OnLoad`: `l2` recomputed to the new content |
| I3 | integration, same | `restart_deep_expired_status_on_load_refuses` | Correction 2 under `OnLoad` |
| I4 | integration, same | `audit_catches_deep_upstream_change` | `trigger_dependency_audit_all_registered` after loading `l2`: `l2` expired with `StaleDependency` |
| I5 | integration, same | `cascade_names_true_predecessor` | Evaluated (not hand-written) 10-link chain; expire `l0`; last link's `via` is `l8` |
| I6 | integration, `dependency_chain_scaling.rs` (new) | `chain_evaluation_scales` (`#[ignore]`) | 10/20/40/200 links, times with `eprintln!`; asserts the bounds (40 < 1 s, 200 < 5 s, relaxed to 8 s per decision if missed) |
| I7 | integration, same | `chain_20_links_smoke` | 20 links under 3 s debug (≈3× the predicted), not ignored |
| R1 | changed | `find_dependencies_respects_nested_recipe_cwd` | Assert the link on the recipe's own analysis; outer list has no nested key |
| R2 | changed | `expiration_nested_recipe_uses_keyed_recipe_plan` | Same; the 45 s expiry is unchanged |
| R3 | changed | `volatility_populates_dependencies_once_and_expiration_reuses_them` | Now one call `analyze_plan_dependencies`; `recipe_opt` calls 3 → 1 |
| R4 | changed | six `find_dependencies` call sites | Drop the `stack` argument |
| R5 | regression | `cascade_over_100_link_chain` | Unchanged; may evaluate its chain afterwards (optional) |

## Example 1: Chain of keyed recipes

### Connection to the High-Level Design
This is the issue's own experiment. It shows the cost fix and the provenance fix together.

### Scenario
`data/recipes.yaml` holds `l0.txt = make_text/l0.txt` and, for *i* ≥ 1,
`l{i}.txt = -R/data/l{i-1}.txt/-/upper/l{i}.txt`. These forms were validated with
`liquers-validate --command make_text --command upper`. The links are evaluated one at a time.

### Sequence of Steps
1. Evaluating `-R/data/l{i}.txt` builds the plan, and `analyze_plan_dependencies` walks
   `l{i-1}` → … → `l0`. That is one visit per key: an O(1) recipe-cache hit plus one
   `to_plan_for_key`. The memo and the on-path set make it i+1 visits.
2. The direct list is `{-R/data/l{i-1}.txt, -R-recipe/data/l{i-1}.txt, command_metadata---upper,
   command_impl---upper}`. The summary carries expiry and volatility from upstream.
3. `finalize_plan_expanded` records exactly those four `DependencyRecord`s and registers four edges.
4. Expiring `l0` cascades `l0 → l1 → … → ln` through the direct edges. Every `via` is the
   predecessor.

### Core Example Code
```rust
let envref = chain_env(n).await?;                       // recipes.yaml as above
for i in 0..=n {
    envref.evaluate(&format!("-R/data/l{i}.txt")).await?.get().await?;
}
let meta = envref.get_async_store().get_metadata(&parse_key("data/l5.txt")?).await?;
let keys: Vec<String> = meta.get_dependencies().iter().map(|d| d.key.to_string()).collect();
assert_eq!(keys, ["-R-recipe/data/l4.txt", "-R/data/l4.txt",
                  "ns-dep/command_impl---upper", "ns-dep/command_metadata---upper"]);
```

### Guide and Executable Example
There is no guide (Phase 1). The executable form is I5 and I6.

## Example 2: Restart after an upstream command changes

### Scenario
Process 1 evaluates `l0..l2` with `make_text` impl v1 and persists them. Process 2 starts over a
replay of the store, with `make_text` impl v2, and evaluates `-R/data/l2.txt`.

### Sequence of Steps
- **`Explicit` (default).**
  1. `try_fast_track(l2)` checks its records: `upper` matches, and `l1` is unknown, so it is not
     checked under `Explicit`. `l2` is served with the old content.
  2. Loading `l2` registers its version and the edge `l1 → l2` (the minimum rule).
  3. Later, evaluating `l1` makes `try_fast_track(l1)` see `make_text` v1 ≠ v2, refuse, and
     recompute.
  4. `l1`'s new version is registered, the edge `l1 → l2` mismatches, and `l2` is expired. The next
     `l2` evaluation gives the new content.
  5. Alternatively, `trigger_dependency_audit_all_registered` finds the gap `l1`;
     `stored_dependency_state(l1)` gives `Stale { dependency: command_impl---make_text }`; `l2` is
     expired with `StaleDependency`.
- **`OnLoad`.** `try_fast_track(l2)` finds `l1` unknown and asks
  `stored_dependency_state(l1)`. That reads `l1`'s stored metadata, sees `make_text` v1 against v2,
  and answers `Stale`. `l2` is refused and recomputed, which evaluates `l1`, which recomputes, and
  so on. The result is the new content immediately.

### Core Example Code
```rust
let e2 = second_process_with(&snapshot, &[], calls, DependencyAuditPolicy::OnLoad).await?; // make_text impl v2
let l2 = e2.evaluate("-R/data/l2.txt").await?.get().await?.try_into_string()?;
assert_eq!(l2, "NEW");
```

## Example 3: Pitfalls and edge cases

- **"My upstream change did not show up" under `Explicit`.**
  - *Symptom:* a stored downstream value is served after a restart.
  - *Cause:* the default policy uses only what the manager knows.
  - *Fix:* run `OnLoad` for a strict service, or call an audit at startup. This is a documented
    behaviour change.
- **A recipe provider that leaves `cwd` unset.** Its recipe's relative operands resolve against the
  caller's cursor, so the memo key includes that cursor (U11). Analyses stay correct, with fewer
  memo hits.
- **Editing `recipes.yaml` on disk.** It is seen at the next lookup, because the cache compares the
  bytes (U13). No `clear_cache` is needed. This differs from `ManifestRecipeProvider` folder
  listings.

## Corner Cases

### 1. Memory
- The walk memo lives for one analysis: O(reachable keys).
- The recipe cache holds one `recipes.yaml` copy per directory per provider. `Arc`, not cloned
  per lookup.
- Stored records shrink from O(depth) to O(fan-in).

### 2. Concurrency
- Two analyses run at once: separate walks, nothing shared.
- A recipe-cache race:
  - Two fills of one directory: last writer wins, and both entries are valid for their bytes.
  - A fill racing a write: re-validated against the bytes on the next read.
- `stored_dependency_state` running concurrently with a cascade: the worst case is a confirmation
  that a concurrent expiry then overrides. That is the same tolerance as `audit_gaps` ("the gap
  list is a snapshot").

### 3. Errors
- A recipe cycle gives `general_error` with the key (U3, U4). A store error inside the
  stored-records walk gives `Unresolvable`. Under `OnLoad` that refuses the load, as today.
- In `audit_gaps` an error from `dependency_version` still propagates. A malformed `recipes.yaml`
  is still an error, and is not cached.

### 4. Serialization
No format change. Old records with transitive entries load and are used as they are (Decision 3).

### 5. Integration (Cross-Crate Interactions)
- `liquers-axum` admin audit endpoints: an audit report may now include `StaleDependency` expiries.
  The existing axum tests are run in Phase 4.
- `liquers-records` (`ManifestRecipeProvider` chains with `DefaultRecipeProvider::new()`):
  unchanged behaviour.

## Documentation and Learning Log

### Guide Candidate Workflows and Examples
None: no repeatable user task. Example 2 goes into the reference as the explanation of the audit
policies.

### Usage, Meaning, and Connections
For `DEPENDENCIES_STATUS.md`:
- **Records:** what a dependency record means.
- **Direct:** the definition of "direct".
- **Summary vs record:** how the analysis summary differs from a record.
- **Fast-track loads:** the minimum rule.
- **Freshness:** the dependency manager's walk, and how `Explicit` and `OnLoad` differ for
  transitive freshness.

### Repeatable Development Guidance
None.

### Corrections and Unexpected Learning
- No existing test covered recipe-cycle detection, or a deep upstream change across a restart.
  Both were unguarded behaviour.
- Transitive records were silently doing the work of a transitive freshness check.

## Test Plan

### Unit Tests
U1–U20 in the files named, plus the changed R1–R4. Planner tests use `ImmediateEnvironment` with
`CountingRecipeProvider` (already in `plan.rs` tests). Recipe-cache tests use
`SimpleEnvironment<Value>` over `AsyncMemoryStore`. Asset-manager tests use the existing
`expiry_test_envref()` pattern with hand-written stored metadata.

### Integration Tests
- I1–I5 extend `liquers-core/tests/dependency_audit_integration.rs`. They reuse
  `second_process_with`, `register_counting_commands_in` (extended so `make_text`'s
  `impl_version` and output text are parameters) and `StoreSnapshot`.
- I6–I7 go in a new `liquers-core/tests/dependency_chain_scaling.rs`, which is what the scratch
  benchmark becomes.

### Manual Validation
```bash
cargo test -p liquers-core --lib --tests
cargo test -p liquers-core --test dependency_chain_scaling -- --ignored --nocapture
cargo test -p liquers-axum --tests        # audit endpoints
cargo test -p liquers-records --all-features --lib --tests
```

## Auto-Invoke: liquers-unittest Skill Output

Conventions applied:
- `#[tokio::test]` for async tests, returning `Result<(), Box<dyn std::error::Error>>`;
  `unwrap`/`expect` only in tests.
- No `_ =>` arms when matching `StoredDependencyState`.
- Integration tests in `tests/`, unit tests in the owning module's `mod tests`.
- `parse_key` / `parse_query` for setup; `AsyncMemoryStore::new(&Key::new())`; `eprintln!`, never
  `println!`, for benchmark output.

Templates follow `references/test-patterns.md` (plan and store sections).

## Review Checklist

- [x] Overview table present
- [x] 2–3 scenarios, plus pitfalls
- [x] Corner cases for memory, concurrency, errors, serialization and integration
- [x] Unit and integration tests, error paths included (U3, U4, U10, U19, U20)
- [x] Queries validated (`liquers-validate`, absolute `-R/data/…` form)

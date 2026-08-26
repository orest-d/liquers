# Phase 3: Examples & Use-cases - Predecessor Cut Equivalence

## High-Level Introduction

Phase 1 set the purpose: cutting at the outermost cacheable predecessor becomes the default,
because that is what lets the `AssetManager` cache, share, expire and schedule an intermediate.
Phase 2 said how, and fixed the expanded plan as the **oracle** — the reference implementation
the cut is verified against, not a co-equal shipping form.

That framing decides what this phase produces. The primary deliverable is not a demonstration
but a **verification**: a table-driven suite that evaluates each shape both ways and compares the
result. Everything else exists to make that suite trustworthy — units that pin the mechanisms it
rests on, and corner cases for the three risks Phase 2 recorded rather than resolved.

The scenarios progress accordingly. **Scenario 1** is the workflow the design exists for, a
shared prefix cached across two consumers. **Scenario 2** goes into what Scenario 1 glosses: how
the boundary is placed when a payload or volatility is in the way, and what the author sees when
it moves. **Scenario 3** is the pitfalls set, and none of it is hypothetical — every entry was
measured during Phases 1 and 2.

## Example Type

**Runnable tests, not `examples/*.rs` demos.** `plan-cwd-freeze` Phase 3 settled this and Phase 1
confirmed it: there is no user-facing API here, only internal behaviour to pin, and an equivalence
claim cannot be discharged by a snippet — the suite must execute both forms and compare.
**Confirm at the gate.**

## Overview Table

| # | Type | Name | What it demonstrates or checks |
|---|---|---|---|
| 1 | Example | Shared prefix, two consumers | The Phase 1 purpose: one cut, one cached intermediate, shared |
| 2 | Example | Boundary placement under payload and volatility | Where the boundary lands when the walk steps back, and the `init_info` an author reads |
| 3 | Example | Five pitfalls, each measured | The failures this design removes, with their observed symptoms |
| 4 | Integration | **Equivalence suite, E1-E16 × 3 CWD conditions** | **Primary deliverable**: cut and expanded agree on the result |
| 5 | Unit | `plan.rs` — freeze and prologue (3) | Cause 1, the defect; fails on `main` without any cut |
| 6 | Unit | `plan.rs` — placement walk (5) | Candidate flags, step-back, `Declared` decline, filename exclusion, degenerate guard |
| 7 | Unit | `plan.rs` — `split` and consistency (3) | Field carry by construction; `split_index == predecessor_steps`; invariants |
| 8 | Unit | `recipes.rs` — the fold (3) | `volatile:`/`expires:` reaching the plan, and the preview that under-reported them |
| 9 | Integration | Corner cases (5) | Serde, concurrency, cycle, the fold's dependency-record risk, cross-crate |
| 10 | Fix | Two existing tests corrected | A mis-declared payload; a shape assertion made policy-explicit |

## Example 1: Shared Prefix, Two Consumers

### Connection to the High-Level Design

This is Phase 1's purpose in one picture. An expensive prefix computed once and read by two
queries is precisely what the boundary exists for, and it is invisible without one.

### Scenario

`proj/report.txt` and `proj/summary.txt` both begin by loading and cleaning the same CSV, then
diverge. Expanded, the load-and-clean runs twice. Cut, it runs once and both consumers read the
same asset.

### Sequence of Steps

1. The asset manager is asked for `proj/report.txt` and finds the recipe in `proj/recipes.yaml`.
2. `Recipe::to_plan` builds a source-relative plan and prepends `Step::SetCwd(proj)`.
3. `finalize_plan` freezes, then cuts: the walk finds `-R/./data.csv/-/clean` cacheable and
   replaces the leading steps with one `Step::Evaluate` boundary.
4. `apply_plan` executes; the boundary resolves through `Context::get_dependency_state`, creating
   the intermediate asset.
5. `proj/summary.txt` is requested. Its own cut produces the **same** boundary query, so the
   asset manager serves the existing asset and `clean` does not run again.

### Core Example Code

```rust
// liquers-core/tests/plan_cwd_freeze.rs
#[tokio::test]
async fn a_shared_prefix_is_computed_once() -> Result<(), Box<dyn std::error::Error>> {
    let store = AsyncMemoryStore::new(&Key::new());
    seed(&store, "proj/data.csv", "raw").await?;
    seed_yaml(&store, "proj/recipes.yaml", br#"recipes:
  - query: "-R/./data.csv/-/clean/report/report.txt"
  - query: "-R/./data.csv/-/clean/summarise/summary.txt"
"#).await?;
    let envref = env_with(store)?;

    for name in ["report.txt", "summary.txt"] {
        let asset = envref.evaluate(&format!("-R/proj/{name}")).await?;
        asset.get().await?;
    }

    assert_eq!(clean_calls(), 1, "the shared prefix runs once, not once per consumer");
    Ok(())
}
```

The counter is the assertion that matters. A value comparison would pass either way — which is
the trap Phase 2 recorded about volatility, and the reason this scenario counts calls.

## Example 2: Boundary Placement Under Payload and Volatility

### Connection to the High-Level Design

Scenario 1 leaves the placement rule implicit, because nothing was in the way. This is what
happens when something is, and what the author sees.

### Scenario

A chain `fetch/personalize/render` where `personalize` declares `payload: required`. The
outermost candidate `fetch/personalize` cannot be cached — a payload is not part of a cache key —
so the walk steps back to `fetch`, which can. `fetch` is cached and shared;
`personalize/render` runs inline per payload.

### Sequence of Steps

1. `cut_predecessor` checks `volatility_source`: not `Declared`, so the walk proceeds.
2. Candidate `fetch/personalize`: its plan reports `payload_required`. Passed over, with an
   `init_info` naming the command.
3. Candidate `fetch`: clean. The boundary is cut there.
4. The plan carries a diagnostic an author can read without instrumenting anything.

### Core Example Code

```rust
#[tokio::test]
async fn the_walk_steps_back_past_a_payload() -> Result<(), Box<dyn std::error::Error>> {
    let mut plan = recipe_plan("fetch/personalize/render", Some("proj/a"))?;
    finalize_plan(envref.clone(), &mut plan, &context).await?;

    assert!(plan.cut_predecessor(cmr)?);
    assert!(matches!(&plan.steps[1], Step::Evaluate(q) if q.encode() == "fetch"));
    assert!(plan.init_steps.iter().any(|s| matches!(s, Step::Info(m)
        if m.contains("personalize") && m.contains("payload"))));
    Ok(())
}
```

Volatility behaves identically, because it is the same predicate on the same candidate plan:
`prefix/vol_prefix/tail` steps back to `prefix`. The one difference is `Declared` volatility,
which declines before the walk rather than during it.

## Example 3: Five Pitfalls, Each Measured

Every entry is an observed result from Phase 1 or Phase 2, not a hypothetical.

### Pitfall 1 — a boundary query frozen before the prologue

`Recipe::to_plan` prepends a `SetCwd` the builder never emitted. The step count is compensated;
the cursor is not. Measured on `-R-stored/./input.txt/-/identity/result.txt` with `cwd: programmatic`:

```
expanded: "programmatic"
cut:      Error KeyNotFound: 'input.txt'    <- boundary froze against "", not "programmatic"
```

and, worse because it is silent, on `pass-~X~-R-cwd/./child/-/cwd~E/append_cwd/result.txt`
with `cwd: a/c`:

```
expanded: "a/c/child|a/c"
cut:      "child|a/c"
```

**Corrected by** `prologue_steps` and advancing over it before resolving the predecessor.

### Pitfall 2 — a recipe-level flag is not in the query

Counting prefix executions over two evaluations:

| | expanded | cut |
|---|---|---|
| command-level `volatile: true` | 2 | 2 |
| recipe-level `volatile: true` | 2 | **1** |

**Corrected by** `VolatilitySource::Declared`. The command-level row is the control: it never
diverged, which is why the fix targets scope rather than volatility.

### Pitfall 3 — `v` at the end defeats itself

In `a/b/v` the outermost non-volatile prefix is the entire plan (`predecessor_steps == steps.len()
== 2`, and the existing guard tests `>` not `>=`). Cutting yields `[Evaluate(a/b)]` with an empty
tail: a volatile parent whose whole content is one cached boundary.

**Corrected by** the same `Declared` decline, which makes it unreachable. The degenerate-guard
unit test pins it anyway, because a future positional `v` would reopen it.

### Pitfall 4 — an undeclared payload

`/-/first_cmd/second_cmd/third_cmd` with injected parameters and no `payload: required`. Works
inlined; behind a boundary fails with `Command 'first_cmd' failed: No payload for UserId`.

**Not corrected — declared.** This is the "declare it, or lose it" rule, and the fix is to the
test. E8 pins it as a known inequivalence so the rule stays falsifiable.

### Pitfall 5 — a plan mutated through a subset of coupled fields

Three instances, two shipped: `Recipe::to_plan`'s stale `predecessor_steps`,
`freeze_cwd_with`'s stale cursor, `Plan::split`'s dropped fields.

**Corrected by** building from `self.clone()` and `check_consistent`.

## Unit Tests

### `liquers-core/src/plan.rs` — freeze and the prologue

| Test | Checks |
|---|---|
| `freeze_resolves_predecessor_after_the_recipe_prologue` | Cause 1. Recipe with `cwd: "a/c"` and a relative predecessor; the frozen `predecessor` names `a/c/…`. **Must fail on `main`** — confirm before keeping it |
| `freeze_leaves_predecessor_alone_without_a_prologue` | `prologue_steps == 0` is the unchanged path |
| `prologue_steps_survives_serde_round_trip` | Legacy plan without the field loads at `0` |

### `liquers-core/src/plan.rs` — the placement walk

| Test | Checks |
|---|---|
| `candidate_flags_are_per_prefix` | `prefix/vol/tail/render` — `prefix` clean, `prefix/vol` volatile. The measured basis of the walk |
| `walk_steps_back_past_a_payload_candidate` | Cuts at `fetch` in `fetch/personalize/render`, `init_info` names the command |
| `declared_volatility_declines_before_the_walk` | `v` anywhere, and a `volatile: true` recipe: `Ok(false)`, no boundary, reason recorded |
| `filename_candidate_is_excluded` | `a/b/c/d/out.txt` — the `remainder_is_action == false` candidate is never chosen |
| `degenerate_full_plan_cut_is_declined` | `predecessor_steps == steps.len()` yields `Ok(false)`, not an empty tail. Pins pitfall 3 independently of the `Declared` rule |

### `liquers-core/src/plan.rs` — `split` and consistency

| Test | Checks |
|---|---|
| `split_carries_frozen_cwd_and_clears_predecessor` | Both halves; a clone-based rebuild carries a future field by construction |
| `split_index_equals_predecessor_steps` | Pins the coincidence Phase 2 declined to rely on — two numbers derived by different means |
| `check_consistent_rejects_a_stale_range` | `prologue_steps <= predecessor_steps <= steps.len()`; a stale range is an `Error`, not a panic |

### `liquers-core/src/recipes.rs` — the fold

| Test | Checks |
|---|---|
| `to_plan_folds_recipe_volatility` | `volatile: true` → `plan.is_volatile` **and** `volatility_source == Declared` |
| `to_plan_combines_recipe_expiration` | A finite `expires:` reaches `plan.expires`; `Immediately` also sets `Declared` |
| `finite_expiration_does_not_block_a_cut` | Phase 1's decision, pinned: `expires: in 5 min` still cuts |

## Integration Tests

### The equivalence suite — `liquers-core/tests/plan_cwd_freeze.rs`

Table-driven. `evaluate_both_ways` moves out of `interpreter.rs`'s `#[cfg(test)] mod` and is
widened on two axes.

**Compare the result, not everything.** Value, `is_volatile`, `payload_required`, and the
surfaced error (type, message, position). Per Phase 1, the expanded plan is the oracle: asset
count, dependency edges and metadata differ *by design* and are not compared. This is stated in
the suite's header comment, because the next person to add a shape will otherwise reach for a
metadata assertion and find the feature working.

**Vary the CWD.** Every shape runs three ways, because the present harness always builds a recipe
with no `cwd:` and passes `cwd: None` — so it structurally cannot reach Cause 1, whatever shapes
are added.

| Condition | How | Reaches |
|---|---|---|
| No CWD | `Recipe::new(q, "", "")`, `cwd: None` | today's coverage |
| Recipe CWD | `recipe.cwd = Some("a/c")` | **Cause 1** |
| Provider (keyed) | recipe read from `a/c/recipes.yaml`, evaluated by key | the prologue *and* the keyed-asset path |

**Report, do not stop.** A per-shape result row, so one run prints every divergence. The four
remaining divergences were found in a single forced run of the whole suite; a fail-fast harness
would have surfaced them one release apart.

| # | Shape | Query | Covers |
|---|---|---|---|
| E1 | Pure transform chain | `word/greet-Ciao` | Base case |
| E2 | Resource then action | `-R/./input.csv/-/analyze` | Relative operand freezing |
| E3 | Resource, action, filename | `-R/./x.csv/-/analyze/result.txt` | A filename is not an action |
| E4 | CWD-setting predecessor | `-R-cwd/./sub/-R/./input.csv/-/analyze` | Inner `cwd` under a prologue |
| E5 | Absolute query | `/-R/./input.csv/-/analyze` | Root resolution across the boundary |
| E6 | Volatile command | `vol_counted/vol.txt` | Positional volatility — the control that never diverged |
| E7 | Payload, declared | `word/greet-Ciao` + `payload: required` | Payload crosses inline |
| E8 | Payload, undeclared | same, without the declaration | **Known inequivalence**, pinned |
| E9 | Explicit link parameter | `-R/./x.csv/-/join-~X~-R/data/big.csv~E` | Link scope under freeze |
| E10 | Recipe with link override | `use/result.txt` + link | Overrides stay on the parent's last action |
| E11 | Relative default link | `list_siblings/out.txt` | Promotion |
| E12 | Nested plan | hand-built `Step::Plan` | Shared-cursor rule end to end |
| E13 | Mid-chain payload | `fetch/personalize/render` | Steps back to `fetch`; the deeper candidate is frozen, not left relative |
| E14 | Head payload | `personalize/fetch/render` | No boundary is safe; `was_cut` false, value matches |
| E15 | Recipe-level volatility | `prefix/tail/out.txt` + `volatile: true` | The 2 → 1 divergence; `Declared` declines |
| E16 | `v` instruction | `a/v/b/c` and `a/b/v` | `Declared` declines; no empty tail |

E13 is the one that catches the freeze wrinkle: a stepped-back candidate left CWD-relative
produces a boundary resolving against the wrong folder — Cause 1 reappearing one level down. It
runs under the recipe-CWD condition for that reason.

### Corner cases

| Test | Concern |
|---|---|
| `frozen_plan_survives_serde_round_trip` | A plan serialized without the new fields loads at pre-change defaults |
| `two_folders_race_on_one_boundary_query` | Concurrency — two contexts requesting the same boundary get one asset and one evaluation |
| `self_referential_prefix_is_a_cycle_not_a_recursion` | A cut adds a dependency edge; cycle detection must catch it |
| `volatile_recipe_dependency_records` | **The fold's specific risk.** Folding makes a volatile recipe stop registering plan dependencies, as a volatile plan already does. 19 suites stayed green because nothing asserts this today. This test is the debt Phase 2 recorded |
| `liquers_lib_environment_cuts_via_finalize_plan` | Cross-crate — `liquers-lib`'s `apply_recipe` inherits the change without its own call |

### Corrections to existing tests

| Test | Change |
|---|---|
| `injection::test_chained_commands_with_payload` | `first_cmd` and `third_cmd` gain `payload: required` — a mis-declaration, not a code defect |
| `interpreter::tests::absolute_outer_resource_keeps_relative_link_on_live_cwd` | Two `steps[1]` shape assertions made policy-explicit. Measured: with them relaxed the test passes under the cut, same value and same context CWD |

### Environments

Per the `liquers-unittest` table. `ImmediateEnvironment<Value>` for plan and CWD shapes;
`SimpleEnvironment<Value>` with an `AsyncMemoryStore` for anything with a `-R/` operand or a keyed
recipe (E2-E5, E9-E11, E15, and Examples 1 and 3);
`SimpleEnvironmentWithPayload<Value, String>` for E7, E8, E13, E14. Every `-R/` query runs in an
environment with a store.

## Test Plan

```bash
cargo test -p liquers-core --lib                        # freeze, walk, split, fold units
cargo test -p liquers-core --test plan_cwd_freeze       # the equivalence suite
cargo test -p liquers-core --test recipe_cwd_resolution # the two Cause 1 divergences
cargo test -p liquers-core --test injection             # the declaration fix
cargo test -p liquers-core --tests --no-fail-fast       # the full gate
cargo test -p liquers-lib --lib --tests                 # cross-crate, the CLAUDE.md default loop
```

Baseline to preserve, measured at `d1bd02e`: `liquers-core --tests` green across 19 suites, and
`liquers-lib --lib --tests` exits 0 — both with the prologue fix applied and the cut forced on,
and both again with the recipe fold applied.

Not covered, deliberately: throughput from parallel boundary scheduling, which wants a benchmark
rather than a test (`BENCHMARK-SUITE`), and `liquers-py`, whose `apply_recipe` is `todo!()`.

## Documentation and Learning Log

### Guide candidates

**None**, and this is a confirmation of Phase 1's `neither`, re-tested as the Phase 1 review asked.
The candidate was "how do I read the boundary diagnostics" — and the answer is that the
`init_info` names the command and the reason at the moment the author asks. A guide would restate
the reference. Reconsider if the diagnostics turn out to need interpretation.

### Usage, meaning and connections

- A boundary is a **cache entry**. Payload and volatility placement rules both follow from it.
- Volatility has two **scopes**. Positional (a command) permits a cut in front of it; declared
  (`v`, a recipe flag) forbids one anywhere.
- A recipe-level declaration carries no position, which is why `volatile:` means volatile from the
  *first* action.
- "Equivalent" is the result, not everything: a cut changes asset count and dependency edges by
  design.
- The expanded plan keeps its analysis role structurally — `liquers-validate` builds without
  finalizing, so the cut cannot reach it.

### Repeatable development guidance

- Switching a plan policy on and running the **existing** suite is how this class of defect
  surfaces. All four divergences came from one forced run; the purpose-built harness found none,
  because it holds fixed the axis they live on.
- A scratch `#[cfg(test)] mod` that prints a table, then `git checkout` of the file, establishes a
  fact in minutes and leaves no test behind. The candidate walk, the `v` positions, the
  `split_index` equality and the recipe fold were all established that way.
- Measure before writing the claim down. Several positions in this design were wrong in draft and
  were corrected by a measurement that cost less than the argument preceding it.

### Corrections and unexpected learning

1. **The issue's own diagnosis was wrong.** Not "a nested keyed recipe re-deriving its working
   key" — one missing cursor advance.
2. **The payload position moved three times.** Declare-it, then a blanket decline, then a
   per-candidate walk. The first two made the plan compensate for a missing declaration.
3. **Volatility was overstated, then under-analysed.** Command-level never diverged; only
   recipe-level did. Then `v` showed the real issue was scope, not volatility.
4. **`split_index == predecessor_steps` everywhere**, which made the obvious `Plan::split` fix the
   wrong one.
5. **`v` was checked as a risk and returned an architecture.** It replaced a recipe-only marker
   with a scope distinction covering three declarations.

## Review Record

The host does not permit spawning agents, so the drafting and the three review passes ran
sequentially against the same briefs, per this skill's host-compatibility clause.

**Reviewer 1 — Phase 1 conformity.** The suite is the primary deliverable, as Phase 1 required,
and the oracle framing is carried through: the comparison asserts the result, and what a cut
changes by design is stated as out of scope rather than enumerated. The guide `neither` was
re-tested rather than assumed, as the Phase 1 review asked.

**Reviewer 2 — Phase 2 conformity.** Signatures used match Phase 2:
`cut_predecessor(&mut self, cmr) -> Result<bool, Error>`, `Plan::{prologue_steps,
volatility_source}`, `VolatilitySource::{Positional, Declared}`, `check_consistent() -> Result`.
One gap closed: Phase 2 recorded the fold's dependency-record risk without a test, so
`volatile_recipe_dependency_records` was added to the corner cases.

**Reviewer 3 — codebase and query validation.** All queries checked with `liquers-validate`; no
query contains a space or newline; every `-R/` query runs in an environment with a store;
environments follow the `liquers-unittest` table. `plan_cwd_freeze.rs` already exists with 8 tests
and the `where_am_i` fixture, so the suite extends a file rather than creating one.

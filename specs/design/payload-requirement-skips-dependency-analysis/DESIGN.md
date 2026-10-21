---
id: PAYLOAD-REQUIREMENT-SKIPS-DEPENDENCY-ANALYSIS
kind: design
title: Asking whether a dependency needs a payload builds its plan without walking its upstream
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: not-eligible
area: [core/plan]
issues: [DEPENDENCY-ANALYSIS-REWALKS-UPSTREAM-PER-EVALUATION]
created: 2026-10-20
---
# Asking whether a dependency needs a payload builds its plan without walking its upstream

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20. Phases 1-4 are written but not
approved, and nothing is implemented. This is the first slice of the source feature. The remainder
(caching summaries across evaluations) is filed as
`DEPENDENCY-ANALYSIS-SUMMARIES-NOT-REUSED-ACROSS-EVALUATIONS`.

## Phase 1: High-Level Design

### Purpose

Each evaluation of chain link *i* walks its upstream about three times (the source issue). At HEAD
(18732dd), two of the three walks do work their caller throws away.
`Context::schedule_dependency_asset_with_key` (`liquers-core/src/context.rs`) asks
`query.requires_payload(envref)` for every dependency it schedules, and
`impl RequiresPayload for Query` (`liquers-core/src/interpreter.rs`) answers through `make_plan`.
That is `PlanBuilder::build` **plus** `analyze_plan_dependencies`, a full upstream walk, and then
only `plan.payload_required` is read. That flag is a plan-building fact: `PlanBuilder` sets it,
links included (`check_parameter_for_volatile_links`, `plan.rs`), and the analysis never touches
it. A dependency is scheduled twice per run (the pre-pass `schedule_plan_dependencies` and the step
itself), so two walks per dependency are wasted.

The count fits: the smoke test `chain_20_links_smoke` (`liquers-core/tests/dependency_chain_scaling.rs`)
reports 691 recipe lookups for 20 links. That is 3 × Σᵢ i = 630 walk lookups (one `finalize_plan`
walk and two `requires_payload` walks per link), plus about 3 direct lookups per evaluation.

### Problem Example

```rust
let (envref, calls) = chain_env(20).await?;                 // l0 ← l1 ← … ← l20
for i in 0..=20 { envref.evaluate(&format!("-R/data/l{i}.txt")).await?.get().await?; }
// today: calls ≈ 691        should: ≈ 271 (one upstream walk per evaluation)
```

(`-R/data/l3.txt` validated: `GetAsset[data, l3.txt]`.)

### Scope and Acceptance Criteria

- **AC-1** One upstream walk per evaluation
  WHEN the 20-link chain is evaluated one link at a time
  THEN the recipe lookups are below 350 (691 at HEAD)
- **AC-2** The payload answer is unchanged
  WHEN a query needs a payload through its own command, through a link, or not at all
  THEN `requires_payload` answers as today
- **AC-3** A broken upstream still fails the dependent
  WHEN a dependency's upstream recipe has a cycle
  THEN the dependent's evaluation fails, and its error names the cycle (raised by the dependency's own analysis instead of at scheduling)
- **AC-4** Existing analysis tests unchanged
  WHEN `plan::tests` and `interpreter::tests` run
  THEN they pass

Non-goals: caching summaries across evaluations or within an evaluation session (follow-up issue
above); `Query::is_volatile`, which needs the walk for transitive volatility.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** not-eligible. Rule 2: it is a performance feature rather than a bug fix,
  and AC-3 moves where a broken-upstream error is raised.
- **Leading issue:** None
- **Explanation:** The flag in question is computed before the analysis and the analysis never
  changes it, so the walk can be dropped without a contract change. Only the point where an
  upstream error surfaces moves, to the dependency's own run, which already reports it.
- **Open questions:** None.

### Design Dependencies

- `dependency-chain-analysis-cost` (complete, E4): origin of the source feature.
- `cwd-less-recipe-analysed-at-root`: **overlaps** (weak; it edits `DependencyWalk`, this design
  removes calls into it). No ordering. Its `memo` change would matter only to the follow-up.
- `recipe-preview-shares-plan-analysis`: **overlaps** (weak; another caller of the analysis passes).
- `external-manager-evaluation-session-sharing`: **overlaps** the follow-up only (a session-scoped
  memo would rely on sessions).

## Phase 2: Architecture

### Solution

In `liquers-core/src/interpreter.rs`, add

```rust
/// The plan of `query` as built, without dependency analysis: what a caller needs when it reads
/// only building facts (`payload_required`, steps), never volatility or expiry.
fn build_plan<E: Environment, Q: TryToQuery>(envref: &EnvRef<E>, query: Q) -> Result<Plan, Error>;
```

`build_plan` is `PlanBuilder::new(query.try_to_query()?, envref.get_command_metadata_registry()).build()`.
`make_plan_with_cwd` calls it for its phase 1. `impl RequiresPayload for Query` uses it instead of
`make_plan`. `IsVolatile for Query` keeps `make_plan`.

Rejected: share one memo across the analyses of an evaluation session. That cuts the cold
single-request case only, and the maintainer chose simpler code first; it moves to the follow-up.
Also rejected: cache `payload_required` per query, which a single plan build makes unnecessary.

Known-issue preflight: none open touches `RequiresPayload`.

### Changes

- `interpreter.rs`: private `build_plan`; `make_plan_with_cwd` and `RequiresPayload for Query` use it.
- `liquers-core/tests/dependency_chain_scaling.rs`: `chain_20_links_smoke` bound tightened (AC-1).
- `specs/reference/DEPENDENCIES_STATUS.md` "Analysis summary versus record": one sentence (the
  payload check builds without analysing), History, `reviewed:`.
- No `pub` signature, command or format change.

### Risks

| Risk | Handling | Certainty |
|---|---|---|
| The analysis error of a dependency appears later and wrapped differently | AC-3 test asserts the cycle is named | high |
| Some caller relied on `requires_payload` validating the upstream | only `schedule_dependency_asset_with_key` and `Step::Evaluate` call it; both evaluate the dependency next | high |
| The lookup bound is fragile | 350 leaves ~30% headroom over the expected 271 | medium |

## Phase 3: Examples and Tests

### Examples

The Problem Example is `chain_20_links_smoke`, with its bound tightened.

### Tests

- `liquers-core/tests/dependency_chain_scaling.rs` `chain_20_links_smoke`: `lookups < 350`.
  Covers AC-1.
- `liquers-core/src/interpreter.rs` `tests::query_requires_payload_matches_the_analysed_plan`: for
  three queries (a payload command, a link to one, a plain command), `requires_payload` equals
  `make_plan(..).payload_required`. Covers AC-2.
- `liquers-core/tests/dependency_manager_integration.rs` `dependency_with_cyclic_upstream_fails_the_dependent`:
  recipes `a.txt` = `-R/b.txt/-/upper`, `b.txt` = `-R/a.txt/-/upper`, and `c.txt` = `eval_a`, a
  test command that calls `context.evaluate("-R/a.txt")` in its body (so `c.txt`'s own static walk
  does not see the cycle and only the scheduling path does). Evaluating `c.txt` is an error whose
  message names `a.txt` (at HEAD it comes from `requires_payload`; after, from `a.txt`'s run).
  Queries validated with `--command upper --command eval_a`. Covers AC-3.
- `cargo test -p liquers-core --lib plan::tests interpreter::tests`. Covers AC-4.

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test dependency_chain_scaling --test
dependency_manager_integration` and `--lib interpreter::tests`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. Tests above. AC-1 fails at HEAD (691); AC-2 and AC-3 pass. Proof: the run commands.
- [ ] 2. `interpreter.rs` `build_plan`, used by `make_plan_with_cwd` (no behaviour change). Proof:
  `--lib interpreter::tests`.
- [ ] 3. `RequiresPayload for Query` uses `build_plan`. Proof: all three tests. Rollback: revert
  this one impl.
- [ ] 4. Run the ignored benchmark (`-- --ignored --nocapture`) and record the 200-link time in the
  PR; `DEPENDENCIES_STATUS.md`; note on the source issue that this slice shipped and the remainder
  is the follow-up. Proof: `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests`; diff review.

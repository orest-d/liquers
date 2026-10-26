---
id: INHERITED-VOLATILITY-BOUNDS-PREDECESSOR-CUT
kind: design
title: A predecessor boundary is not cut over a step that reads a volatile upstream
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [core/plan]
issues: [DEPENDENCY-VOLATILITY-INVISIBLE-TO-PREDECESSOR-WALK]
created: 2026-10-26
---
# A predecessor boundary is not cut over a step that reads a volatile upstream

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented.

## Phase 1: High-Level Design

### Purpose

Finalization finds that a plan is volatile because a key it reads has a volatile recipe (or one that
expires `immediately`). It does not record which step does the reading, and `Plan::cut_predecessor`
rebuilds each candidate boundary without dependency analysis, so it cannot see that volatility.
A boundary is then cut over a prefix that reads a volatile key. That boundary is volatile, so the
manager evaluates it as an unregistered volatile asset: an extra asset and an extra hop that cache
nothing. The cut walk should stop in front of the first step that reads a volatile upstream, just as
it already does for a volatile command or `v`.

### Problem Example

Verified by reading `analyze_plan_dependencies` and `Plan::cut_predecessor`
(`liquers-core/src/plan.rs`) at HEAD (cd6d5da). Store recipe `vol.txt` with `volatile: true`;
commands `upper`, `lower`. Query `-R/vol.txt/-/upper/lower` (`liquers-validate --command upper
--command lower`: `Ok`, steps `GetAsset(vol.txt)`, `upper`, `lower`; predecessor
`-R/vol.txt/-/upper`, `predecessor_steps` 2).

- Today: `finalize_plan` marks the plan volatile ("Volatile due to dependency on volatile key"),
  `volatility_source` stays `None`, and the cut produces `[Evaluate(-R/vol.txt/-/upper), lower]`.
  Evaluating it creates a second, volatile, unregistered asset for `-R/vol.txt/-/upper`.
- Should be: the walk sees that step 0 reads a volatile upstream. It steps back past
  `-R/vol.txt/-/upper` and then declines `-R/vol.txt`, which has no predecessor. The plan stays
  expanded, `[GetAsset(vol.txt), upper, lower]`, and one volatile asset is evaluated.

The value is the same either way. The defect is cost.

### Scope and Acceptance Criteria

- **AC-1** No boundary over a volatile read
  WHEN `-R/vol.txt/-/upper/lower` is finalized and `vol.txt`'s recipe is `volatile: true`
  THEN the plan has no `Step::Evaluate`, and an init step records why the boundary at
  `-R/vol.txt/-/upper` was expanded ("it reads a volatile dependency")
- **AC-2** A pure prefix ahead of the read is still cut
  WHEN `-R/stable.txt/-/upper/join-~X~-R/vol.txt~E/lower` is finalized (`stable.txt` stored, not
  volatile; `vol.txt` volatile; the link is read by `join`, step 2)
  THEN the plan is `[Evaluate(-R/stable.txt/-/upper), join…, lower]`, cut in front of `join`
- **AC-3** `immediately` counts as volatile
  WHEN the upstream recipe has `expires: immediately` instead of `volatile: true`
  THEN AC-1 holds
- **AC-4** Unchanged elsewhere
  WHEN a plan has no volatile upstream, or is `Declared` volatile, or contains `v`
  THEN the cut is as today (`positional_v_cuts_before_itself`, `trailing_v_cuts_the_whole_prefix`,
  `walk_steps_back_past_an_uncached_candidate` stay green)
- **AC-5** The rustdoc is true
  WHEN `VolatilitySource` and `cut_predecessor` are read
  THEN they say that volatility inherited from an upstream recipe records no source but bounds the
  cut at the first step that reads it

Non-goals: runtime volatility (a command that evaluates a volatile query through its context,
`runtime-volatility`), which no static analysis can see; recording the position in the serialized
`Plan`; changing what makes an upstream volatile (an upstream volatile *command* still does not,
`upstream_volatile_command_does_not_mark_plan_volatile`).

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — a bug fix in `liquers-core` (`plan.rs`, `interpreter.rs`) that
  makes the cut honour its own documented rule ("walk back while the candidate cannot be cached").
  Only crate-private functions and private fields are added; `Plan`, `cut_predecessor` and
  `finalize_plan` keep their signatures and serialized shape (rules 3-4).
- **Leading issue:** None
- **Explanation:** the walk already knows, per read, what the upstream contributes. Tagging each read
  of the plan being analysed with its top-level step index gives the first volatile position, and
  `finalize_plan` hands it to the cut it performs right afterwards.
- **Open questions:** None. The issue's alternative, accepting the cost and documenting it, is
  rejected because the fix is local and cheap. The other alternative, re-running analysis per
  candidate, is rejected because it costs one upstream walk per step back.

### Design Dependencies

- `payload-requirement-skips-dependency-analysis` — **overlaps**: edits `requires_payload` and the
  walk's callers, not the walk's frames. No order.
- `cwd-less-recipe-analysed-at-root` — **overlaps**: edits `DependencyWalk::visit` (the recipe
  cursor). This design edits `walk_plan`, `run` and `scan_plan`'s root bookkeeping. Small textual
  conflicts are possible, but neither order is required.
- `query-cache-directives`, `context-title-predecessor-inheritance`, `recipe-override-addressing` —
  **overlaps**: each touches `cut_predecessor` (reason texts, inherited fields). This design adds
  one reason branch. Whichever lands second rebases.
- `dependency-summary-reuse` — **overlaps**: a cached summary must still report volatility. It does,
  because only non-volatile summaries are cached there, so positions found by this design are
  unaffected.

### Consolidated Findings

- Position is per **top-level step** of the plan being analysed. Reads from a nested `Step::Plan` or
  `Step::Evaluate`, and from an action's link parameters, take the index of the step that contains
  them. Indices are in `Plan::steps` space, prologue included, which is the space `cut_at` uses.
- The bound only applies in `finalize_plan`, the one place that analyses and then cuts. A caller that
  runs `finalize_plan_expanded` and later calls the public `cut_predecessor` keeps today's
  behaviour. The rustdoc says so. Storing the position in `Plan` would need a new public field or a
  serialized-format change, which this P3 cost fix does not justify.
- If the plan became volatile by inheritance but no position was found (not expected), the bound is
  0, so nothing is cut. This is conservative and correct.
- `RECIPES_AND_PLANS.md` lists this issue under Known limitations, and its §Predecessor boundaries
  conditions gain the bound.

## Phase 2: Architecture

### Solution

All in `liquers-core`.

1. `plan.rs` `PlanScan` gains `read_steps: Vec<usize>`, parallel to `reads`. `walk_plan` scans the
   plan being analysed so that each read is tagged with the index of the top-level step that
   produced it (record `reads.len()` before and after each top-level step; nested `Step::Plan` /
   `Evaluate` reads inherit the outer index). Recipe frames do not need tags.
2. `Frame` (root only) gains `first_volatile_step: Option<usize>`. In `run`, wherever a finished read
   is merged into the **root** frame (both the `Visit::Done(Some(summary))` arm and the frame-pop
   arm), compute what that read contributes to the root summary (`merge_read`'s rule). If the
   contribution makes it volatile (`declared_volatile` set, or `expires.is_volatile()`), keep the
   minimum step index.
3. `walk_plan` returns the root's `first_volatile_step` with the dependencies and summary.
4. New `pub(crate) async fn analyze_plan_dependencies_at(envref, plan, initial_cwd) ->
   Result<Option<usize>, Error>`: today's body. It returns the position when the plan was not volatile
   before analysis and became volatile through it, or `Some(0)` when volatile by inheritance with no
   position found. Otherwise it returns `None`. `analyze_plan_dependencies` becomes a wrapper that
   discards the value, so its callers (recipe preview, tests) are unchanged.
5. `interpreter.rs`: a private `finalize_plan_expanded_at` returns the position.
   `finalize_plan_expanded` (pub) wraps it. `finalize_plan` calls `finalize_plan_expanded_at`, then
   `plan.cut_predecessor_bounded(cmr, bound)` instead of `cut_predecessor`.
6. `plan.rs` `Plan`: `pub(crate) fn cut_predecessor_bounded(&mut self, cmr, volatile_from:
   Option<usize>) -> Result<bool, Error>`. This is today's body with one more reason in the
   step-back loop, checked after payload and before `is_volatile`: `if volatile_from.is_some_and(|v|
   cut_at > v)` → "reads a volatile dependency". `cut_predecessor` (pub) calls it with `None`.
7. Rustdoc: `VolatilitySource::Positional` (inherited volatility records no source but bounds the cut
   made by `finalize_plan`), `cut_predecessor` (the bound, and that the public method does not
   have it).

Explicit matches stay explicit. No `_ =>` arms are added.

### Changes

- `liquers-core/src/plan.rs`: `PlanScan`, `Frame`, `DependencyWalk::walk_plan`, `run`,
  `analyze_plan_dependencies_at` (new, crate), `analyze_plan_dependencies`, `Plan::cut_predecessor`,
  `Plan::cut_predecessor_bounded` (new, crate), rustdoc, tests.
- `liquers-core/src/interpreter.rs`: `finalize_plan_expanded_at` (new, private), `finalize_plan`.
- Documents: `specs/reference/RECIPES_AND_PLANS.md` — §Predecessor boundaries ("Where a boundary
  goes": a candidate whose steps include a read of a volatile upstream is stepped back over), remove
  the Known limitations bullet; History row, `reviewed:` bump.

### Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `plan.rs`, `interpreter.rs`, one reference document |
| Existing tests | cut tests without volatile upstreams are unchanged. `declared_volatile_*` tests still pass (messages unchanged) |
| Compatibility | plans that inherited volatility are cut less (or not at all). Results are equal by the cut-equivalence contract (`predecessor-cut-equivalence`) |
| Performance | one integer comparison per merge into the root frame; fewer assets at run time |
| Concurrency | none: analysis is per plan |
| Recovery | revert |
| Certainty | high for top-level reads. Medium for reads inside an `Evaluate` step of an already-cut plan, which cannot occur in `finalize_plan` because analysis precedes the cut |

## Phase 3: Examples and Tests

### Examples

Validated at HEAD with `liquers-validate --command upper --command lower --command join`:

- `-R/vol.txt/-/upper/lower` — `Ok`, three steps, predecessor `-R/vol.txt/-/upper`.
- `-R/stable.txt/-/upper/join-~X~-R/vol.txt~E/lower` — `Ok`, steps `GetAsset`, `upper`, `join`,
  `lower`, `predecessor_steps` 3.

### Tests

In `liquers-core/src/plan.rs` `mod tests` (using `CountingRecipeProvider` and `walk_env`):

- `inherited_volatility_position_is_the_reading_step` (AC-1, AC-2 at analysis level):
  `analyze_plan_dependencies_at` returns `Some(0)` for `-R/c/vol.txt/-/upper/lower`, and `Some(2)`
  for the link form; `None` for a plan with no volatile upstream.
- `immediately_expiring_upstream_has_a_position` (AC-3): `expires: immediately` returns `Some(0)`.
- `bounded_cut_steps_back_over_a_volatile_read` (AC-1): freeze the plan, call
  `cut_predecessor_bounded(&cmr, Some(0))`. It returns `false`, the steps are unchanged, and an init
  step contains "reads a volatile dependency".
- `bounded_cut_keeps_the_pure_prefix` (AC-2): link form with bound `Some(2)` → `true`, the first
  step is `Evaluate(-R/c/stable.txt/-/upper)`.
- Existing cut tests (AC-4).

In `liquers-core/src/interpreter.rs` `mod tests`:

- `finalize_plan_does_not_cut_over_inherited_volatility` (AC-1 end to end): memory store with
  `recipes.yaml` declaring `vol.txt` volatile; `finalize_plan` on `-R/vol.txt/-/upper/lower`; no
  `Step::Evaluate` in the result.

AC-5 is checked in review (rustdoc).

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib plan::tests interpreter::tests`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. Tests — add the four `plan.rs` tests (against the new crate functions, so they compile only with step 2–3) and `finalize_plan_does_not_cut_over_inherited_volatility` (fails at HEAD) — `cargo test -p liquers-core --lib finalize_plan_does_not_cut`
- [ ] 2. `plan.rs` — `PlanScan::read_steps`, `Frame::first_volatile_step`, `walk_plan` / `run` bookkeeping, `analyze_plan_dependencies_at` + wrapper — depends on 1 — `cargo test -p liquers-core --lib plan::tests`. Rollback: revert.
- [ ] 3. `plan.rs` `cut_predecessor_bounded` + `cut_predecessor` wrapper; `interpreter.rs` `finalize_plan_expanded_at`, `finalize_plan`; rustdoc of `VolatilitySource` and `cut_predecessor` — depends on 2 — all step 1 tests pass; `cargo test -p liquers-core --lib --tests`. Rollback: revert 2-3 together.
- [ ] 4. `RECIPES_AND_PLANS.md` — boundary rule, Known limitations; History row, `reviewed:` — `python3 scripts/docs_index.py --check`
- [ ] 5. Close `DEPENDENCY-VOLATILITY-INVISIBLE-TO-PREDECESSOR-WALK` with a resolution note; this design to `complete` — `python3 scripts/docs_index.py` then `--check`

### Validation

`cargo fmt --check -p liquers-core`; `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests`
(including `plan_cwd_freeze`, `deep_chain_evaluation`, `dependency_chain_scaling`, which exercise
cuts and the walk); `cargo test -p liquers-lib --lib --tests`. Diff review: two source files and one
document.

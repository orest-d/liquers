---
id: CWD-LESS-RECIPE-ANALYSED-AT-ROOT
kind: design
title: Dependency analysis resolves a recipe without cwd against the root, as its run does
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/plan]
issues: [DEPENDENCY-ANALYSIS-USES-CALLER-CWD-FOR-A-CWD-LESS-RECIPE]
created: 2026-10-17
---
# Dependency analysis resolves a recipe without cwd against the root, as its run does

Produced under [`guides/autonomous_bulk_design.md`](../../guides/autonomous_bulk_design.md) by the
2026-10-17 backlog compaction: Phases 1-4, reviewed without phase approval. Not an approval and not
an implementation.

## Phase 1: High-Level Design

### Purpose

`DependencyWalk::visit` (`liquers-core/src/plan.rs`) scans a keyed recipe's plan with the
*reading* plan's cursor (`let mut recipe_cursor = read.cursor.clone()`), and memoizes per
`(key, caller CWD)`. The keyed asset itself runs in a fresh `Context` whose CWD is unset, so
`finalize_plan_expanded` freezes its relative operands against the root. Analysis and run disagree
for a recipe without `cwd` reached from a plan that has one.

### Problem Example

A provider (not `DefaultRecipeProvider`, which always sets `cwd`) serves `lib/r.txt` =
`-R/./s.txt/-/up` with no `cwd`, plus `x/s.txt` (expires in 10 s) and `s.txt` (never expires). A
plan `SetCwd(x)`, `GetAsset(lib/r.txt)` is analysed.

Today: the plan inherits `x/s.txt`'s 10 s expiry, though the run of `lib/r.txt` reads `s.txt`. The
unit test `memo_respects_caller_cwd_for_recipe_without_cwd` pins this. Should be: the plan inherits
`s.txt`'s summary (never expires), the same for every caller CWD.

### Scope and Acceptance Criteria

- **AC-1** Analysis matches the run
  - WHEN a recipe without `cwd` with a relative operand is read from a plan with a CWD
  - THEN the walk resolves that operand against the root, and the plan's expiry, volatility and
    recorded dependencies are those of the root-resolved key
- **AC-2** One analysis per key
  - WHEN the same recipe is read under two caller CWDs in one plan
  - THEN its recipe is looked up once (`recipe_opt` counts one call for it)
- **AC-3** Priming sees every recipe
  - WHEN `upstream_graph` walks a chain through a recipe without `cwd`
  - THEN that recipe and what it reads are recorded as nodes (they are caller-independent now)
- **AC-4** Recipes with `cwd` unchanged
  - WHEN a recipe has its own `cwd`
  - THEN its analysis is as today (its `SetCwd` prologue sets the cursor)

Non-goal: making a dependency's run inherit its caller's CWD.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — the issue names it a contract decision (rule 5) and it
  changes the documented analysis rule in `DEPENDENCIES_STATUS.md`.
- **Leading issue:** **Proposed resolution - which side moves:** the analysis moves to the run
  (root), recommended; the alternative is that a dependency's run inherits the caller's CWD.
- **Explanation:** A keyed asset is one asset per key, shared and stored under that key; letting
  its value depend on who read it first would make the key's value caller-dependent, which the
  asset manager's per-key maps cannot represent. Moving the analysis is local to `DependencyWalk`.
- **Open questions:**
  1. **Proposed resolution - which side moves:** recommended: analyse against the root. Inheriting
     the caller's CWD would need a per-(key, CWD) asset identity and is out of proportion for P3.

### Design Dependencies

- `overlaps` `runtime-volatility` (PR #142; weak: both concern volatility a reader inherits, at
  different times). `overlaps` `INLINE-MANAGER-DYNAMIC-DEPENDENCY-CHAIN-NESTS` (weak: priming
  receives more nodes after AC-3).

### Consolidated Findings

The memo key, the `Frame::own_cwd` field and the `caller_independent` filter in `run` all exist
only because of the caller-cursor rule; all three go. Two tests pin today's behaviour and are
rewritten, not deleted.

## Phase 2: Architecture

### Solution

In `liquers-core/src/plan.rs`:

- `DependencyWalk::visit`: `let mut recipe_cursor = CwdCursor::default();` (root, as the run).
  A recipe with `cwd` still gets its `SetCwd` from `to_plan_for_key`, so AC-4 holds.
- `done: HashMap<Key, Option<DependencySummary>>`; `Frame::key: Option<Key>`; memo lookups by key.
- Remove `Frame::own_cwd` and the `caller_independent` condition in `run`: every finished recipe
  frame is recorded.
- `Read::cursor` stays (it is still the cursor *of the reading plan* used while scanning it), but
  is no longer read by `visit`. Doc comments of `DependencyWalk` and `done` updated.
- `specs/reference/DEPENDENCIES_STATUS.md` "Analysis summary versus record": "keyed by key" (drop
  "and caller CWD"), plus one sentence: a recipe without `cwd` is analysed against the root, as its
  keyed asset runs; History row and `reviewed:`.

Rejected: keep the per-CWD memo and only change the cursor (memo entries would be identical);
inherit the caller CWD at run time (open question 1).

Known-issue preflight: PR #142 (`runtime-volatility`) edits `context.rs`/`assets.rs`, not
`DependencyWalk`; no conflict. Crate-private items only; no `pub` change.

### Changes

`plan.rs`: `DependencyWalk`, `Frame`, `visit`, `run`; tests below. `DEPENDENCIES_STATUS.md`.

### Risks

| Risk | Handling |
|---|---|
| Priming now evaluates cwd-less recipes reached from a CWD | Correct now (analysis = run); covered by AC-3 test on both managers' existing priming tests |
| A provider relying on caller-relative analysis | None in tree; such a provider's runs already ignored the caller CWD |
| The run's first relative operand warns (`RELATIVE_WITHOUT_CWD_WARNING`) | Unchanged; the analysis emits no warning for recipe scans |

Recovery: revert. Certainty: high.

## Phase 3: Examples and Tests

### Examples

The Problem Example; secondary: `d/top.txt` (with `cwd: d`) reads `d/r.txt` (no `cwd`) reads
`d/s.txt` — all three are graph nodes.

### Tests

In `liquers-core/src/plan.rs` `tests`:

- `memo_respects_caller_cwd_for_recipe_without_cwd` → renamed
  `recipe_without_cwd_is_analysed_at_root`: providers `lib/r.txt`, `x/s.txt` (10 s), `y/s.txt`,
  `s.txt` (never); assert `plan.expires == Never` and `recipe_opt_calls() == 2` — AC-1, AC-2
- `upstream_graph_leaves_out_caller_dependent_recipes` → renamed
  `upstream_graph_records_recipes_without_cwd`: nodes contain `d/s.txt` and `d/r.txt` — AC-3
- `recipe_with_cwd_analysis_unchanged`: `r.txt` with `cwd: x` reads `x/s.txt` from a root plan and
  from a `SetCwd(y)` plan — AC-4

Command: `cargo test -p liquers-core --lib plan::tests`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `plan.rs` `visit`, `done`, `Frame`, `run` — `cargo check -p liquers-core`
- [ ] 2. Rewrite and add the three tests — `cargo test -p liquers-core --lib plan::tests`
- [ ] 3. Full core loop (priming, evaluation session) — `cargo test -p liquers-core --lib --tests`
- [ ] 4. `DEPENDENCIES_STATUS.md` rule, History row, `reviewed:`; close the issue — `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests`. Rollback:
revert the commit.

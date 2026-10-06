# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** needs-decision
- **Leading issue:** **Open design question — keep transitive dependency records.** Each link
  records every upstream link. This design found the reason, and recommends keeping it while
  removing the repeated work that makes it expensive.
- **Explanation:** The edge growth the issue observed is confirmed in the code (below). The cost
  attribution (which repeated work dominates) is not yet measured, so Phase 4 begins with a
  benchmark and profile, and then applies the fixes in order of measured effect. Each fix
  preserves the recorded edges, so none changes semantics.
- **Open questions:**
  1. **Proposed resolution — keep transitive records, make them cheap.** Transitive edges let a
     *restarted* process detect a stale link without loading every intermediate. Link *i* records
     the command versions and recipes of everything upstream, so a changed upstream command
     expires it on load (fast-track edge check) even when link *i−1*'s stored metadata was never
     reloaded. Recording direct edges only (the issue's alternative) would lose that unless load
     walked the chain, which moves the cost rather than removing it.
  2. **Open design question — acceptable bound.** Proposed: a 40-link chain evaluates (cold, debug)
     in under 2 s, against 80 s today. Linear or low-constant quadratic growth is measured by
     `chain_evaluation_scales` at 10/20/40 links.

## Problem and evidence

Evaluating a chain of keyed recipes `l{i} = -R/data/l{i-1}.txt/-/upper` takes 0.15 s for 5 links,
6 s for 20 and 80 s for 40 (debug build), roughly the fourth power of the length.

Confirmed by reading `liquers-core/src/plan.rs` `find_dependencies` (≈2589). For each
`GetAsset(key)` step it:

1. adds `key` as a direct dependency;
2. calls `recipe_provider.recipe_opt(key)`, which for `DefaultRecipeProvider` **reads and parses
   `<dir>/recipes.yaml` from the store on every call** (`recipes.rs` ≈765, `get_recipes`). A chain
   whose recipes share one directory parses an O(n)-entry YAML file each time;
3. builds the recipe's plan (`to_plan_for_key`) and **recurses**, extending the result with
   everything upstream (`dependencies.extend(nested_dependencies)`).

So the analysis of link *i* does *i* recursive levels, each paying O(n) for the recipe file. That is
O(i·n) per analysis and O(n³) over the chain, multiplied by however many times a link's plan is
analysed during one evaluation (asset creation, dependency scheduling, fast-track checks: to be
counted in Phase 4 step 1). Each link then registers *i* edges (`enter_dependencies` /
`register_plan_dependencies`), O(n²) edges in total.

## Expected behaviour and acceptance

1. `chain_evaluation_scales` (new, `#[ignore]`d benchmark plus a non-ignored 20-link smoke test with
   a generous bound) shows the 40-link chain within the bound decided in question 2.
2. The dependency records of every link are **identical** before and after (the same set of
   `DependencyRecord` keys; versions as produced by the same evaluation). Semantics unchanged.
3. Cycle detection through recipes still reports `Circular dependency detected` (existing tests).
4. `cascade_over_100_link_chain` keeps passing. It may then evaluate its chain instead of writing it
   by hand (optional follow-up).

## Scope and non-goals

Planning-time dependency analysis and recipe lookup cost. Not in scope: changing which dependencies
are recorded (question 1), or the dependency manager's data structures, unless profiling shows them
dominant. In that case they get their own issue.

## Design Dependencies

- `dependency-edge-superseded-version` — **overlaps**. Both touch dependency recording, with no
  ordering constraint.
- `dependency-audit-and-expiry-provenance` (complete) — **overlaps**. Its restart-time edge checks
  are why transitive records matter.

## Documentation assessment

- Reference: `specs/reference/DEPENDENCIES_STATUS.md`: state that dependency records are
  transitive and why (restart freshness), answering the issue's "or at least a documented reason".
- No guide.

## Consolidated Findings

- The issue's "documented reason for the transitive records" now exists: restart-time freshness.
  Record it in the reference even if nothing else changes.
- Two cheap, semantics-preserving levers exist: (a) memoize `find_dependencies` per resolved key
  within one analysis (and across analyses of one evaluation), and (b) cache parsed `recipes.yaml`
  per directory in `DefaultRecipeProvider`, invalidated by the existing
  `RecipeProvider::directory_changed` hook (`refresh_listing_version` already calls it on every
  write). Which matters more is for the profile to say.

# Phase 1: High-Level Design - Direct dependency records and linear dependency analysis

## Design Readiness

- **Readiness:** not assessed. Phase 1 questions answered 2026-10-07, see Decisions. Phases 2–4 are being rewritten after this revision. The previous
  autonomous draft is in git history.
- **Leading issue:** `EVALUATING-A-LONG-DEPENDENCY-CHAIN-GETS-SUPER-LINEARLY-SLOW`.
- **Decision taken by the maintainer (2026-10-07):** dependency records hold **direct** dependencies
  only. Transitive structure belongs to the dependency manager, which already cascades through the
  direct graph. This reverses the previous draft's "keep transitive records" recommendation.

## Feature Name

Direct dependency records and linear dependency analysis.

## Purpose

Evaluating a chain of *n* keyed recipes costs O(n⁴) today: 40 links take ~90 s in a debug build.
This design records only direct dependencies and analyses each reachable recipe once per analysis.
Transitive freshness is the dependency manager's job, over the direct graph it holds. Together these
make chain evaluation roughly quadratic with a small constant, and preserve which dependencies are
direct, so dependency chains can be debugged.

## Evidence (measured 2026-10-07, cold debug build, one link at a time)

The cost is the product of four factors. Recipe lookups for link *i* are exactly 3·(i+1)².
- **Transitive record set.** `find_dependencies` (`plan.rs:2589`) merges every upstream key into
  the plan's dependencies, so link *i* has i+1 entries.
- **Quadratic re-analysis.** `has_expirable_dependencies_impl` (`plan.rs:2836`) re-runs
  `has_volatile_dependencies`, which is a full recursive walk, for every one of those entries. That
  makes about i² lookups per analysis.
- **Three analyses per evaluation.** `make_plan` from scheduling, `make_plan` from `do_step`, and
  `finalize_plan`.
- **O(n) lookups.** Every `recipe_opt` re-reads and re-parses the whole `recipes.yaml`
  (`recipes.rs:692`).

A throwaway prototype removed the first, second and fourth factors (direct records, one-level
re-analysis, a constant-time recipe index). The 4 failing tests and the restart probe below are
from a separate run with the recipe index reverted.

| Links | HEAD | Prototype |
|---|---|---|
| 40 | ~90 s (71 461 lookups) | **0.56 s** (7 501 lookups) |
| 200 | hours (extrapolated) | **12 s** (181 501 lookups) |

What remains is quadratic: every analysis still walks the whole upstream chain. With direct
records, 993/995 core unit tests and all integration suites pass. The 2 unit-test failures assert
transitive records by design.

**Correctness finding.** A restart probe showed what transitive records currently provide. A chain
`l0 = make_text`, `l1 = upper(l0)`, `l2 = upper(l1)` was persisted; `make_text`'s implementation
version was changed and the process restarted. HEAD recomputes `l2` (it recorded
`command_impl---make_text`). Direct records alone serve the stale `l2`, because `try_fast_track`
(`assets.rs:1321`) checks each record but never recurses, and an unloaded `l1` has no version in
the dependency manager. No existing test covers this. The decisions below make the outcome a
matter of the audit policy.

## Core Interactions

- **Plan / query.** Dependency analysis becomes one memoized walk per analysis. Each reachable
  recipe key is visited once. Cycle detection is depth-first search: an on-path set gives O(1)
  "is this key already on the current path?" checks (`Vec::contains` today is O(depth)), and a
  done set skips keys already analysed. The walk is O(V+E) over the reachable recipe graph. The
  same pass produces the transitive *summaries* (volatility, combined expiry). Those summaries go
  into the plan as analysis results, never as dependency records. "Direct" means the nearest
  addressable dependency: a keyed `GetAsset` stops the record set; anonymous `Evaluate` steps and
  nested plans pass their dependencies through to the enclosing asset.
- **Assets / dependency manager.** Only direct edges are registered and stored. **Any recursive
  walk over dependencies belongs to the dependency manager**, never to the asset or the planner.
  The manager always uses everything it knows at that moment. How complete and how current that
  knowledge is depends on the audit policy. The minimum, required under every policy: when an
  asset is loaded on the fast track with a consistent version, its recorded dependencies are added
  to the manager. That needs no extra metadata scan, and the code already does it
  (`assets.rs:1386` → `load_from_records`; `add_dependency` keeps an edge whose dependency version
  is still unknown). The cascade provides transitivity, and `via` names the true predecessor.
- **Recipes.** `DefaultRecipeProvider` keeps a per-directory parsed and indexed recipe cache,
  checked against the stored `recipes.yaml` bytes on each lookup. Amended in Phase 2 (correction 3):
  a `directory_changed` hook would miss edits made behind Liquers' back.
- **Store, commands, value types, web/UI:** no change. The stored record format is unchanged; only
  its contents shrink.

## Crate Placement

`liquers-core` only (`plan.rs`, `interpreter.rs`, `assets.rs`, `recipes.rs`, `dependencies.rs`).

## Optional optimization, not part of this design: caching transitive summaries across evaluations

The remaining quadratic term is each analysis re-walking its upstream. Estimate from the
prototype, about 65 µs per visited key in debug:
- **Chain of 200 links.** A single walk per analysis would take about 1–4 s, against a floor of
  roughly 1–2 s for the per-link work that does not depend on depth. Not worth caching.
- **Chain of 1 000 links.** About 30–100 s, against a floor of about 5–10 s. Worth caching.
- **Wide fan-in.** For example, 1 000 assets over a shared 500-node upstream: about 500k visits,
  roughly 30 s per cold sweep. Worth caching.

A cache could make the work per evaluation proportional to the direct dependencies only. The
better shape is a cache of per-key *summaries* (volatile, expiry, acyclic), not stored transitive
closures. Even better, read the summary from the dependency's own live plan or asset, whose
invalidation the asset lifecycle already handles (recipe and command keys are recorded
dependencies). Its cost is invalidation correctness on recipe edits. **Opinion: worth doing only
when real deployments show chains deeper than about 300 or large fan-in. Low priority.** It goes
into a follow-up issue in Phase 5 rather than into this scope.

## Documentation Intent

- **Reference:** extend `specs/reference/DEPENDENCIES_STATUS.md`. Records are direct only; how
  transitivity is obtained (cascade; the dependency manager's walk under `OnLoad`); what the
  manager learns on a fast-track load; analysis summaries versus records. Check
  `specs/reference/ASSETS.md`'s fast-track section for what each audit policy guarantees.
- **Guide:** neither. No new repeatable task. Reconsider if the audit policies gain
  configuration.
- **Other:** a follow-up issue for the optional summary cache, filed in Phase 5.
- **Updates:** the issue file's resolution, `specs/README.md` and the index.

## Decisions (maintainer, 2026-10-07)

1. **Transitive freshness is the dependency manager's job, and depends on policy.** No recursive
   check in `try_fast_track`: it makes one call into the dependency-management layer, which does
   the recursion. In Phase 2 this layer is `AssetManager::stored_dependency_state`, because the
   `DependencyManager` struct has no store access. Consequence for the restart probe:
   - **Default policy (`Explicit`).** The stale `l2` is served until `l1` is touched. Then `l1` is
     refused (its recorded `make_text` version differs), recomputed, and registers a new version.
     The cascade then expires `l2` through the edge recorded when `l2` loaded. A changed upstream
     command is treated like a changed upstream data file is today: it is caught when the manager
     learns of it, or by an explicit audit (`trigger_dependency_audit_all_registered`). The audit catches it
     only because it uses the dependency manager's walk over stored records (amended in
     Phase 2: today's audit compares only stored versions).
   - **`OnLoad` policy.** The manager resolves the unknown dependencies of the asset being loaded
     by walking the stored dependency records recursively, and remembers what it confirms. That
     gives the same guarantee HEAD gets today from the transitive records. Phase 2 specifies this
     walk as a dependency-manager operation.
2. **A missing intermediate.** Keep today's policy semantics (`Explicit` serves, `OnLoad` refuses).
3. **No migration.** Records already stored with transitive entries are left as they are.
4. **Simpler correct code over optimization.** Each analysis is made linear in place. The three
   analyses per evaluation are not merged unless that is the simpler code.
5. **Acceptance bound.** Debug, cold: 40 links under 1 s, 200 links under 5 s. If 200 links misses
   5 s, the bound is relaxed to 8 s (Phase 2, Decision 2).

## Design Dependencies

- `dependency-audit-and-expiry-provenance` (complete), **overlaps**. Its Phase 2 already chose
  "direct dependencies only; transitivity comes from the cascade" (`phase2-architecture.md:144`).
  This design makes the records match that decision.
- `dependency-edge-superseded-version`, **overlaps** (dependency recording). No ordering
  constraint.

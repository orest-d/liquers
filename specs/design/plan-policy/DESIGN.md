---
id: PLAN-POLICY
kind: design
title: Caching strategies for recipes and queries, an inline-running command flag, positional volatility, and retirement of the plan-builder policy markers
workflow: liquers-project
status: complete
area: [core/plan, core/assets, core/commands, core/query, core/context]
issues: [CORE-PLAN-POLICY-AND-DEFAULTS, V-INSTRUCTION-IS-WHOLE-PLAN-NOT-POSITIONAL]
merged: 2026-10-10
gh_pr: [102]
affects_docs: [specs/reference/ENVIRONMENT_CONFIG.md, specs/guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md, specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md, specs/reference/api/DOC_08_RECIPES_PLANS.md, specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md, specs/reference/PROJECT_OVERVIEW.md, specs/reference/COMMAND_DECLARATION.md, specs/reference/REGISTER_COMMAND_FSD.md, specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md, specs/reference/ASSETS.md, specs/guides/COMMAND_REGISTRATION_GUIDE.md, specs/guides/COMMAND_DESIGN_GUIDE.md, specs/guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md]
created: 2026-10-10
---
# Plan Policy Design Tracking

**Created:** 2026-10-10

Owns the remainder of `CORE-PLAN-POLICY-AND-DEFAULTS` after `predecessor-cut-equivalence`
settled its `expand_predecessors` half, plus `V-INSTRUCTION-IS-WHOLE-PLAN-NOT-POSITIONAL`,
merged on 2026-10-10 at the user's request.

## Phase Status

- [x] Phase 1: High-Level Design — **approved 2026-10-10**
- [x] Phase 2: Architecture — **approved 2026-10-10**

Pre-approved after Phase 2 on 2026-10-10 (`proceed all`): Phases 3 and 4, implementation and Phase 5.

- [x] Phase 3: Examples and Tests — reviewed 2026-10-10 (pre-approved)
- [x] Phase 4: Implementation Plan — reviewed 2026-10-10 (pre-approved; decision log: nothing blocking, nothing needs a decision)
- [x] Implementation — Phase 4 Steps 1–10, 2026-10-10
- [x] Phase 5: Documentation — **complete 2026-10-10** (PR #102 green)

## Notes

- 2026-10-10: started as a compact design (source complexity `M`). After the Phase 1 discussion
  the scope grew to `L` (see Phase 1, Scope Changes), so the design was converted to the full form.
- User decisions recorded on 2026-10-10: directive syntax `stored-<bool>` / `cached-<bool>`; positional
  `v` folded in; flags plus a log entry are enough to explain a missing value (no reason field).
- 2026-10-10, measured rather than read. Each cut boundary is evaluated as its own query asset,
  whose plan is finalized and cut again, so **every prefix of a chain becomes a cached asset**.
  With counting commands, after `seed/t1/t2/t1` was fetched through `AssetManager::get_asset`,
  `seed/t1/t2`, `seed/t1` and `seed` were all served without recomputation (counters unchanged).
  So `DOC_08_RECIPES_PLANS.md` ("One cut retains **one** intermediate") is wrong at HEAD, and a
  long chain over a large value retains one copy per step. The free function
  `interpreter::evaluate` recomputed every prefix, which is consistent with it bypassing the asset
  cache. The probe was a temporary test and is not committed.
- 2026-10-10, second discussion round. The user keeps the "disable cutting" switch as a debugging
  aid (checking whether an uncut plan gives a different result). Agreed that `stored` is meaningful
  only for keyed assets and stays recipe-only: no command keyword, no directive. Under assessment: a
  positional cache switch (`cache-off` / `cache-on` / `cache-this`) instead of per-value
  `cached-<bool>`.
- 2026-10-10, third round. Deferred the in-query cache directive and filed it as
  `QUERY-CANNOT-MARK-CACHED-INTERMEDIATES` with the assessment, so no retention directive is in
  scope. Decided:
  - A recipe whose `cached` excludes intermediates creates no intermediate assets, but reuses one
    that already exists. This differs from an expanded plan, which ignores existing ones.
  - The meaning of the recipe's `cached` beyond a boolean is under discussion (`none` / `result` /
    `all`).
  - Command-level `cached` stays: an uncached command's output is never a boundary, so the plan
    runs it inline (`a/b/c/d` with `c` uncached gives `Evaluate(a/b) c d`).
  - The default stays `on` / `all`, which is today's behaviour. The disable-cutting switch is kept
    for debugging.
  - This supersedes the `stored-<bool>` / `cached-<bool>` directive decision recorded above.
- 2026-10-10, fourth round. A keyed result follows only the effective recipe strategy, not the
  last command's flag. The strategy settings are named `recipe_cache_strategy` and
  `query_cache_strategy`. A recipe's `cached:` also accepts `default`, meaning the global recipe
  strategy; the global settings have no `default`. A size limit on cached data is a requirement for
  `CORE-ASSET-GC` and is noted there. Strategy propagation into boundaries was reopened by the
  user's web-service case: guests run ad-hoc queries with a restricted cache, while recipes are
  "approved" queries whose intermediates should be cached.
- 2026-10-10, fifth round. Decided: the strategy follows the origin. A boundary uses its creator's
  strategy, an existing intermediate is reused by everyone, and the command flag only restricts.
  The `assets:` keys are `recipe_cache_strategy`, `query_cache_strategy` and `cut_predecessors`.
  The size limit stays in `CORE-ASSET-GC`. Phase 1 rewritten on this model.
- 2026-10-10, sixth round. The strategy is a property of the asset, set at construction and read
  by its context. The plan is independent of the strategy and of the manager's state: reuse
  happens when a boundary step executes, and a missing boundary is evaluated unregistered under
  `result` / `none`. `a/b/v` caches `a/b` and yields a volatile, unmanaged asset; this changes the
  meaning of an existing trailing or mid-chain `v`. The documentation plan now covers configuration,
  the environment construction guide, and the API references for plan, command metadata, recipes
  and the asset manager.

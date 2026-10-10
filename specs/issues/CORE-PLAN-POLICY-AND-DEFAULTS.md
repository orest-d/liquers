---
id: CORE-PLAN-POLICY-AND-DEFAULTS
kind: issue
title: Plan builder has no configuration and questionable defaults
status: closed
priority: P2
complexity: L
area: [core/plan]
design: plan-policy
created: 2026-08-08
github:
---
## Problem

`liquers-core/src/plan.rs:899-901` records three unsupported policies — `cache`, `volatile flags`,
`inline flag` — and `:909` says `expand_predecessors: true, // TODO: expand_predecessors should be
false by default`.

## Impact

Behaviour that ought to be a caller's choice is compiled in, and one default is documented as
wrong. `CORE-RECIPES-EXPAND-PREDECESSORS-CRASH` is the same default crashing a test.

## Update, 2026-08-16 (`plan-cwd-freeze`)

The `expand_predecessors` half of this issue has moved. The flag and its two builder methods are
gone: `PlanBuilder` always expands, and cutting a predecessor into a `Step::Evaluate` boundary is
now `Plan::cut_predecessor`, applied after freezing. So the question is no longer "what should the
builder's default be" but "when should a plan be cut", which is a policy about a plan
transformation rather than a builder setting.

`CORE-RECIPES-EXPAND-PREDECESSORS-CRASH` no longer blocks that decision — it is closed. Two things
now bear on it:

- **`PREDECESSOR-CUT-NOT-YET-EQUIVALENT`** (P1) — cutting is not yet observably equivalent to
  expanding. Four divergences remain, one of which is a test asserting the expanded shape.
- **The trade is per query, not global.** Cutting buys dependency management, caching with
  independent expiration, and parallel scheduling for an intermediate; it costs retaining that
  intermediate in the asset manager. A large intermediate used once is better inlined; a slow prefix
  shared by many consumers is much better cut. That argues against a single global default as much
  as it argues for cutting. See `DOC_08_RECIPES_PLANS.md`, "Predecessor boundaries".

The `cache`, `volatile flags` and `inline flag` markers at `plan.rs:899-901` are untouched.

## Update, 2026-08-26 (`predecessor-cut-equivalence`)

**The `expand_predecessors` question is answered.** Cutting at the outermost cacheable
predecessor is now the default: `finalize_plan` calls `Plan::cut_predecessor` after freezing and
after the analysis passes. It is not a global on/off — the cut is placed per plan, at the last
candidate prefix that can be cached, and declines where none can be.

That also settles the "per query, not global" argument recorded above. It was reasoning about
cutting *everywhere*, which retains every intermediate; one cut retains one, and the memory
counterweight belongs to an asset-manager retention policy (`CORE-ASSET-GC`) rather than to the
shape of a plan. `DOC_08_RECIPES_PLANS.md` is updated accordingly.

`PREDECESSOR-CUT-NOT-YET-EQUIVALENT` is closed.

**What remains here:** the `cache`, `volatile flags` and `inline flag` markers at
`plan.rs:899-901`, untouched.

## Update, 2026-10-10 (`plan-step-state-metadata`)

One more cut policy: `Plan::cut_predecessor` declines a boundary when the prefix, apart from
`SetCwd`, is a single key read — the asset at that key is already cached under its key, so the
boundary added nothing but a keyless query asset. Each `Step::Action` now records the prefix query
it completes (`Step::Action::query`), frozen like `Plan::predecessor`; a future configurable cut
policy can read candidate boundary queries from the steps instead of rebuilding candidate plans.
See `reference/api/DOC_08_RECIPES_PLANS.md` §Predecessor boundaries.

## Update, 2026-10-10 (`plan-policy`)

Design `plan-policy` owns the remaining markers:

- `cache` becomes the caching strategies `none` / `result` / `all`: a recipe's `cached:`, plus the
  `assets.recipe_cache_strategy` and `assets.query_cache_strategy` defaults.
- `inline flag` becomes a command's `cached: false`: its output is never a boundary, so the command
  runs inline.
- `volatile flags` was already covered, apart from positional `v`, which is merged in from
  `V-INSTRUCTION-IS-WHOLE-PLAN-NOT-POSITIONAL`.

Complexity re-evaluated `M` → `L`.

## Expected behaviour

A `PlanBuilderConfig` carrying these policies, with the defaults chosen deliberately and stated.

## Discovery

Migration triage, 2026-08-08. Source: `todo20260219.md` #8, work package WP-7. Verified against HEAD: markers present at `plan.rs:899-901` and `:909`. See `specs/archive/2026-08-08-docs-migration-plan.md` §4.0c.

## Resolution

Closed 2026-10-10 by `design/plan-policy/`. The three markers are replaced by builder documentation
(`builder_policy_markers_are_retired` pins their absence), and each policy is now a stated choice:

- `cache` → `CacheStrategy` (`none` / `result` / `all`) per asset: a recipe's `cached:` or
  `assets.recipe_cache_strategy`, `assets.query_cache_strategy` for ad-hoc queries, and the
  creator's strategy for a dependency.
- `inline flag` → a command's `cached: false` (`Plan::uncached_by`): the boundary walk steps back
  past it, so the command runs inline.
- `volatile flags` → already covered, plus positional `v`.
- The debugging switch `assets.cut_predecessors`.

All defaults equal the previous behaviour. Evidence: `liquers-core/tests/cache_strategy.rs` (18
tests on both managers) and the unit tests in `cache_strategy.rs`, `plan.rs`,
`environment_config.rs` and `environment_builder.rs`. Reference: `ASSETS.md` §When an asset is
kept for reuse, `DOC_08_RECIPES_PLANS.md` §Predecessor boundaries, `ENVIRONMENT_CONFIG.md`.

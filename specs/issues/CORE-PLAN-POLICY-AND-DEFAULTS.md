---
id: CORE-PLAN-POLICY-AND-DEFAULTS
kind: issue
title: Plan builder has no configuration and questionable defaults
status: accepted
priority: P2
complexity: M
area: [core/plan]
design: 
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

## Expected behaviour

A `PlanBuilderConfig` carrying these policies, with the defaults chosen deliberately and stated.

## Discovery

Migration triage, 2026-08-08. Source: `todo20260219.md` #8, work package WP-7. Verified against HEAD: markers present at `plan.rs:899-901` and `:909`. See `specs/archive/2026-08-08-docs-migration-plan.md` §4.0c.

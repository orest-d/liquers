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

## Expected behaviour

A `PlanBuilderConfig` carrying these policies, with the defaults chosen deliberately and stated.

## Discovery

Migration triage, 2026-08-08. Source: `todo20260219.md` #8, work package WP-7. Verified against HEAD: markers present at `plan.rs:899-901` and `:909`. See `specs/archive/2026-08-08-docs-migration-plan.md` §4.0c.

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

## Expected behaviour

A `PlanBuilderConfig` carrying these policies, with the defaults chosen deliberately and stated.

## Discovery

Migration triage, 2026-08-08. Source: `todo20260219.md` #8, work package WP-7. Verified against HEAD: markers present at `plan.rs:899-901` and `:909`. See `specs/archive/2026-08-08-docs-migration-plan.md` §4.0c.

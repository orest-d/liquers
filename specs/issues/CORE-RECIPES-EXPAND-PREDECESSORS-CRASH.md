---
id: CORE-RECIPES-EXPAND-PREDECESSORS-CRASH
kind: issue
title: `disable_expand_predecessors` crashes an evaluation test
status: closed
priority: P0
complexity: M
area: [core/assets, core/plan]
design: plan-cwd-freeze
created: 2026-08-08
github:
---
## Resolution

Resolved by `specs/design/plan-cwd-freeze/`. The option is **removed**, which the issue allowed as
one of its two acceptable outcomes — but not because the crash was unfixable. There was no crash.

Enabling `disable_expand_predecessors()` produced 11 test failures from four causes. The named one
was the least serious: the test's `word` command omitted `payload: required`, so the payload did
not reach it across an evaluation boundary — the documented "declare it, or lose it" rule, fixed by
declaring it. The others were structural, and diagnosing them showed the option was a symptom
rather than the problem: CWD-relative operands were resolved by three independent passes, each with
its own cursor.

`Plan::freeze_cwd` now resolves them once, before dependency analysis. `PlanBuilder` always expands
and records what a boundary cut would need; `Plan::cut_predecessor` performs the cut on a frozen
plan. So the capability the option was reaching for exists, in a place where it can be correct —
`PlanBuilder` had no entry CWD and could not have been.

Cutting remains off by default. Divergences between cutting and expanding are down from 11 to 4
(one of which asserts the expanded plan shape and is not a defect) and are tracked in
`PREDECESSOR-CUT-NOT-YET-EQUIVALENT`; `CORE-PLAN-POLICY-AND-DEFAULTS` owns the default itself.

## Problem

`liquers-core/src/recipes.rs:157` keeps a call commented out:
`// .disable_expand_predecessors(); // TODO: fix - evaluate_immediately unittest is crashing with
this option`.

## Impact

A supported plan-builder option cannot be used from the recipe path, and the reason is a crash
nobody has diagnosed. The commented-out line is the only record that the option is broken.

## Expected behaviour

Either the crash is fixed and the call restored, or the option is removed. A permanently
commented-out call with a TODO is neither.

## Discovery

Migration triage, 2026-08-08. Source: `todo20260219.md` #15, work package WP-7. Verified against HEAD: the commented call is still at `recipes.rs:157`. See `specs/archive/2026-08-08-docs-migration-plan.md` §4.0c.
